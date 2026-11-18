import { describe, expect, it } from 'vitest';
import fixture from '../../../server/src/truenas/testdata/app_event.json';
import {
	APP_DETAIL_SELECT,
	appMounts,
	containerStateLabel,
	detailQuery,
	parseAppDetail,
	parsePorts,
	portLabel,
	portalUrl,
	sortContainers,
	type Container
} from './app-detail';
import { APP_SELECT } from './apps';

const container = (
	service: string,
	state: Container['state'],
	mounts: Container['mounts'] = []
) => ({
	id: service,
	service,
	image: null,
	state,
	mounts
});

describe('detailQuery', () => {
	it('asks for one app by id with the list fields and the detail ones', () => {
		expect(detailQuery('plex')).toEqual([
			[['id', '=', 'plex']],
			{ select: [...APP_DETAIL_SELECT] }
		]);
		for (const f of APP_SELECT) expect(APP_DETAIL_SELECT).toContain(f);
		expect(APP_DETAIL_SELECT).not.toContain('metadata');
		expect(APP_DETAIL_SELECT).not.toContain('active_workloads');
	});
});

describe('parseAppDetail', () => {
	it('reads the TrueNAS entry', () => {
		const d = parseAppDetail(fixture, 'nas.local')!;
		expect(d.app).toMatchObject({ id: 'jellyfin', state: 'running', version: '10.11.2' });
		expect(d.description).toMatch(/^Jellyfin is a Free Software Media System/);
		expect(d.portals).toEqual([{ name: 'Web UI', url: 'http://nas.local:30013/' }]);
		expect(d.containers.map((c) => [c.service, c.state])).toEqual([
			['jellyfin', 'running'],
			['permissions', 'exited']
		]);
		expect(d.containers[0]).toMatchObject({
			id: '5c3e1d2b9f8a7e6d5c4b3a291807f6e5d4c3b2a1908f7e6d5c4b3a2910f8e7d6',
			image: 'jellyfin/jellyfin:10.11.2'
		});
		expect(d.containers[0].mounts[0]).toEqual({
			source: '/mnt/tank/media',
			destination: '/media',
			readOnly: true
		});
		expect(d.ports).toEqual([{ hostPorts: [30013], containerPort: 8096, protocol: 'tcp' }]);
	});

	it('reads a stopped app with no workloads, portals or metadata', () => {
		const raw = { ...fixture, state: 'STOPPED', active_workloads: undefined, portals: {} };
		delete (raw as Record<string, unknown>).metadata;
		const d = parseAppDetail(raw, 'nas')!;
		expect(d.app.state).toBe('stopped');
		expect(d).toMatchObject({ description: null, portals: [], containers: [], ports: [] });
	});

	it('keeps the reads of a dotted select that returned the whole objects', () => {
		const d = parseAppDetail({ ...fixture, active_workloads: { containers: 0 } }, 'nas')!;
		expect(d.containers).toEqual([]);
		expect(d.ports).toEqual([]);
	});

	it('drops unreadable containers and mounts, and refuses what the list refuses', () => {
		const raw = {
			...fixture,
			active_workloads: {
				container_details: [
					7,
					{ service_name: 'no-id' },
					{ id: 'abcdef0123456789', state: 'RESTARTING', volume_mounts: [{ source: '/a' }, 3] }
				]
			}
		};
		const [only] = parseAppDetail(raw, 'nas')!.containers;
		expect(only).toEqual({
			id: 'abcdef0123456789',
			service: 'abcdef012345',
			image: null,
			state: 'unknown',
			mounts: []
		});
		expect(parseAppDetail({ id: 'x' }, 'nas')).toBeNull();
		expect(parseAppDetail(null, 'nas')).toBeNull();
	});
});

describe('portalUrl', () => {
	it('puts the NAS name in place of a bind-all address', () => {
		expect(portalUrl('http://0.0.0.0:30013/', 'nas.tail')).toBe('http://nas.tail:30013/');
		expect(portalUrl('http://[::]:30013/web', 'nas.tail')).toBe('http://nas.tail:30013/web');
		expect(portalUrl('https://0.0.0.0:443/', '192.168.1.5')).toBe('https://192.168.1.5/');
	});

	it('keeps a real host', () => {
		expect(portalUrl('http://192.168.1.9:8080/', 'nas')).toBe('http://192.168.1.9:8080/');
	});

	it('refuses anything but http(s)', () => {
		expect(portalUrl('javascript:alert(1)', 'nas')).toBeNull();
		expect(portalUrl('ftp://0.0.0.0/', 'nas')).toBeNull();
		expect(portalUrl('not a url', 'nas')).toBeNull();
		expect(portalUrl(42, 'nas')).toBeNull();
	});
});

describe('parsePorts', () => {
	it('groups host ports per container port and protocol, once each', () => {
		const ports = parsePorts([
			{ container_port: 53, protocol: 'udp', host_ports: [{ host_port: 53, host_ip: '::' }] },
			{
				container_port: 53,
				protocol: 'tcp',
				host_ports: [
					{ host_port: 5353, host_ip: '0.0.0.0' },
					{ host_port: 53, host_ip: '0.0.0.0' },
					{ host_port: 53, host_ip: '::' }
				]
			},
			{ container_port: 80, protocol: 'tcp', host_ports: [] },
			{ container_port: 0, protocol: 'tcp' },
			{ container_port: 443, host_ports: [{ host_port: 'x' }, { host_port: 70000 }] },
			'junk'
		]);
		expect(ports).toEqual([
			{ hostPorts: [53, 5353], containerPort: 53, protocol: 'tcp' },
			{ hostPorts: [53], containerPort: 53, protocol: 'udp' },
			{ hostPorts: [], containerPort: 80, protocol: 'tcp' },
			{ hostPorts: [], containerPort: 443, protocol: 'tcp' }
		]);
		expect(parsePorts(null)).toEqual([]);
	});

	it('reads as host → container', () => {
		expect(portLabel({ hostPorts: [53, 5353], containerPort: 53, protocol: 'tcp' })).toBe(
			'53, 5353 → 53/tcp'
		);
		expect(portLabel({ hostPorts: [], containerPort: 80, protocol: 'tcp' })).toBe('80/tcp');
	});
});

describe('containers', () => {
	it('sort with attention first, then by service', () => {
		const sorted = sortContainers([
			container('web', 'running'),
			container('db', 'exited'),
			container('worker', 'crashed'),
			container('init', 'starting')
		]);
		expect(sorted.map((c) => c.service)).toEqual(['worker', 'init', 'db', 'web']);
	});

	it('read in words', () => {
		expect(containerStateLabel('running')).toEqual({
			label: 'Running',
			tone: 'success',
			dot: true
		});
		expect(containerStateLabel('crashed').tone).toBe('destructive');
		expect(containerStateLabel('exited').label).toBe('Stopped');
		expect(containerStateLabel('created').label).toBe('Not started');
	});

	it('share folders once, read-only only when every mount is', () => {
		const ro = { source: '/mnt/tank/media', destination: '/media', readOnly: true };
		const rw = { source: '/mnt/cfg', destination: '/config', readOnly: false };
		const mounts = appMounts([
			container('a', 'running', [ro, rw]),
			container('b', 'exited', [{ ...rw, readOnly: true }, ro]),
			container('c', 'exited', [{ ...ro, readOnly: false, destination: '/m' }])
		]);
		expect(mounts).toEqual([rw, { ...ro, readOnly: false, destination: '/m' }, ro]);
	});
});

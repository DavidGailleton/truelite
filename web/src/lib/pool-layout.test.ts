import { describe, expect, it } from 'vitest';
import {
	deviceProblem,
	diskCount,
	hasProblem,
	layoutSummary,
	topologyNeedsLook,
	vdevLabel
} from './pool-layout';
import { parseTopologies, parseTopology, parseVdev, type Topology, type Vdev } from './pools';

// The shape the server sends (`truenas/pools.rs` `compact_topology`).
const disk = (name: string, status = 'ONLINE', errors = {}) => ({
	name: `${name}1`,
	type: 'DISK',
	status,
	disk: name,
	errors: { read: 0, write: 0, checksum: 0, ...errors },
	children: []
});
const vdev = (name: string, type: string, children: unknown[], status = 'ONLINE') => ({
	name,
	type,
	status,
	errors: { read: 0, write: 0, checksum: 0 },
	children
});
const empty = { data: [], log: [], cache: [], spare: [], special: [], dedup: [] };
const topo = (t: Record<string, unknown[]>): Topology => parseTopology({ ...empty, ...t })!;
const one = (raw: unknown): Vdev => parseVdev(raw)!;

describe('parseTopology', () => {
	it('reads nested vdevs and disks', () => {
		const t = topo({
			data: [vdev('raidz1-0', 'RAIDZ1', [disk('sda'), disk('sdb', 'faulted', { read: 3 })])]
		});
		expect(t.data[0].type).toBe('RAIDZ1');
		expect(t.data[0].children[1]).toEqual({
			name: 'sdb1',
			type: 'DISK',
			status: 'FAULTED',
			disk: 'sdb',
			errors: { read: 3, write: 0, checksum: 0 },
			children: []
		});
		expect(t.cache).toEqual([]);
	});

	it('is defensive about missing and odd fields', () => {
		expect(parseTopology(null)).toBeNull();
		expect(parseTopology([1])).toBeNull();
		const t = parseTopology({ data: [{ name: 'x' }, 5, { type: 'DISK' }], log: 'no' })!;
		expect(t.data).toEqual([
			{
				name: 'x',
				type: 'UNKNOWN',
				status: 'UNKNOWN',
				disk: null,
				errors: { read: 0, write: 0, checksum: 0 },
				children: []
			}
		]);
		expect(t.log).toEqual([]);
		expect(one({ name: 'a', errors: { read: -2, write: 1.7, checksum: 'x' } }).errors).toEqual({
			read: 0,
			write: 1,
			checksum: 0
		});
	});

	it('maps a pool.query result by id and skips pools without one', () => {
		const m = parseTopologies([
			{ id: 1, topology: empty },
			{ id: 2, topology: null },
			{ id: 'x', topology: empty },
			7
		])!;
		expect([...m.keys()]).toEqual([1]);
		expect(parseTopologies({})).toBeNull();
	});
});

describe('vdevLabel', () => {
	it('names layouts with how many disks can fail', () => {
		const mirror = one(vdev('m', 'MIRROR', [disk('a'), disk('b')]));
		expect(vdevLabel(mirror, 'data')).toEqual({
			text: 'Mirror · 2 disks · 1 can fail',
			risky: false
		});
		const z2 = one(
			vdev(
				'z',
				'RAIDZ2',
				['a', 'b', 'c', 'd'].map((n) => disk(n))
			)
		);
		expect(vdevLabel(z2, 'data').text).toBe('RAIDZ2 · 4 disks · 2 can fail');
		const dr = one(
			vdev(
				'd',
				'DRAID1',
				['a', 'b', 'c'].map((n) => disk(n))
			)
		);
		expect(vdevLabel(dr, 'data').text).toBe('dRAID1 · 3 disks · 1 can fail');
	});

	it('counts a disk being replaced once', () => {
		const m = one(
			vdev('m', 'MIRROR', [
				vdev('r', 'REPLACING', [disk('old', 'OFFLINE'), disk('new')]),
				disk('b')
			])
		);
		expect(vdevLabel(m, 'data').text).toBe('Mirror · 2 disks · 1 can fail');
		expect(vdevLabel(m.children[0], 'data').text).toBe('Replacing a disk');
	});

	it('flags a lone data disk, not a lone cache or spare', () => {
		expect(vdevLabel(one(disk('a')), 'data')).toEqual({
			text: 'Single disk · no redundancy',
			risky: true
		});
		expect(vdevLabel(one(disk('a')), 'cache').risky).toBe(false);
		expect(vdevLabel(one(disk('a')), 'spare').risky).toBe(false);
	});
});

describe('device problems', () => {
	it('says nothing about a healthy device or a ready spare', () => {
		expect(deviceProblem(one(disk('a')))).toBeNull();
		expect(deviceProblem(one(disk('a', 'AVAIL')))).toBeNull();
	});

	it('names the state and the errors', () => {
		expect(deviceProblem(one(disk('a', 'UNAVAIL')))).toBe('Missing');
		expect(deviceProblem(one(disk('a', 'ONLINE', { read: 1, checksum: 12 })))).toBe(
			'Online · 1 read error · 12 checksum errors'
		);
	});

	it('finds a problem deep in the tree', () => {
		const m = one(vdev('m', 'MIRROR', [disk('a'), disk('b', 'ONLINE', { write: 2 })]));
		expect(deviceProblem(m)).toBeNull();
		expect(hasProblem(m)).toBe(true);
	});
});

describe('pool layout', () => {
	it('summarises the data layout', () => {
		const mirrors = topo({
			data: [
				vdev('m0', 'MIRROR', [disk('a'), disk('b')]),
				vdev('m1', 'MIRROR', [disk('c'), disk('d')])
			],
			cache: [disk('nvme0n1')]
		});
		expect(diskCount(mirrors)).toBe(5);
		expect(layoutSummary(mirrors)).toBe('5 disks · 2 × Mirror');
		expect(
			layoutSummary(topo({ data: [vdev('z', 'RAIDZ1', [disk('a'), disk('b'), disk('c')])] }))
		).toBe('3 disks · RAIDZ1');
		expect(layoutSummary(topo({ data: [disk('a'), disk('b')] }))).toBe('2 disks · no redundancy');
		expect(
			layoutSummary(topo({ data: [vdev('m', 'MIRROR', [disk('a'), disk('b')]), disk('c')] }))
		).toBe('3 disks · mixed layout');
		expect(layoutSummary(topo({}))).toBe('0 disks');
	});

	it('needs a look for a failing disk or unprotected data only', () => {
		const ok = topo({ data: [vdev('m', 'MIRROR', [disk('a'), disk('b')])], cache: [disk('c')] });
		expect(topologyNeedsLook(ok)).toBe(false);
		expect(topologyNeedsLook(topo({ data: [disk('a')] }))).toBe(true);
		const bad = topo({
			data: [vdev('m', 'MIRROR', [disk('a'), disk('b', 'FAULTED')], 'DEGRADED')]
		});
		expect(topologyNeedsLook(bad)).toBe(true);
	});
});

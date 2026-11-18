import { describe, expect, it } from 'vitest';
import {
	coresLabel,
	formatUptime,
	hardwareLabel,
	parseSystemInfo,
	refetchOnUp,
	versionLabel
} from './system';
import type { Status } from './ws';

const GiB = 1024 ** 3;

// The documented `system.info` result (api.truenas.com v25.10), as the middleware sends it.
const raw = {
	version: '25.10.8',
	buildtime: { $date: 1762300000000 },
	hostname: 'nas',
	physmem: 64 * GiB,
	model: 'AMD Ryzen 5 5600G with Radeon Graphics  ',
	cores: 12,
	physical_cores: 6,
	loadavg: [0.4, 0.3, 0.2],
	uptime: '12 days, 3:04:05',
	uptime_seconds: 1047845.5,
	system_serial: null,
	system_product: 'B550M DS3H',
	system_product_version: null,
	license: null,
	boottime: { $date: 1762400000000 },
	datetime: { $date: 1763447845500 },
	timezone: 'Europe/Paris',
	system_manufacturer: 'Gigabyte Technology Co., Ltd.',
	ecc_memory: false
};

describe('parseSystemInfo', () => {
	it('reads the documented result', () => {
		expect(parseSystemInfo(raw)).toEqual({
			hostname: 'nas',
			version: '25.10.8',
			uptimeSeconds: 1047845.5,
			bootTime: new Date(1762400000000),
			cpu: 'AMD Ryzen 5 5600G with Radeon Graphics',
			cores: 12,
			physicalCores: 6,
			memory: 64 * GiB,
			ecc: false,
			hardware: 'Gigabyte Technology Co., Ltd. B550M DS3H',
			timezone: 'Europe/Paris'
		});
	});

	it('accepts ISO dates and missing optional fields', () => {
		const info = parseSystemInfo({
			...raw,
			boottime: '2026-11-06T10:00:00Z',
			physical_cores: undefined,
			system_manufacturer: null,
			system_product: null,
			timezone: undefined,
			ecc_memory: undefined
		});
		expect(info?.bootTime?.toISOString()).toBe('2026-11-06T10:00:00.000Z');
		expect(info?.physicalCores).toBeNull();
		expect(info?.hardware).toBeNull();
		expect(info?.timezone).toBeNull();
		expect(info?.ecc).toBe(false);
	});

	it('gives no boot time for an unreadable date', () => {
		expect(parseSystemInfo({ ...raw, boottime: 'soon' })?.bootTime).toBeNull();
		expect(parseSystemInfo({ ...raw, boottime: null })?.bootTime).toBeNull();
	});

	it('rejects another shape', () => {
		expect(parseSystemInfo(null)).toBeNull();
		expect(parseSystemInfo([raw])).toBeNull();
		for (const key of ['hostname', 'version', 'uptime_seconds', 'model', 'cores', 'physmem']) {
			expect(parseSystemInfo({ ...raw, [key]: undefined }), key).toBeNull();
		}
		expect(parseSystemInfo({ ...raw, uptime_seconds: '12' })).toBeNull();
		expect(parseSystemInfo({ ...raw, cores: Number.NaN })).toBeNull();
	});
});

describe('versionLabel', () => {
	it('names the product once', () => {
		expect(versionLabel('25.10.8')).toBe('TrueNAS 25.10.8');
		expect(versionLabel('TrueNAS-SCALE-25.10.8')).toBe('TrueNAS 25.10.8');
		expect(versionLabel('TrueNAS-25.10.8')).toBe('TrueNAS 25.10.8');
	});
});

describe('formatUptime', () => {
	it.each([
		[-5, 'just started'],
		[0, 'just started'],
		[59.9, 'just started'],
		[60, 'up 1 min'],
		[3599, 'up 59 min'],
		[3600, 'up 1 h'],
		[3600 + 20 * 60, 'up 1 h 20 min'],
		[86399, 'up 23 h 59 min'],
		[86400, 'up 1 day'],
		[86400 + 4 * 3600 + 59, 'up 1 day 4 h'],
		[3 * 86400 + 3600, 'up 3 days 1 h'],
		[7 * 86400 - 1, 'up 6 days 23 h'],
		[7 * 86400, 'up 7 days'],
		[400 * 86400 + 5000, 'up 400 days']
	])('%s s → %s', (s, text) => {
		expect(formatUptime(s)).toBe(text);
	});
});

describe('hardwareLabel', () => {
	it('drops firmware placeholders', () => {
		expect(hardwareLabel('To Be Filled By O.E.M.', 'To Be Filled By O.E.M.')).toBeNull();
		expect(hardwareLabel('System manufacturer', 'System Product Name')).toBeNull();
		expect(hardwareLabel('Default string', ' ')).toBeNull();
		expect(hardwareLabel(null, null)).toBeNull();
		expect(hardwareLabel('iXsystems', null)).toBe('iXsystems');
	});

	it('does not repeat the brand', () => {
		expect(hardwareLabel('Dell Inc.', 'Dell Inc. PowerEdge T140')).toBe('Dell Inc. PowerEdge T140');
		expect(hardwareLabel('HP', 'ProLiant MicroServer Gen10')).toBe('HP ProLiant MicroServer Gen10');
	});
});

describe('coresLabel', () => {
	it('mentions threads only when they differ', () => {
		expect(coresLabel(12, 6)).toBe('12 threads, 6 cores');
		expect(coresLabel(4, 4)).toBe('4 cores');
		expect(coresLabel(1, null)).toBe('1 core');
	});
});

describe('refetchOnUp', () => {
	it('fetches on the first up and after every drop', () => {
		let calls = 0;
		const feed = refetchOnUp(() => calls++);
		const seq: Status[] = ['connecting', 'up', 'up', 'nas-reconnecting', 'up', 'offline'];
		for (const s of seq) feed(s);
		expect(calls).toBe(2);
		feed('connecting');
		feed('up');
		expect(calls).toBe(3);
	});
});

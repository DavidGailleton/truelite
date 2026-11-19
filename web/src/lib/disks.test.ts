import { describe, expect, it } from 'vitest';
import {
	aggParams,
	diskGroups,
	diskKind,
	diskQueryParams,
	disksByName,
	disksHref,
	disksSummary,
	formatTemp,
	parseDisk,
	parseDisks,
	parseTempAgg,
	parseTemps,
	tempLevel,
	type Disk
} from './disks';
import { parseTopology, type Topology } from './pools';

const disk = (name: string, more: Partial<Disk> = {}): Disk => ({
	name,
	model: null,
	serial: null,
	size: null,
	rotation: null,
	type: null,
	bus: null,
	pool: null,
	description: null,
	...more
});

// The shape the server sends (`truenas/pools.rs` `compact_topology`).
const node = (name: string, status = 'ONLINE', errors = {}) => ({
	name: `${name}1`,
	type: 'DISK',
	status,
	disk: name,
	errors: { read: 0, write: 0, checksum: 0, ...errors },
	children: []
});
const empty = { data: [], log: [], cache: [], spare: [], special: [], dedup: [] };
const topo = (t: Record<string, unknown[]>): Topology => parseTopology({ ...empty, ...t })!;

describe('diskQueryParams', () => {
	it('asks for pools and only the shown fields', () => {
		const [filters, options] = diskQueryParams() as [unknown, { select: string[]; extra: object }];
		expect(filters).toEqual([]);
		expect(options.extra).toEqual({ pools: true });
		expect(options.select).toContain('rotationrate');
		expect(options.select).not.toContain('passwd');
	});

	it('asks for 7 days of history', () => {
		expect(aggParams(['sda'])).toEqual([['sda'], 7]);
	});
});

describe('parseDisk', () => {
	it('reads a full entry', () => {
		expect(
			parseDisk({
				name: 'sda',
				model: 'ST4000VN008',
				serial: 'ZA1',
				size: 4e12,
				rotationrate: 7200,
				type: 'hdd',
				bus: 'ATA',
				pool: 'tank',
				description: ' Left bay '
			})
		).toEqual({
			name: 'sda',
			model: 'ST4000VN008',
			serial: 'ZA1',
			size: 4e12,
			rotation: 7200,
			type: 'HDD',
			bus: 'ATA',
			pool: 'tank',
			description: 'Left bay'
		});
	});

	it('reads missing or odd fields as null', () => {
		expect(parseDisk({ name: 'sdb', size: -1, rotationrate: 0, model: '', pool: 3 })).toEqual(
			disk('sdb')
		);
	});

	it('refuses entries without a name', () => {
		expect(parseDisk({ model: 'x' })).toBeNull();
		expect(parseDisk('sda')).toBeNull();
		expect(parseDisks([{ name: 'sda' }, null, { name: '' }])?.map((d) => d.name)).toEqual(['sda']);
		expect(parseDisks({})).toBeNull();
	});
});

describe('parseTemps and parseTempAgg', () => {
	it('reads the reshaped temperatures', () => {
		const t = parseTemps({ sda: { temp: 36, crit: 60 }, sdb: { temp: null, crit: 0 }, sdc: 3 });
		expect(t?.get('sda')).toEqual({ temp: 36, crit: 60 });
		expect(t?.get('sdb')).toEqual({ temp: null, crit: null });
		expect(t?.has('sdc')).toBe(false);
		expect(parseTemps([])).toBeNull();
		expect(parseTemps(null)).toBeNull();
	});

	it('reads the 7-day ranges, skipping empty ones', () => {
		const r = parseTempAgg({ sda: { min: 30, max: 41, avg: 35 }, sdb: { min: null, max: null } });
		expect(r?.get('sda')).toEqual({ min: 30, max: 41 });
		expect(r?.has('sdb')).toBe(false);
		expect(parseTempAgg('x')).toBeNull();
	});
});

describe('diskKind', () => {
	it('names the kind in plain words', () => {
		expect(diskKind(disk('nvme0n1'))).toBe('NVMe SSD');
		expect(diskKind(disk('sda', { rotation: 5400, type: 'HDD' }))).toBe('HDD · 5400 rpm');
		expect(diskKind(disk('sdb', { type: 'SSD' }))).toBe('SSD');
		expect(diskKind(disk('sdc', { type: 'HDD' }))).toBe('HDD');
		expect(diskKind(disk('sdd'))).toBeNull();
	});
});

describe('tempLevel', () => {
	it('compares with the critical limit', () => {
		expect(tempLevel({ temp: 60, crit: 60 })).toBe('hot');
		expect(tempLevel({ temp: 55, crit: 60 })).toBe('warm');
		expect(tempLevel({ temp: 54.9, crit: 60 })).toBe('ok');
		expect(tempLevel({ temp: 70, crit: null })).toBe('ok');
		expect(tempLevel({ temp: null, crit: 60 })).toBe('ok');
		expect(tempLevel(null)).toBe('ok');
	});

	it('formats whole degrees', () => {
		expect(formatTemp(36.6)).toBe('37 °C');
	});
});

describe('disksByName', () => {
	it('finds disks in nested vdevs and every group', () => {
		const t = topo({
			data: [
				{
					name: 'raidz1-0',
					type: 'RAIDZ1',
					status: 'DEGRADED',
					errors: {},
					children: [
						node('sda'),
						{
							name: 'replacing-1',
							type: 'REPLACING',
							status: 'DEGRADED',
							errors: {},
							children: [node('sdb', 'UNAVAIL'), node('sdc')]
						}
					]
				}
			],
			spare: [node('sdd', 'AVAIL')]
		});
		const m = disksByName([t]);
		expect([...m.keys()].sort()).toEqual(['sda', 'sdb', 'sdc', 'sdd']);
		expect(m.get('sdb')?.status).toBe('UNAVAIL');
	});
});

describe('diskGroups', () => {
	const disks = [
		disk('sdb', { pool: 'tank' }),
		disk('sda', { pool: 'tank' }),
		disk('sdc', { pool: 'tank' }),
		disk('sdd', { pool: 'boot-pool' }),
		disk('sde'),
		disk('sdf', { pool: 'archive' }),
		disk('sdg', { pool: 'backup' })
	];

	it('orders pools, then the boot pool, then disks in no pool', () => {
		const groups = diskGroups(disks, null, null, null);
		expect(groups.map((g) => g.title)).toEqual([
			'archive',
			'backup',
			'tank',
			'Boot pool',
			'Not in a pool'
		]);
		expect(groups[2].rows.map((r) => r.disk.name)).toEqual(['sda', 'sdb', 'sdc']);
	});

	it('puts what needs a look first', () => {
		const temps = new Map([
			['sdc', { temp: 61, crit: 60 }],
			['sda', { temp: 57, crit: 60 }],
			['sde', { temp: 30, crit: 60 }]
		]);
		const devices = disksByName([topo({ data: [node('sdb', 'ONLINE', { read: 2 })] })]);
		const ranges = new Map([['sda', { min: 30, max: 50 }]]);
		const groups = diskGroups(disks, temps, ranges, devices);
		expect(groups[0].title).toBe('tank');
		const rows = groups[0].rows;
		expect(rows.map((r) => [r.disk.name, r.level, r.problem])).toEqual([
			['sdc', 'hot', null],
			['sdb', 'ok', 'Online · 2 read errors'],
			['sda', 'warm', null]
		]);
		expect(rows[2].range).toEqual({ min: 30, max: 50 });
		expect(disksSummary(groups)).toBe('7 disks · 1 too hot · 2 need a look');
	});

	it('sorts names by their numbers', () => {
		const groups = diskGroups([disk('sdaa'), disk('sdb'), disk('sda')], null, null, null);
		expect(groups[0].rows.map((r) => r.disk.name)).toEqual(['sda', 'sdaa', 'sdb']);
		expect(disksSummary(diskGroups([disk('sda')], null, null, null))).toBe('1 disk');
	});
});

describe('disksHref', () => {
	it('links to the page or to a disk on it', () => {
		expect(disksHref()).toBe('/storage/disks');
		expect(disksHref('sda')).toBe('/storage/disks#disk-sda');
	});
});

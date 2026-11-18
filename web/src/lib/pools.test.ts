import { describe, expect, it } from 'vitest';
import {
	applyPoolEvent,
	applyScan,
	parsePool,
	parsePools,
	parseRootUsage,
	parseScan,
	poolHealth,
	poolRows,
	rootUsageParams,
	scanLabel,
	sortPools,
	usage,
	type Pool
} from './pools';

const TiB = 1024 ** 4;
const NOW = Date.UTC(2026, 10, 18, 12, 0, 0);
const HOUR = 3600_000;

// A `pool.query` entry with the selected fields, as the server forwards it (fixture of
// server/src/truenas/pools.rs).
const raw = {
	id: 1,
	name: 'tank',
	status: 'ONLINE',
	healthy: true,
	warning: false,
	status_code: 'OK',
	status_detail: null,
	size: 16 * TiB,
	allocated: 6 * TiB,
	free: 10 * TiB,
	scan: {
		function: 'SCRUB',
		state: 'FINISHED',
		start_time: { $date: NOW - 3 * 24 * HOUR },
		end_time: { $date: NOW - 3 * 24 * HOUR + 3 * HOUR },
		percentage: 100,
		bytes_to_process: 6 * TiB,
		bytes_processed: 6 * TiB,
		bytes_issued: 6 * TiB,
		pause: null,
		errors: 0,
		total_secs_left: null
	}
};

const pool = (over: Partial<Pool> = {}): Pool => ({ ...parsePool(raw)!, ...over });

describe('parsePool', () => {
	it('reads the selected fields', () => {
		expect(parsePool(raw)).toEqual({
			id: 1,
			name: 'tank',
			status: 'ONLINE',
			healthy: true,
			warning: false,
			statusCode: 'OK',
			statusDetail: null,
			size: 16 * TiB,
			allocated: 6 * TiB,
			scan: {
				kind: 'scrub',
				state: 'finished',
				percent: 100,
				end: new Date(NOW - 3 * 24 * HOUR + 3 * HOUR),
				paused: null,
				errors: 0,
				secondsLeft: null
			}
		});
	});

	it('needs an id, a name and a status', () => {
		expect(parsePool({ ...raw, id: '1' })).toBeNull();
		expect(parsePool({ ...raw, name: undefined })).toBeNull();
		expect(parsePool({ ...raw, status: null })).toBeNull();
		expect(parsePool(null)).toBeNull();
	});

	it('takes a missing scan or space as unknown', () => {
		const p = parsePool({ id: 2, name: 'x', status: 'online' })!;
		expect(p).toMatchObject({ status: 'ONLINE', healthy: true, scan: null, size: null });
	});

	it('drops unreadable entries from a list', () => {
		expect(parsePools([raw, { id: 'x' }])?.map((p) => p.name)).toEqual(['tank']);
		expect(parsePools({})).toBeNull();
	});
});

describe('parseScan', () => {
	it('reads ISO dates from pool.scan events', () => {
		const s = parseScan({
			function: 'RESILVER',
			state: 'SCANNING',
			end_time: null,
			percentage: 42.4,
			pause: '2026-11-18T11:00:00Z',
			errors: 2,
			total_secs_left: 3600
		});
		expect(s).toEqual({
			kind: 'resilver',
			state: 'running',
			percent: 42.4,
			end: null,
			paused: new Date('2026-11-18T11:00:00Z'),
			errors: 2,
			secondsLeft: 3600
		});
	});

	it('refuses unknown functions and states', () => {
		expect(parseScan({ ...raw.scan, function: 'TRIM' })).toBeNull();
		expect(parseScan({ ...raw.scan, state: 'X' })).toBeNull();
		expect(parseScan(null)).toBeNull();
	});
});

describe('root usage', () => {
	it('reads parsed or raw values of root datasets only', () => {
		const u = parseRootUsage([
			{ id: 'tank', used: { parsed: 4 * TiB }, available: { parsed: 6 * TiB } },
			{ id: 'fast', used: { rawvalue: '100' }, available: { rawvalue: '300', parsed: null } },
			{ id: 'tank/media', used: { parsed: 1 }, available: { parsed: 1 } },
			{ id: 'bad', used: {}, available: { parsed: 1 } }
		]);
		expect([...u]).toEqual([
			['tank', { used: 4 * TiB, available: 6 * TiB }],
			['fast', { used: 100, available: 300 }]
		]);
		expect(parseRootUsage(null).size).toBe(0);
	});

	it('asks only for the root datasets and two properties', () => {
		expect(rootUsageParams(['tank'])).toEqual([
			[['id', 'in', ['tank']]],
			{
				select: ['id', 'used', 'available'],
				extra: { retrieve_children: false, properties: ['used', 'available'] }
			}
		]);
	});

	it('prefers usable space and falls back to raw space', () => {
		expect(usage(pool(), { used: 4 * TiB, available: 6 * TiB })).toEqual({
			used: 4 * TiB,
			total: 10 * TiB,
			percent: 40,
			raw: false
		});
		expect(usage(pool())).toEqual({ used: 6 * TiB, total: 16 * TiB, percent: 37.5, raw: true });
		expect(usage(pool({ size: null }))).toBeNull();
		expect(usage(pool({ size: 0, allocated: 0 }))).toBeNull();
	});
});

describe('poolHealth', () => {
	const level = (p: Pool, pct: number | null = 10) => poolHealth(p, pct);

	it('is healthy when nothing is wrong', () => {
		expect(level(pool())).toEqual({ level: 'ok', label: 'Healthy', detail: null });
	});

	it('names each problem', () => {
		expect(level(pool({ status: 'DEGRADED' }))).toMatchObject({
			level: 'warning',
			label: 'Degraded'
		});
		for (const status of ['FAULTED', 'UNAVAIL', 'OFFLINE', 'REMOVED', 'SUSPENDED']) {
			expect(level(pool({ status }))).toMatchObject({ level: 'critical', label: 'Not available' });
		}
		expect(level(pool({ healthy: false, statusDetail: 'One or more devices has errors' }))).toEqual(
			{ level: 'warning', label: 'Errors found', detail: 'One or more devices has errors' }
		);
		expect(level(pool({ warning: true, statusCode: 'CORRUPT_DATA' }))).toMatchObject({
			level: 'warning',
			label: 'Needs attention'
		});
		expect(level(pool({ status: 'WEIRD' }))).toMatchObject({ level: 'warning' });
	});

	it('ignores feature upgrades', () => {
		expect(level(pool({ warning: true, statusCode: 'FEAT_DISABLED' })).level).toBe('ok');
	});

	it('warns while rebuilding', () => {
		const scan = { ...pool().scan!, kind: 'resilver' as const, state: 'running' as const };
		expect(level(pool({ scan }))).toMatchObject({ level: 'warning', label: 'Rebuilding' });
		expect(level(pool({ scan: { ...scan, state: 'finished' } })).level).toBe('ok');
	});

	it('warns from 80% and alarms from 90%', () => {
		expect(level(pool(), 79.9).level).toBe('ok');
		expect(level(pool(), 80)).toMatchObject({ level: 'warning', label: 'Getting full' });
		expect(level(pool(), 90)).toMatchObject({ level: 'critical', label: 'Almost full' });
		expect(level(pool(), null).level).toBe('ok');
	});

	it('shows the worst problem, the first one on a tie', () => {
		expect(level(pool({ status: 'DEGRADED' }), 95).label).toBe('Almost full');
		expect(level(pool({ status: 'FAULTED' }), 95).label).toBe('Not available');
		expect(level(pool({ status: 'DEGRADED' }), 85).label).toBe('Degraded');
	});
});

describe('sortPools', () => {
	it('puts the worst first, then sorts by name', () => {
		const row = (name: string, lvl: 'ok' | 'warning' | 'critical') => ({
			name,
			health: { level: lvl, label: '', detail: null }
		});
		const sorted = sortPools([
			row('b', 'ok'),
			row('a', 'ok'),
			row('z', 'warning'),
			row('y', 'critical')
		]);
		expect(sorted.map((r) => r.name)).toEqual(['y', 'z', 'a', 'b']);
	});
});

describe('applyPoolEvent', () => {
	const list = [pool(), pool({ id: 2, name: 'fast' })];

	it('adds, changes and removes by id', () => {
		const added = applyPoolEvent(list, {
			kind: 'added',
			id: 3,
			fields: { ...raw, id: 3, name: 'new' }
		});
		expect(added?.map((p) => p.name)).toEqual(['tank', 'fast', 'new']);
		const changed = applyPoolEvent(list, {
			kind: 'changed',
			id: 1,
			fields: { ...raw, status: 'DEGRADED' }
		});
		expect(changed?.map((p) => p.status)).toEqual(['DEGRADED', 'ONLINE']);
		expect(applyPoolEvent(list, { kind: 'removed', id: 1 })?.map((p) => p.id)).toEqual([2]);
	});

	it('asks for a query again when the event is unreadable', () => {
		expect(applyPoolEvent(list, { kind: 'changed', id: 1, fields: { id: 1 } })).toBeNull();
		expect(applyPoolEvent(list, { kind: 'changed', id: '1', fields: raw })).toBeNull();
		expect(applyPoolEvent(list, { kind: 'changed', id: 2, fields: raw })).toBeNull();
		expect(applyPoolEvent(list, { kind: 'other', id: 1 })).toBeNull();
	});
});

describe('applyScan', () => {
	const list = [pool()];
	const running = { ...raw.scan, state: 'SCANNING', percentage: 42, end_time: null };

	it('updates the named pool', () => {
		const next = applyScan(list, { name: 'tank', scan: running });
		expect(next[0].scan?.state).toBe('running');
		expect(list[0].scan?.state).toBe('finished');
	});

	it('ignores unknown pools and odd payloads', () => {
		expect(applyScan(list, { name: 'nope', scan: running })).toBe(list);
		expect(applyScan(list, { name: 'tank', scan: null })).toBe(list);
		expect(applyScan(list, null)).toBe(list);
	});
});

describe('scanLabel', () => {
	const scan = parsePool(raw)!.scan!;

	it('shows progress while running', () => {
		const running = {
			...scan,
			state: 'running' as const,
			percent: 42,
			secondsLeft: 3600,
			end: null
		};
		expect(scanLabel(running, NOW)).toBe('Checking data · 42% · about 1 h left');
		expect(scanLabel({ ...running, kind: 'resilver', secondsLeft: null }, NOW)).toBe(
			'Rebuilding · 42%'
		);
		expect(scanLabel({ ...running, paused: new Date(NOW) }, NOW)).toBe(
			'Checking data paused at 42%'
		);
		expect(scanLabel({ ...running, errors: 1, secondsLeft: 1500 }, NOW)).toBe(
			'Checking data · 42% · about 25 min left · 1 problem found'
		);
	});

	it('says when the last check ended', () => {
		expect(scanLabel(scan, NOW)).toBe('Last checked 2 days ago');
		expect(scanLabel({ ...scan, end: new Date(NOW - 26 * HOUR) }, NOW)).toBe(
			'Last checked yesterday'
		);
		expect(scanLabel({ ...scan, end: new Date(NOW - 2 * HOUR), errors: 3 }, NOW)).toBe(
			'Last checked 2 h ago · 3 problems found'
		);
		expect(scanLabel({ ...scan, state: 'canceled' }, NOW)).toBe('Data check stopped 2 days ago');
		expect(scanLabel({ ...scan, kind: 'resilver', end: new Date(NOW) }, NOW)).toBe(
			'Rebuilt just now'
		);
	});

	it('says nothing without a scan or an end date', () => {
		expect(scanLabel(null, NOW)).toBeNull();
		expect(scanLabel({ ...scan, end: null }, NOW)).toBeNull();
	});
});

describe('poolRows', () => {
	it('puts a nearly full pool before a healthy one', () => {
		const roots = new Map([['tank', { used: 95, available: 5 }]]);
		const rows = poolRows([pool({ id: 2, name: 'apps' }), pool()], roots, NOW);
		expect(rows.map((r) => [r.name, r.health.label, r.usage?.raw])).toEqual([
			['tank', 'Almost full', false],
			['apps', 'Healthy', true]
		]);
		expect(rows[0].scan).toBe('Last checked 2 days ago');
	});
});

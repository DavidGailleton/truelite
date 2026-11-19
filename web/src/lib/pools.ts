// Pool health and space from `pool.query`, `pool.scan` and the root datasets of
// `pool.dataset.query` (api.truenas.com v25.10; roles POOL_READ, POOL_SCRUB_READ, DATASET_READ).
// Framework-free so it can be unit-tested: the Svelte binding lives in pools.svelte.ts.
import { formatPercent } from './realtime';
import { isNum, isObj, isStr, parseDate } from './system';
import { ago } from './time';
import type { EventData } from './ws';

export const POOL_QUERY = 'pool.query';
export const POOL_SCAN = 'pool.scan';
export const DATASET_QUERY = 'pool.dataset.query';

/** Fields asked of `pool.query`; the server forwards the same ones in events (`truenas/pools.rs`). */
export const POOL_SELECT = [
	'id',
	'name',
	'status',
	'healthy',
	'warning',
	'status_code',
	'status_detail',
	'size',
	'allocated',
	'free',
	'scan'
] as const;

/** A scrub (data check) or resilver (rebuild onto a new disk). */
export interface Scan {
	kind: 'scrub' | 'resilver';
	state: 'running' | 'finished' | 'canceled';
	/** 0–100. */
	percent: number;
	end: Date | null;
	/** Set while paused. */
	paused: Date | null;
	/** Problems found so far. */
	errors: number;
	secondsLeft: number | null;
}

export interface Pool {
	id: number;
	name: string;
	/** ZFS state: `ONLINE`, `DEGRADED`, `FAULTED`, … */
	status: string;
	healthy: boolean;
	warning: boolean;
	statusCode: string | null;
	statusDetail: string | null;
	/** Raw bytes, parity included. */
	size: number | null;
	allocated: number | null;
	scan: Scan | null;
}

/** Usable space of a pool's root dataset, bytes. */
export interface RootUsage {
	used: number;
	available: number;
}

export type Level = 'ok' | 'warning' | 'critical';

export interface Health {
	level: Level;
	/** One or two words for the status pill. */
	label: string;
	/** A sentence saying what is wrong, or `null` when all is well. */
	detail: string | null;
}

const SCAN_KINDS: Record<string, Scan['kind']> = { SCRUB: 'scrub', RESILVER: 'resilver' };
const SCAN_STATES: Record<string, Scan['state']> = {
	SCANNING: 'running',
	FINISHED: 'finished',
	CANCELED: 'canceled'
};

const numOrNull = (v: unknown) => (isNum(v) ? v : null);
const strOrNull = (v: unknown) => (isStr(v) && v.trim() !== '' ? v : null);

/** A pool's `scan` object, or `null` when there is none or it has another shape. */
export function parseScan(raw: unknown): Scan | null {
	if (!isObj(raw)) return null;
	const kind = isStr(raw.function) ? SCAN_KINDS[raw.function] : undefined;
	const state = isStr(raw.state) ? SCAN_STATES[raw.state] : undefined;
	if (!kind || !state) return null;
	return {
		kind,
		state,
		percent: Math.min(100, Math.max(0, numOrNull(raw.percentage) ?? 0)),
		end: parseDate(raw.end_time),
		paused: parseDate(raw.pause),
		errors: Math.max(0, numOrNull(raw.errors) ?? 0),
		secondsLeft: numOrNull(raw.total_secs_left)
	};
}

/** One `pool.query` entry, or `null` when it lacks the fields the card needs. */
export function parsePool(raw: unknown): Pool | null {
	if (!isObj(raw)) return null;
	const { id, name, status } = raw;
	if (!isNum(id) || !isStr(name) || !isStr(status)) return null;
	return {
		id,
		name,
		status: status.toUpperCase(),
		healthy: raw.healthy !== false,
		warning: raw.warning === true,
		statusCode: strOrNull(raw.status_code),
		statusDetail: strOrNull(raw.status_detail),
		size: numOrNull(raw.size),
		allocated: numOrNull(raw.allocated),
		scan: parseScan(raw.scan)
	};
}

/** The `pool.query` result; unreadable entries are dropped. `null` when it isn't a list. */
export function parsePools(raw: unknown): Pool[] | null {
	if (!Array.isArray(raw)) return null;
	return raw.map(parsePool).filter((p): p is Pool => p !== null);
}

/**
 * A ZFS property as bytes: a plain number (the server flattens `pool.dataset.query` properties,
 * `truenas/datasets.rs`) or a property object (`{parsed, rawvalue, value, source}`).
 */
export function bytesOf(prop: unknown): number | null {
	if (isNum(prop)) return prop;
	if (!isObj(prop)) return null;
	if (isNum(prop.parsed)) return prop.parsed;
	const n = isStr(prop.rawvalue) ? Number(prop.rawvalue) : NaN;
	return Number.isFinite(n) ? n : null;
}

/** Root datasets from `pool.dataset.query`, by pool name. */
export function parseRootUsage(raw: unknown): Map<string, RootUsage> {
	const usage = new Map<string, RootUsage>();
	if (!Array.isArray(raw)) return usage;
	for (const ds of raw) {
		if (!isObj(ds) || !isStr(ds.id) || ds.id.includes('/')) continue;
		const used = bytesOf(ds.used);
		const available = bytesOf(ds.available);
		if (used !== null && available !== null) usage.set(ds.id, { used, available });
	}
	return usage;
}

/** `pool.dataset.query` params for the root datasets of `names`: two properties, no children. */
export function rootUsageParams(names: readonly string[]): unknown[] {
	return [
		[['id', 'in', names]],
		{
			select: ['id', 'used', 'available'],
			extra: { retrieve_children: false, properties: ['used', 'available'] }
		}
	];
}

export interface Usage {
	used: number;
	total: number;
	/** 0–100. */
	percent: number;
	/** Raw pool space (parity included), because the usable space is unknown. */
	raw: boolean;
}

/** Usable space when the root dataset is known, else raw pool space; `null` when neither is. */
export function usage(pool: Pool, root?: RootUsage): Usage | null {
	let used: number, total: number, raw: boolean;
	if (root) {
		used = root.used;
		total = root.used + root.available;
		raw = false;
	} else if (pool.size !== null && pool.allocated !== null) {
		used = pool.allocated;
		total = pool.size;
		raw = true;
	} else {
		return null;
	}
	if (total <= 0) return null;
	const clamped = Math.min(Math.max(0, used), total);
	return { used: clamped, total, percent: (clamped / total) * 100, raw };
}

/** Space thresholds, %: TrueNAS raises its own capacity alerts at the same points. */
export const FULL_WARNING = 80;
export const FULL_CRITICAL = 90;

/** How full `percent` is against those thresholds. */
export const fullLevel = (percent: number): Level =>
	percent >= FULL_CRITICAL ? 'critical' : percent >= FULL_WARNING ? 'warning' : 'ok';

const UNAVAILABLE = new Set(['FAULTED', 'UNAVAIL', 'OFFLINE', 'REMOVED', 'SUSPENDED']);
/** `status_code`s that set `warning` but need no action from the user here. */
const BENIGN_CODES = new Set(['OK', 'FEAT_DISABLED']);
const RANK: Record<Level, number> = { critical: 0, warning: 1, ok: 2 };

/** What to tell the user about a pool, the worst problem first. */
export function poolHealth(pool: Pool, percent: number | null): Health {
	const found: Health[] = [];
	if (UNAVAILABLE.has(pool.status)) {
		found.push({
			level: 'critical',
			label: 'Not available',
			detail: 'The pool is offline: its data cannot be reached.'
		});
	} else if (pool.status === 'DEGRADED') {
		found.push({
			level: 'warning',
			label: 'Degraded',
			detail: 'A disk failed or is missing. Data is still readable; replace the disk.'
		});
	} else if (pool.status !== 'ONLINE') {
		found.push({ level: 'warning', label: 'Needs attention', detail: pool.statusDetail });
	}
	if (pool.status === 'ONLINE' && !pool.healthy) {
		found.push({
			level: 'warning',
			label: 'Errors found',
			detail: pool.statusDetail ?? 'Some data on this pool could not be read correctly.'
		});
	}
	if (pool.warning && pool.statusCode && !BENIGN_CODES.has(pool.statusCode)) {
		found.push({ level: 'warning', label: 'Needs attention', detail: pool.statusDetail });
	}
	if (pool.scan?.kind === 'resilver' && pool.scan.state === 'running') {
		found.push({
			level: 'warning',
			label: 'Rebuilding',
			detail: 'Copying data onto a replaced disk. The pool works, but more slowly.'
		});
	}
	if (percent !== null && percent >= FULL_CRITICAL) {
		found.push({
			level: 'critical',
			label: 'Almost full',
			detail: 'Free up space: a full pool stops accepting writes.'
		});
	} else if (percent !== null && percent >= FULL_WARNING) {
		found.push({
			level: 'warning',
			label: 'Getting full',
			detail: 'Pools get slower when more than 80% full.'
		});
	}
	if (found.length === 0) return { level: 'ok', label: 'Healthy', detail: null };
	// Stable: among equally bad problems, the first rule above wins.
	return found.reduce((worst, h) => (RANK[h.level] < RANK[worst.level] ? h : worst));
}

/** Worst first, then by name. */
export function sortPools<T extends { name: string; health: Health }>(pools: readonly T[]): T[] {
	return [...pools].sort(
		(a, b) => RANK[a.health.level] - RANK[b.health.level] || a.name.localeCompare(b.name)
	);
}

/**
 * The pool list after a `pool.query` event, or `null` when the event can't be read and the
 * list should be queried again.
 */
export function applyPoolEvent(pools: readonly Pool[], event: EventData): Pool[] | null {
	if (!isNum(event.id)) return null;
	const id = event.id;
	if (event.kind === 'removed') return pools.filter((p) => p.id !== id);
	if (event.kind !== 'added' && event.kind !== 'changed') return null;
	const pool = parsePool(event.fields);
	if (!pool || pool.id !== id) return null;
	const at = pools.findIndex((p) => p.id === id);
	if (at < 0) return [...pools, pool];
	return pools.map((p, i) => (i === at ? pool : p));
}

/** The pool list after a `pool.scan` event (`{name, scan}`); unchanged when it names no pool. */
export function applyScan(pools: readonly Pool[], fields: unknown): readonly Pool[] {
	if (!isObj(fields) || !isStr(fields.name)) return pools;
	const scan = parseScan(fields.scan);
	const at = pools.findIndex((p) => p.name === fields.name);
	if (at < 0 || !scan) return pools;
	return pools.map((p, i) => (i === at ? { ...p, scan } : p));
}

/** `25 min`, `3 h`, `2 days`. */
function duration(seconds: number): string {
	const min = Math.max(1, Math.round(seconds / 60));
	if (min < 60) return `${min} min`;
	const h = Math.round(min / 60);
	if (h < 48) return `${h} h`;
	return `${Math.round(h / 24)} days`;
}

/** The scrub or resilver line under a pool, or `null` when there is nothing to say. */
export function scanLabel(scan: Scan | null, now: number): string | null {
	if (!scan) return null;
	const what = scan.kind === 'scrub' ? 'Checking data' : 'Rebuilding';
	const problems =
		scan.errors > 0 ? ` · ${scan.errors} ${scan.errors === 1 ? 'problem' : 'problems'} found` : '';
	if (scan.state === 'running') {
		const pct = formatPercent(scan.percent);
		if (scan.paused) return `${what} paused at ${pct}${problems}`;
		const left = scan.secondsLeft;
		const eta = left !== null && left > 0 ? ` · about ${duration(left)} left` : '';
		return `${what} · ${pct}${eta}${problems}`;
	}
	if (!scan.end) return null;
	const when = ago(scan.end, now);
	if (scan.state === 'canceled') {
		return `${scan.kind === 'scrub' ? 'Data check' : 'Rebuild'} stopped ${when}`;
	}
	return `${scan.kind === 'scrub' ? 'Last checked' : 'Rebuilt'} ${when}${problems}`;
}

/** What the card shows for one pool. */
export interface PoolRow {
	id: number;
	name: string;
	usage: Usage | null;
	health: Health;
	scan: string | null;
}

/** The card's rows, worst first. `now` is for "last checked … ago". */
export function poolRows(
	pools: readonly Pool[],
	roots: ReadonlyMap<string, RootUsage>,
	now: number
): PoolRow[] {
	return sortPools(
		pools.map((p) => {
			const u = usage(p, roots.get(p.name));
			return {
				id: p.id,
				name: p.name,
				usage: u,
				health: poolHealth(p, u?.percent ?? null),
				scan: scanLabel(p.scan, now)
			};
		})
	);
}

// ---- Topology (pools page) -------------------------------------------------------------------
// `pool.query` with `topology` selected; the server cuts each node down to `name`, `type`,
// `status`, `disk`, `errors` and `children` (`truenas/pools.rs` `compact_topology`). The 25.10
// docs only name the groups, so every node field is read defensively.

/** `pool.query` params for the pools page's layouts: ids and topology only. */
export const TOPOLOGY_PARAMS: readonly unknown[] = [[], { select: ['id', 'topology'] }];

export const TOPOLOGY_GROUPS = ['data', 'special', 'dedup', 'log', 'cache', 'spare'] as const;
export type TopologyGroup = (typeof TOPOLOGY_GROUPS)[number];

export interface DeviceErrors {
	read: number;
	write: number;
	checksum: number;
}

/** A vdev (mirror, RAIDZ, …) or a disk. */
export interface Vdev {
	name: string;
	/** `DISK`, `MIRROR`, `RAIDZ1`, `REPLACING`, … upper case. */
	type: string;
	/** `ONLINE`, `DEGRADED`, `OFFLINE`, `AVAIL` (spare), … upper case. */
	status: string;
	/** The disk's device name (`sda`), for disks. */
	disk: string | null;
	errors: DeviceErrors;
	children: Vdev[];
}

export type Topology = Record<TopologyGroup, Vdev[]>;

const count = (v: unknown) => (isNum(v) && v > 0 ? Math.floor(v) : 0);

/** One topology node, or `null` when it lacks a name. */
export function parseVdev(raw: unknown): Vdev | null {
	if (!isObj(raw) || !isStr(raw.name)) return null;
	const errors = isObj(raw.errors) ? raw.errors : {};
	return {
		name: raw.name,
		type: isStr(raw.type) ? raw.type.toUpperCase() : 'UNKNOWN',
		status: isStr(raw.status) ? raw.status.toUpperCase() : 'UNKNOWN',
		disk: strOrNull(raw.disk),
		errors: {
			read: count(errors.read),
			write: count(errors.write),
			checksum: count(errors.checksum)
		},
		children: Array.isArray(raw.children)
			? raw.children.map(parseVdev).filter((v): v is Vdev => v !== null)
			: []
	};
}

/** A pool's `topology`, or `null` when there is none (TrueNAS sends `null` when unavailable). */
export function parseTopology(raw: unknown): Topology | null {
	if (!isObj(raw)) return null;
	const groups = {} as Topology;
	for (const g of TOPOLOGY_GROUPS) {
		const list = raw[g];
		groups[g] = Array.isArray(list) ? list.map(parseVdev).filter((v): v is Vdev => v !== null) : [];
	}
	return groups;
}

/** The `pool.query` result of `TOPOLOGY_PARAMS`, by pool id; `null` when it isn't a list. */
export function parseTopologies(raw: unknown): Map<number, Topology> | null {
	if (!Array.isArray(raw)) return null;
	const out = new Map<number, Topology>();
	for (const p of raw) {
		if (!isObj(p) || !isNum(p.id)) continue;
		const t = parseTopology(p.topology);
		if (t) out.set(p.id, t);
	}
	return out;
}

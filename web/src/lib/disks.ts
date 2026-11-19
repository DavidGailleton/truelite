// The disks page (#39): every disk with its pool, its temperature against its critical limit and
// its state in the pool layout. `disk.query` (DISK_READ), `disk.temperatures` and
// `disk.temperature_agg` (REPORTING_READ), api.truenas.com v25.10. The server keeps only the
// shown disk fields and gives temperatures as `{name: {temp, crit}}` (`truenas/disks.rs`).
// SMART is not in the 25.10 API (#61). Framework-free so it can be unit-tested: the Svelte
// binding lives in disks.svelte.ts.
import { TOPOLOGY_GROUPS, type Topology, type Vdev } from './pools';
import { deviceProblem, disksOf } from './pool-layout';
import { isNum, isObj, isStr } from './system';

export const DISK_QUERY = 'disk.query';
export const DISK_TEMPS = 'disk.temperatures';
export const DISK_TEMP_AGG = 'disk.temperature_agg';

/** Fields read for each disk (`FIELDS` in `truenas/disks.rs`). */
export const DISK_SELECT = [
	'identifier',
	'name',
	'serial',
	'model',
	'size',
	'type',
	'rotationrate',
	'bus',
	'pool',
	'description'
];

/** Every disk with its pool: without `extra.pools`, TrueNAS leaves `pool` null. */
export function diskQueryParams(): unknown[] {
	return [[], { select: DISK_SELECT, extra: { pools: true } }];
}

/** All disks, with their critical limits. */
export const TEMPS_PARAMS: readonly unknown[] = [[], true];

/** Days of temperature history asked for (TrueNAS keeps at most 7). */
export const AGG_DAYS = 7;

/** The boot pool's name on TrueNAS. */
export const BOOT_POOL = 'boot-pool';

/** A temperature this close to the critical limit (°C) is "warm". */
export const WARM_MARGIN = 5;

export interface Disk {
	/** Device name: `sda`, `nvme0n1`. */
	name: string;
	model: string | null;
	serial: string | null;
	/** Bytes. */
	size: number | null;
	/** RPM; `null` for SSDs and unknown devices. */
	rotation: number | null;
	/** `HDD`, `SSD`, or `null` when TrueNAS doesn't know. */
	type: string | null;
	bus: string | null;
	pool: string | null;
	/** The note set in TrueNAS ("Left bay"). */
	description: string | null;
}

export interface DiskTemp {
	/** °C; `null` without a reading (asleep, no sensor). */
	temp: number | null;
	/** Critical limit in °C; `null` when the disk reports none. */
	crit: number | null;
}

export interface TempRange {
	min: number | null;
	max: number | null;
}

const text = (v: unknown): string | null => (isStr(v) && v.trim() !== '' ? v.trim() : null);
const num = (v: unknown): number | null => (isNum(v) && Number.isFinite(v) ? v : null);
const positive = (v: unknown): number | null => {
	const n = num(v);
	return n !== null && n > 0 ? n : null;
};

/** One compacted `disk.query` entry, or `null` without a name. */
export function parseDisk(raw: unknown): Disk | null {
	if (!isObj(raw) || !text(raw.name)) return null;
	return {
		name: String(text(raw.name)),
		model: text(raw.model),
		serial: text(raw.serial),
		size: positive(raw.size),
		rotation: positive(raw.rotationrate),
		type: text(raw.type)?.toUpperCase() ?? null,
		bus: text(raw.bus),
		pool: text(raw.pool),
		description: text(raw.description)
	};
}

/** A `disk.query` result; `null` when it is not a list. */
export function parseDisks(raw: unknown): Disk[] | null {
	if (!Array.isArray(raw)) return null;
	return raw.map(parseDisk).filter((d): d is Disk => d !== null);
}

/** A reshaped `disk.temperatures` result; `null` when it is not an object. */
export function parseTemps(raw: unknown): Map<string, DiskTemp> | null {
	if (!isObj(raw) || Array.isArray(raw)) return null;
	const out = new Map<string, DiskTemp>();
	for (const [name, v] of Object.entries(raw)) {
		if (isObj(v)) out.set(name, { temp: num(v.temp), crit: positive(v.crit) });
	}
	return out;
}

/** A `disk.temperature_agg` result (`{name: {min, max, avg}}`); `null` when not an object. */
export function parseTempAgg(raw: unknown): Map<string, TempRange> | null {
	if (!isObj(raw) || Array.isArray(raw)) return null;
	const out = new Map<string, TempRange>();
	for (const [name, v] of Object.entries(raw)) {
		if (!isObj(v)) continue;
		const range = { min: num(v.min), max: num(v.max) };
		if (range.min !== null || range.max !== null) out.set(name, range);
	}
	return out;
}

/** `disk.temperature_agg` params for these disks. */
export function aggParams(names: readonly string[]): unknown[] {
	return [[...names], AGG_DAYS];
}

/** "NVMe SSD", "HDD · 7200 rpm", "SSD"; `null` when TrueNAS doesn't know. */
export function diskKind(d: Disk): string | null {
	if (d.name.startsWith('nvme') || d.bus?.toUpperCase() === 'NVME') return 'NVMe SSD';
	if (d.rotation !== null) return `HDD · ${d.rotation} rpm`;
	if (d.type === 'HDD' || d.type === 'SSD') return d.type;
	return null;
}

export type TempLevel = 'ok' | 'warm' | 'hot';

/** Hot at or over the critical limit, warm within {@link WARM_MARGIN} of it. */
export function tempLevel(t: DiskTemp | null | undefined): TempLevel {
	if (!t || t.temp === null || t.crit === null) return 'ok';
	if (t.temp >= t.crit) return 'hot';
	return t.temp >= t.crit - WARM_MARGIN ? 'warm' : 'ok';
}

/** `36 °C`. */
export function formatTemp(c: number): string {
	return `${Math.round(c)} °C`;
}

/** Each pool disk's layout node, by device name (`sda`), across every pool. */
export function disksByName(topologies: Iterable<Topology>): Map<string, Vdev> {
	const out = new Map<string, Vdev>();
	for (const t of topologies) {
		for (const g of TOPOLOGY_GROUPS) {
			for (const v of t[g].flatMap(disksOf)) out.set(v.disk ?? v.name, v);
		}
	}
	return out;
}

export interface DiskRow {
	disk: Disk;
	temp: DiskTemp | null;
	level: TempLevel;
	range: TempRange | null;
	/** The disk's node in its pool layout; `null` outside a pool or before layouts load. */
	device: Vdev | null;
	/** What the pool says is wrong ("Failed", "Online · 2 read errors"); `null` when nothing. */
	problem: string | null;
}

export interface DiskGroup {
	/** The pool name, or `''` for disks in no pool. */
	pool: string;
	title: string;
	rows: DiskRow[];
}

/** Whether the row needs a look: hot, warm, or a pool problem. */
export const rowNeedsLook = (r: DiskRow): boolean => r.level !== 'ok' || r.problem !== null;

const rank = (r: DiskRow): number =>
	r.level === 'hot' ? 0 : r.problem !== null ? 1 : r.level === 'warm' ? 2 : 3;

const byName = (a: string, b: string) => a.localeCompare(b, 'en', { numeric: true });

/**
 * Disks grouped by pool: data pools by name, then the boot pool, then disks in no pool; a group
 * with a disk needing a look comes first. In each group, hot disks, then pool problems, then warm
 * ones, then by name (`sda`, `sdb`, …, `sdaa`).
 */
export function diskGroups(
	disks: readonly Disk[],
	temps: ReadonlyMap<string, DiskTemp> | null,
	ranges: ReadonlyMap<string, TempRange> | null,
	devices: ReadonlyMap<string, Vdev> | null
): DiskGroup[] {
	const groups = new Map<string, DiskRow[]>();
	for (const disk of disks) {
		const temp = temps?.get(disk.name) ?? null;
		const device = devices?.get(disk.name) ?? null;
		const row: DiskRow = {
			disk,
			temp,
			level: tempLevel(temp),
			range: ranges?.get(disk.name) ?? null,
			device,
			problem: device ? deviceProblem(device) : null
		};
		const key = disk.pool ?? '';
		const rows = groups.get(key);
		if (rows) rows.push(row);
		else groups.set(key, [row]);
	}
	const order = (pool: string) => (pool === '' ? 2 : pool === BOOT_POOL ? 1 : 0);
	return [...groups]
		.map(([pool, rows]) => ({
			pool,
			title: pool === '' ? 'Not in a pool' : pool === BOOT_POOL ? 'Boot pool' : pool,
			rows: rows.sort((a, b) => rank(a) - rank(b) || byName(a.disk.name, b.disk.name))
		}))
		.sort(
			(a, b) =>
				Number(b.rows.some(rowNeedsLook)) - Number(a.rows.some(rowNeedsLook)) ||
				order(a.pool) - order(b.pool) ||
				byName(a.pool, b.pool)
		);
}

/** "6 disks", "6 disks · 1 too hot · 1 needs a look". */
export function disksSummary(groups: readonly DiskGroup[]): string {
	const rows = groups.flatMap((g) => g.rows);
	const hot = rows.filter((r) => r.level === 'hot').length;
	const look = rows.filter((r) => r.level !== 'hot' && rowNeedsLook(r)).length;
	const parts = [`${rows.length} ${rows.length === 1 ? 'disk' : 'disks'}`];
	if (hot > 0) parts.push(`${hot} too hot`);
	if (look > 0) parts.push(`${look} ${look === 1 ? 'needs' : 'need'} a look`);
	return parts.join(' · ');
}

/** The id of a disk's row on the disks page. */
export const diskAnchor = (name: string): string => `disk-${name}`;

/** The disks page, scrolled to `name` when given. */
export const disksHref = (name?: string): string =>
	name ? `/storage/disks#${diskAnchor(name)}` : '/storage/disks';

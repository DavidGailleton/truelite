// Live statistics from the `reporting.realtime` topic, in the compact shape the server sends
// (docs/ws-protocol.md, "Reshaped topics"). Framework-free so it can be unit-tested.

export const REALTIME_TOPIC = 'reporting.realtime';

/** Samples kept for the sparklines: 2 minutes at one sample every 2 s. */
export const HISTORY_SIZE = 60;

export interface NetStats {
	/** Bytes per second received. */
	rx: number;
	/** Bytes per second sent. */
	tx: number;
	up: boolean;
	/** Link speed in Mb/s, `null` when unknown. */
	speed: number | null;
}

export interface Sample {
	/** Whole-CPU usage, %. */
	cpu: number;
	/** °C, `null` when the NAS reports none. */
	temp: number | null;
	/** Bytes. */
	mem: { total: number; available: number; arc: number };
	net: Record<string, NetStats>;
}

const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isObj = (v: unknown): v is Record<string, unknown> =>
	typeof v === 'object' && v !== null && !Array.isArray(v);

/** The sample in `fields`, or `null` when it does not have the documented shape. */
export function parseSample(fields: unknown): Sample | null {
	if (!isObj(fields) || !isNum(fields.cpu)) return null;
	const { temp, mem, net } = fields;
	if (temp !== null && !isNum(temp)) return null;
	if (!isObj(mem) || !isNum(mem.total) || !isNum(mem.available) || !isNum(mem.arc)) return null;
	if (!isObj(net)) return null;
	const ifaces: Record<string, NetStats> = {};
	for (const [name, i] of Object.entries(net)) {
		if (!isObj(i) || !isNum(i.rx) || !isNum(i.tx) || typeof i.up !== 'boolean') return null;
		if (i.speed !== null && !isNum(i.speed)) return null;
		ifaces[name] = { rx: i.rx, tx: i.tx, up: i.up, speed: i.speed };
	}
	return {
		cpu: fields.cpu,
		temp,
		mem: { total: mem.total, available: mem.available, arc: mem.arc },
		net: ifaces
	};
}

/** `history` with `sample` appended, keeping the last `size` samples. Returns a new array. */
export function appendSample(history: readonly Sample[], sample: Sample, size = HISTORY_SIZE) {
	const next = history.length >= size ? history.slice(history.length - size + 1) : [...history];
	next.push(sample);
	return next;
}

/**
 * The interface to chart: `saved` while it still exists, else the fastest one that is up (ties
 * by name), else the first by name; `null` when there is none.
 */
export function pickInterface(net: Record<string, NetStats>, saved?: string | null) {
	if (saved && saved in net) return saved;
	const names = Object.keys(net).sort();
	const up = names.filter((n) => net[n].up);
	if (up.length === 0) return names[0] ?? null;
	return up.reduce((best, n) => ((net[n].speed ?? 0) > (net[best].speed ?? 0) ? n : best));
}

/**
 * Memory in bytes, split the way TrueNAS shows it: `free` is what the kernel reports available,
 * `arc` the ZFS cache (it gives memory back when services need it), `services` the rest.
 */
export function memorySplit(mem: Sample['mem']) {
	const total = Math.max(0, mem.total);
	const free = Math.min(Math.max(0, mem.available), total);
	const arc = Math.min(Math.max(0, mem.arc), total - free);
	return { total, services: total - free - arc, arc, free };
}

function scaled(value: number, base: number, units: readonly string[]): string {
	let v = Math.max(0, value);
	let u = 0;
	while (v >= base && u < units.length - 1) {
		v /= base;
		u++;
	}
	// One decimal below 100 (not for plain bytes), without a trailing ".0".
	const text = u > 0 && v < 99.95 ? String(Math.round(v * 10) / 10) : String(Math.round(v));
	return `${text} ${units[u]}`;
}

/** Bytes per second in SI units, as network speeds are given: `1.2 MB/s`. */
export function formatRate(bytesPerSecond: number): string {
	return scaled(bytesPerSecond, 1000, ['B/s', 'kB/s', 'MB/s', 'GB/s']);
}

/** Bytes in binary units, as TrueNAS shows memory: `12.4 GiB`. */
export function formatBytes(bytes: number): string {
	return scaled(bytes, 1024, ['B', 'KiB', 'MiB', 'GiB', 'TiB']);
}

/** `7.5%` below 10, `42%` above. */
export function formatPercent(percent: number): string {
	const p = Math.min(100, Math.max(0, percent));
	return `${p > 0 && p < 10 ? p.toFixed(1) : Math.round(p)}%`;
}

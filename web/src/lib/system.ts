// System details from `system.info` (api.truenas.com v25.10, role READONLY_ADMIN, no event).
// Framework-free so it can be unit-tested: the Svelte binding lives in system.svelte.ts.
import type { Status } from './ws';

export const SYSTEM_INFO = 'system.info';

export interface SystemInfo {
	hostname: string;
	/** As the NAS reports it; see {@link versionLabel}. */
	version: string;
	/** Seconds since boot when the reply was made. */
	uptimeSeconds: number;
	bootTime: Date | null;
	/** CPU model. */
	cpu: string;
	/** Logical CPUs (threads). */
	cores: number;
	physicalCores: number | null;
	/** Bytes. */
	memory: number;
	ecc: boolean;
	/** Manufacturer and product, `null` when the firmware gives nothing real. */
	hardware: string | null;
	timezone: string | null;
}

export const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
export const isStr = (v: unknown): v is string => typeof v === 'string';
export const isObj = (v: unknown): v is Record<string, unknown> =>
	typeof v === 'object' && v !== null && !Array.isArray(v);

/** Whether `roles` hold `role` or FULL_ADMIN (TrueNAS expands FULL_ADMIN, but don't rely on it). */
export const hasRole = (roles: readonly string[] | undefined, role: string): boolean =>
	roles !== undefined && (roles.includes(role) || roles.includes('FULL_ADMIN'));

/** A middleware datetime: `{"$date": ms}` on the wire, or an ISO string. */
export function parseDate(v: unknown): Date | null {
	const raw = isObj(v) ? v.$date : v;
	if (!isNum(raw) && !isStr(raw)) return null;
	const d = new Date(raw);
	return Number.isNaN(d.getTime()) ? null : d;
}

/** The result of `system.info`, or `null` when it does not have the documented shape. */
export function parseSystemInfo(raw: unknown): SystemInfo | null {
	if (!isObj(raw)) return null;
	const { hostname, version, uptime_seconds, model, cores, physmem } = raw;
	if (!isStr(hostname) || !isStr(version) || !isNum(uptime_seconds)) return null;
	if (!isStr(model) || !isNum(cores) || !isNum(physmem)) return null;
	const str = (v: unknown) => (isStr(v) ? v : null);
	return {
		hostname,
		version,
		uptimeSeconds: Math.max(0, uptime_seconds),
		bootTime: parseDate(raw.boottime),
		cpu: model.trim(),
		cores,
		physicalCores: isNum(raw.physical_cores) ? raw.physical_cores : null,
		memory: physmem,
		ecc: raw.ecc_memory === true,
		hardware: hardwareLabel(str(raw.system_manufacturer), str(raw.system_product)),
		timezone: str(raw.timezone)
	};
}

/** `TrueNAS 25.10.8`, whether the NAS says `25.10.8` or `TrueNAS-SCALE-25.10.8`. */
export function versionLabel(version: string): string {
	const bare = version.trim().replace(/^TrueNAS(-SCALE)?[-\s]*/i, '');
	return `TrueNAS ${bare}`;
}

/** `up 5 min`, `up 3 h 20 min`, `up 1 day 4 h`, `up 12 days`. */
export function formatUptime(seconds: number): string {
	const s = Math.max(0, Math.floor(seconds));
	if (s < 60) return 'just started';
	const min = Math.floor(s / 60) % 60;
	const h = Math.floor(s / 3600) % 24;
	const days = Math.floor(s / 86400);
	if (days >= 7) return `up ${days} days`;
	if (days >= 1) {
		const d = `${days} ${days === 1 ? 'day' : 'days'}`;
		return h > 0 ? `up ${d} ${h} h` : `up ${d}`;
	}
	if (h >= 1) return min > 0 ? `up ${h} h ${min} min` : `up ${h} h`;
	return `up ${min} min`;
}

/** Firmware strings that mean "not filled in". */
const PLACEHOLDERS = [
	/^to be filled by o\.?e\.?m\.?$/i,
	/^system (product name|manufacturer|manufacturer name)$/i,
	/^default string$/i,
	/^not (specified|applicable|available)$/i,
	/^o\.?e\.?m\.?$/i,
	/^none$/i,
	/^n\/a$/i,
	/^$/
];

/** Manufacturer and product joined, placeholders dropped; `null` when nothing real is left. */
export function hardwareLabel(manufacturer: string | null, product: string | null): string | null {
	const parts = [manufacturer, product]
		.map((p) => p?.trim() ?? '')
		.filter((p) => !PLACEHOLDERS.some((re) => re.test(p)));
	// "Dell Inc." + "Dell Inc. PowerEdge" would repeat the brand.
	if (parts.length === 2 && parts[1].toLowerCase().startsWith(parts[0].toLowerCase())) {
		return parts[1];
	}
	return parts.length > 0 ? parts.join(' ') : null;
}

/** `8 threads, 4 cores`, or `4 cores` when both counts match or the physical one is unknown. */
export function coresLabel(cores: number, physical: number | null): string {
	const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? '' : 's'}`;
	if (physical === null || physical === cores) return plural(cores, 'core');
	return `${plural(cores, 'thread')}, ${plural(physical, 'core')}`;
}

/**
 * Calls `fetch` each time the connection becomes `up` (the first time and after every drop):
 * a NAS that reconnected may have rebooted. Feed it every status change.
 */
export function refetchOnUp(fetch: () => void): (status: Status) => void {
	let wasUp = false;
	return (status) => {
		const up = status === 'up';
		if (up && !wasUp) fetch();
		wasUp = up;
	};
}

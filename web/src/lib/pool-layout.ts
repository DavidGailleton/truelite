// Pool layouts in plain words: what each vdev protects against, which device has a problem.
// Only the Storage page uses these (kept out of pools.ts, which the dashboard loads).
import { TOPOLOGY_GROUPS, type Topology, type TopologyGroup, type Vdev } from './pools';

/** What each group is for, in plain words. */
export const GROUP_LABELS: Record<TopologyGroup, string> = {
	data: 'Storage',
	special: 'Metadata',
	dedup: 'Dedup table',
	log: 'Write log',
	cache: 'Read cache',
	spare: 'Spares'
};

/** A disk is a node with a device name, or any node without children. */
export function isDisk(v: Vdev): boolean {
	return v.type === 'DISK' || v.type === 'FILE' || v.children.length === 0;
}

/** Every disk under `v` (itself when it is one). */
export function disksOf(v: Vdev): Vdev[] {
	return isDisk(v) ? [v] : v.children.flatMap(disksOf);
}

/** Disks across every group of a pool. */
export function diskCount(t: Topology): number {
	return TOPOLOGY_GROUPS.reduce((n, g) => n + t[g].reduce((m, v) => m + disksOf(v).length, 0), 0);
}

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

export interface VdevLabel {
	text: string;
	/** Nothing protects this data from a single disk failure. */
	risky: boolean;
}

/**
 * A vdev's layout in plain words: "Mirror · 2 disks", "RAIDZ2 · 6 disks · 2 can fail",
 * "Single disk · no redundancy". `group` matters: a lone cache or spare disk is no risk.
 */
export function vdevLabel(v: Vdev, group: TopologyGroup): VdevLabel {
	if (isDisk(v)) {
		const risky = group === 'data' || group === 'special' || group === 'dedup';
		return { text: risky ? 'Single disk · no redundancy' : 'Disk', risky };
	}
	// Width, not disks: a disk being replaced counts once (its REPLACING node holds both).
	const width = v.children.length;
	const disks = plural(width, 'disk');
	if (v.type === 'MIRROR')
		return { text: `Mirror · ${disks} · ${width - 1} can fail`, risky: false };
	const parity = /^(RAIDZ|DRAID)([123])/.exec(v.type);
	if (parity) {
		const name = parity[1] === 'RAIDZ' ? 'RAIDZ' : 'dRAID';
		return { text: `${name}${parity[2]} · ${disks} · ${parity[2]} can fail`, risky: false };
	}
	if (v.type === 'REPLACING') return { text: 'Replacing a disk', risky: false };
	if (v.type === 'SPARE') return { text: 'Spare in use', risky: false };
	const kind = `${v.type.charAt(0)}${v.type.slice(1).toLowerCase()}`;
	return { text: `${kind} · ${disks}`, risky: false };
}

const STATUS_WORDS: Record<string, string> = {
	ONLINE: 'Online',
	DEGRADED: 'Degraded',
	FAULTED: 'Failed',
	OFFLINE: 'Offline',
	UNAVAIL: 'Missing',
	REMOVED: 'Removed',
	AVAIL: 'Ready',
	INUSE: 'In use'
};

/** A device's state in words. */
export function deviceStatus(status: string): string {
	return STATUS_WORDS[status] ?? `${status.charAt(0)}${status.slice(1).toLowerCase()}`;
}

/** Healthy states: ONLINE, and AVAIL / INUSE for spares. */
const FINE = new Set(['ONLINE', 'AVAIL', 'INUSE']);

/** What is wrong with a device, `null` when nothing: "Missing", "Online · 12 read errors". */
export function deviceProblem(v: Vdev): string | null {
	const { read, write, checksum } = v.errors;
	const parts = [
		read > 0 && plural(read, 'read error'),
		write > 0 && plural(write, 'write error'),
		checksum > 0 && plural(checksum, 'checksum error')
	].filter((p): p is string => typeof p === 'string');
	if (FINE.has(v.status) && parts.length === 0) return null;
	return [deviceStatus(v.status), ...parts].join(' · ');
}

/** Whether any device under `v` (itself included) has a problem. */
export function hasProblem(v: Vdev): boolean {
	return deviceProblem(v) !== null || v.children.some(hasProblem);
}

/** Whether the pool's layout needs a look: a device with a problem, or unprotected data. */
export function topologyNeedsLook(t: Topology): boolean {
	return TOPOLOGY_GROUPS.some((g) => t[g].some((v) => hasProblem(v) || vdevLabel(v, g).risky));
}

/** "4 disks · RAIDZ2", "6 disks · 3 × Mirror", "2 disks · no redundancy": the data layout. */
export function layoutSummary(t: Topology): string {
	const disks = plural(diskCount(t), 'disk');
	const data = t.data;
	if (data.length === 0) return disks;
	if (data.every(isDisk)) return `${disks} · no redundancy`;
	const kinds = new Set(data.map((v) => vdevLabel(v, 'data').text.split(' · ')[0]));
	if (kinds.size !== 1) return `${disks} · mixed layout`;
	const [kind] = kinds;
	return data.length === 1 ? `${disks} · ${kind}` : `${disks} · ${data.length} × ${kind}`;
}

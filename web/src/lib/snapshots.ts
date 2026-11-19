// One dataset's snapshots (#37), from `pool.snapshot.query` (api.truenas.com v25.10, role
// SNAPSHOT_READ). The server turns each property into a number (`truenas/snapshots.rs`). Take a
// snapshot (`pool.snapshot.create`, SNAPSHOT_WRITE) and delete one (`pool.snapshot.delete`,
// SNAPSHOT_DELETE): plain calls, not jobs. Framework-free so it can be unit-tested: the Svelte
// binding lives in snapshots.svelte.ts.
import { hasRole, isNum, isObj, isStr } from './system';
import { type ErrorBody, type EventData, BUSY_MESSAGE } from './ws';

export const SNAPSHOT_QUERY = 'pool.snapshot.query';
export const SNAPSHOT_CREATE = 'pool.snapshot.create';
export const SNAPSHOT_DELETE = 'pool.snapshot.delete';

/** ZFS properties read for each snapshot (`SPACE` and `creation` in `truenas/snapshots.rs`). */
const SNAPSHOT_PROPS = ['used', 'referenced', 'creation'];

/** Longest name accepted (the server's `NAME_MAX`). */
export const NAME_MAX = 200;

/**
 * The snapshots of `dataset` only, not its children's. TrueNAS reads every snapshot of the NAS
 * to filter them (its own FIXME): ask only for the three properties shown.
 */
export function snapshotQueryParams(dataset: string): unknown[] {
	return [
		[['dataset', '=', dataset]],
		{
			select: ['id', 'dataset', 'snapshot_name', 'createtxg', 'properties'],
			extra: { properties: SNAPSHOT_PROPS }
		}
	];
}

export interface Snapshot {
	/** `dataset@name`. */
	id: string;
	/** The part after `@`. */
	name: string;
	/** ZFS transaction group: creation order. `null` when the NAS didn't send it. */
	txg: number | null;
	created: Date | null;
	/** Bytes only this snapshot holds: what deleting it frees. */
	used: number | null;
	/** Bytes of data the snapshot shows (the dataset's size when it was taken). */
	referenced: number | null;
}

const num = (v: unknown): number | null => (isNum(v) && v >= 0 ? v : null);

/** One compacted `pool.snapshot.query` entry, or `null` without a `dataset@name` id. */
export function parseSnapshot(raw: unknown): Snapshot | null {
	if (!isObj(raw) || !isStr(raw.id)) return null;
	const at = raw.id.indexOf('@');
	if (at <= 0 || at === raw.id.length - 1) return null;
	const created = num(raw.created);
	return {
		id: raw.id,
		name: raw.id.slice(at + 1),
		txg: num(raw.txg),
		created: created === null ? null : new Date(created * 1000),
		used: num(raw.used),
		referenced: num(raw.referenced)
	};
}

/** The dataset part of a snapshot id. */
export const datasetOf = (id: string) => id.slice(0, id.indexOf('@'));

/** Newest first: by creation order, then date, then name. */
function newestFirst(a: Snapshot, b: Snapshot): number {
	return (
		(b.txg ?? -1) - (a.txg ?? -1) ||
		(b.created?.getTime() ?? 0) - (a.created?.getTime() ?? 0) ||
		a.name.localeCompare(b.name)
	);
}

/** A query answer as `dataset`'s snapshots, newest first; `null` when it isn't a list. */
export function parseSnapshots(raw: unknown, dataset: string): Snapshot[] | null {
	if (!Array.isArray(raw)) return null;
	return raw
		.map(parseSnapshot)
		.filter((s): s is Snapshot => s !== null && datasetOf(s.id) === dataset)
		.sort(newestFirst);
}

/** `list` with `snap` added, or replacing the one with its id. */
export function withSnapshot(list: readonly Snapshot[], snap: Snapshot): Snapshot[] {
	return [...list.filter((s) => s.id !== snap.id), snap].sort(newestFirst);
}

/**
 * `list` after a `pool.snapshot.query` event, or `null` when the event isn't about `dataset`
 * (every snapshot of the NAS comes on the same topic).
 */
export function applySnapshotEvent(
	list: readonly Snapshot[],
	data: EventData,
	dataset: string
): Snapshot[] | null {
	const id = isStr(data.id) ? data.id : isStr(data.fields?.id) ? data.fields.id : null;
	if (id === null || datasetOf(id) !== dataset) return null;
	if (data.kind === 'removed') return list.filter((s) => s.id !== id);
	if (data.kind !== 'added' && data.kind !== 'changed') return null;
	const snap = parseSnapshot({ ...data.fields, id });
	return snap ? withSnapshot(list, snap) : null;
}

const pad = (n: number) => String(n).padStart(2, '0');

/** `manual-2026-11-19_14-05` in local time, the name TrueNAS suggests. */
export function defaultSnapshotName(date: Date): string {
	return (
		`manual-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}` +
		`_${pad(date.getHours())}-${pad(date.getMinutes())}`
	);
}

/** What's wrong with a new snapshot's name, in plain words; `null` when it can be taken. */
export function snapshotNameProblem(name: string, existing: readonly Snapshot[]): string | null {
	if (name === '') return 'Give the snapshot a name.';
	if (name.length > NAME_MAX) return `Use at most ${NAME_MAX} characters.`;
	if (name.startsWith(' ') || name.endsWith(' ')) return 'Remove the spaces at the start or end.';
	if (!/^[A-Za-z0-9_.: -]+$/.test(name)) {
		return 'Use only letters, digits, spaces and - _ . :';
	}
	if (existing.some((s) => s.name === name)) return 'A snapshot with this name already exists.';
	return null;
}

/** Roles that may take snapshots. */
export const canTakeSnapshots = (roles: readonly string[] | undefined) =>
	hasRole(roles, 'SNAPSHOT_WRITE');

/** Roles that may delete snapshots (a separate role in TrueNAS). */
export const canDeleteSnapshots = (roles: readonly string[] | undefined) =>
	hasRole(roles, 'SNAPSHOT_DELETE');

/** The `/storage/snapshots` link of a dataset. */
export const snapshotsHref = (dataset: string) =>
	`/storage/snapshots?dataset=${encodeURIComponent(dataset)}`;

export type SnapshotAction = 'take' | 'delete';

/** A failed take or delete, in plain words. */
export function snapshotError(action: SnapshotAction, error: ErrorBody): string {
	const what = action === 'take' ? 'take the snapshot' : 'delete the snapshot';
	if (error.errname === 'EACCES') {
		return action === 'take'
			? "Your account can't take snapshots. It needs the Snapshot write role (SNAPSHOT_WRITE) on the NAS."
			: "Your account can't delete snapshots. It needs the Snapshot delete role (SNAPSHOT_DELETE) on the NAS.";
	}
	if (action === 'delete' && error.errname === 'EBUSY') {
		return "Couldn't delete the snapshot: something still uses it (a hold for replication, or a clone). Free it in TrueNAS first.";
	}
	switch (error.code) {
		case 'disconnected':
		case 'reconnecting':
			return `The connection dropped while truelite asked the NAS to ${what}. Check the list once it's back.`;
		case 'busy':
			return BUSY_MESSAGE;
		default:
			return `Couldn't ${what}: ${error.message}`;
	}
}

// The dataset tree of the Storage page, from `pool.dataset.query` (api.truenas.com v25.10, role
// DATASET_READ). The server flattens each space property to bytes (`truenas/datasets.rs`); the
// list comes flat and the tree is built here. Framework-free so it can be unit-tested: the
// Svelte binding lives in datasets.svelte.ts.
import { bytesOf, fullLevel, type Level } from './pools';
import { isObj, isStr } from './system';

export const DATASET_QUERY = 'pool.dataset.query';

/** Every dataset in one flat list, with only the properties the tree shows. */
export const DATASET_TREE_PARAMS: readonly unknown[] = [
	[],
	{
		select: [
			'id',
			'type',
			'encrypted',
			'locked',
			'key_loaded',
			'used',
			'available',
			'quota',
			'refquota'
		],
		extra: {
			flat: true,
			retrieve_user_props: false,
			properties: ['used', 'available', 'quota', 'refquota']
		}
	}
];

export interface Dataset {
	/** Full path, pool first: `tank/media/photos`. */
	id: string;
	/** Last part of the path. */
	name: string;
	/** A volume (zvol: a block device for a VM or iSCSI) rather than a file system. */
	volume: boolean;
	/** Bytes, snapshots and children included. */
	used: number;
	/** Bytes still writable here (smaller under a quota). */
	available: number;
	/** The tighter of `quota` and `refquota`, bytes; `null` when neither is set. */
	quota: number | null;
	encrypted: boolean;
	/** Encrypted and its key isn't loaded: the data can't be read. */
	locked: boolean;
}

export interface DatasetNode extends Dataset {
	children: DatasetNode[];
}

export interface DatasetTree {
	/** One per pool, by full path. Shown with their `id`. */
	roots: DatasetNode[];
	/** Internal datasets TrueNAS keeps for itself, not shown (topmost ones only). */
	hidden: number;
}

const positive = (n: number | null) => (n !== null && n > 0 ? n : null);

/** One flattened `pool.dataset.query` entry, or `null` without an `id` or `used`. */
export function parseDataset(raw: unknown): Dataset | null {
	if (!isObj(raw) || !isStr(raw.id) || raw.id === '') return null;
	const used = bytesOf(raw.used);
	if (used === null) return null;
	const limits = [positive(bytesOf(raw.quota)), positive(bytesOf(raw.refquota))].filter(
		(n): n is number => n !== null
	);
	const encrypted = raw.encrypted === true;
	return {
		id: raw.id,
		name: raw.id.slice(raw.id.lastIndexOf('/') + 1),
		volume: isStr(raw.type) && raw.type.toUpperCase() === 'VOLUME',
		used,
		available: Math.max(0, bytesOf(raw.available) ?? 0),
		quota: limits.length > 0 ? Math.min(...limits) : null,
		encrypted,
		locked: raw.locked === true || (encrypted && raw.key_loaded === false)
	};
}

/** Datasets TrueNAS keeps for itself: the system dataset, apps, containers. */
const INTERNAL = new Set(['ix-applications']);

/** Whether `id` is, or is under, an internal dataset (a dot-named part, or `ix-applications`). */
export function isInternal(id: string): boolean {
	return id
		.split('/')
		.slice(1)
		.some((part) => part.startsWith('.') || INTERNAL.has(part));
}

const byName = (a: DatasetNode, b: DatasetNode) =>
	a.name.localeCompare(b.name, undefined, { numeric: true });

function sortTree(nodes: DatasetNode[]): void {
	nodes.sort(byName);
	for (const n of nodes) sortTree(n.children);
}

const byId = (a: DatasetNode, b: DatasetNode) =>
	a.id.localeCompare(b.id, undefined, { numeric: true });

/**
 * The tree of a flat dataset list, children sorted by name. Internal datasets and everything
 * under them are left out and counted (topmost only). A dataset whose parent is missing hangs
 * under its nearest known ancestor, or becomes a root. `null` when `raw` isn't a list.
 */
export function buildTree(raw: unknown): DatasetTree | null {
	if (!Array.isArray(raw)) return null;
	const list = raw.map(parseDataset).filter((d): d is Dataset => d !== null);
	const nodes = new Map<string, DatasetNode>();
	let hidden = 0;
	for (const d of list) {
		if (!isInternal(d.id)) {
			nodes.set(d.id, { ...d, children: [] });
		} else if (!isInternal(d.id.slice(0, d.id.lastIndexOf('/')))) {
			hidden++;
		}
	}
	const roots: DatasetNode[] = [];
	for (const node of nodes.values()) {
		let parent: DatasetNode | undefined;
		for (let at = node.id.lastIndexOf('/'); at > 0 && !parent;) {
			const up = node.id.slice(0, at);
			parent = nodes.get(up);
			at = up.lastIndexOf('/');
		}
		(parent ? parent.children : roots).push(node);
	}
	for (const r of roots) sortTree(r.children);
	// By full path: an orphan root (`other/x`) is shown with it.
	roots.sort(byId);
	return { roots, hidden };
}

export interface DatasetUsage {
	/** 0–100 of what this dataset may hold (`used + available`). */
	percent: number;
	level: Level;
}

/**
 * How full a dataset is: `used` of `used + available`. Under a quota that is about the quota;
 * without one, the pool's free space.
 */
export function datasetUsage(d: Dataset): DatasetUsage | null {
	const total = d.used + d.available;
	if (total <= 0) return null;
	const percent = Math.min(100, (d.used / total) * 100);
	return { percent, level: fullLevel(percent) };
}

/** Locked, or a quota nearly reached. A full pool is the pool card's business. */
export function needsLook(d: Dataset): boolean {
	if (d.locked) return true;
	return d.quota !== null && (datasetUsage(d)?.level ?? 'ok') !== 'ok';
}

/** Ids of the datasets to show opened: roots, and every ancestor of one that needs a look. */
export function openIds(roots: readonly DatasetNode[]): Set<string> {
	const open = new Set<string>();
	const walk = (n: DatasetNode): boolean => {
		let below = false;
		for (const c of n.children) below = walk(c) || below;
		if (below) open.add(n.id);
		return below || needsLook(n);
	};
	for (const r of roots) {
		walk(r);
		open.add(r.id);
	}
	return open;
}

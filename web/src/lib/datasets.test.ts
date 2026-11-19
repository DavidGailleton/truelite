import { describe, expect, it } from 'vitest';
import {
	DATASET_TREE_PARAMS,
	buildTree,
	datasetUsage,
	isInternal,
	needsLook,
	openIds,
	parseDataset,
	type DatasetNode
} from './datasets';

const GB = 1e9;

// As the server forwards them (fixture of server/src/truenas/datasets.rs, flattened).
const ds = (id: string, extra: Record<string, unknown> = {}) => ({
	id,
	type: 'FILESYSTEM',
	encrypted: false,
	locked: false,
	key_loaded: null,
	used: 10 * GB,
	available: 90 * GB,
	quota: 0,
	refquota: 0,
	...extra
});

const ids = (nodes: DatasetNode[]): unknown[] =>
	nodes.map((n) => (n.children.length ? [n.name, ids(n.children)] : n.name));

describe('parseDataset', () => {
	it('reads a flattened entry', () => {
		expect(parseDataset(ds('tank/media/photos', { quota: 200 * GB, refquota: 150 * GB }))).toEqual({
			id: 'tank/media/photos',
			name: 'photos',
			volume: false,
			used: 10 * GB,
			available: 90 * GB,
			quota: 150 * GB,
			encrypted: false,
			locked: false
		});
	});

	it('reads property objects, volumes and locks', () => {
		const d = parseDataset({
			id: 'tank/vm',
			type: 'VOLUME',
			used: { parsed: 5 },
			available: { rawvalue: '7' },
			encrypted: true,
			key_loaded: false
		});
		expect(d).toMatchObject({ name: 'vm', volume: true, used: 5, available: 7, quota: null });
		expect(d?.locked).toBe(true);
	});

	it('rejects entries without an id or used space', () => {
		expect(parseDataset({ id: 'tank' })).toBeNull();
		expect(parseDataset({ used: 1 })).toBeNull();
		expect(parseDataset(null)).toBeNull();
	});
});

describe('isInternal', () => {
	it('finds the datasets TrueNAS keeps for itself', () => {
		for (const id of [
			'tank/.system',
			'tank/.system/cores',
			'tank/.ix-apps',
			'tank/ix-applications/x'
		]) {
			expect(isInternal(id), id).toBe(true);
		}
		for (const id of ['tank', '.tank', 'tank/media', 'tank/media.old', 'tank/my-ix-applications']) {
			expect(isInternal(id), id).toBe(false);
		}
	});
});

describe('buildTree', () => {
	it('nests by path, sorts by name and hides internal datasets', () => {
		const tree = buildTree([
			ds('tank'),
			ds('tank/media'),
			ds('tank/media/photo10'),
			ds('tank/media/photo2'),
			ds('tank/backups'),
			ds('tank/.system'),
			ds('tank/.system/cores'),
			ds('tank/.ix-apps'),
			ds('fast'),
			{ junk: true }
		]);
		expect(ids(tree!.roots)).toEqual([
			'fast',
			['tank', ['backups', ['media', ['photo2', 'photo10']]]]
		]);
		expect(tree!.hidden).toBe(2);
	});

	it('hangs an orphan under its nearest ancestor', () => {
		const tree = buildTree([ds('tank/a/b/c'), ds('tank'), ds('other/x')]);
		expect(ids(tree!.roots)).toEqual(['x', ['tank', ['c']]]); // by path: other/x, tank
	});

	it('refuses a result that is not a list', () => {
		expect(buildTree({})).toBeNull();
		expect(buildTree([])).toEqual({ roots: [], hidden: 0 });
	});
});

describe('usage and attention', () => {
	const d = (extra: Record<string, unknown>) => parseDataset(ds('tank/x', extra))!;

	it('measures used of used + available', () => {
		expect(datasetUsage(d({}))).toEqual({ percent: 10, level: 'ok' });
		expect(datasetUsage(d({ used: 85, available: 15 }))?.level).toBe('warning');
		expect(datasetUsage(d({ used: 95, available: 5 }))?.level).toBe('critical');
		expect(datasetUsage(d({ used: 0, available: 0 }))).toBeNull();
	});

	it('flags locked datasets and nearly full quotas only', () => {
		expect(needsLook(d({}))).toBe(false);
		expect(needsLook(d({ locked: true }))).toBe(true);
		expect(needsLook(d({ used: 85, available: 15 }))).toBe(false); // pool space: the pool card's
		expect(needsLook(d({ used: 85, available: 15, quota: 100 }))).toBe(true);
	});

	it('opens roots and the ancestors of what needs a look', () => {
		const tree = buildTree([
			ds('tank'),
			ds('tank/a'),
			ds('tank/a/b'),
			ds('tank/a/b/locked', { locked: true }),
			ds('tank/c'),
			ds('tank/c/d')
		])!;
		expect([...openIds(tree.roots)].sort()).toEqual(['tank', 'tank/a', 'tank/a/b']);
	});
});

it('asks for one flat list with four properties', () => {
	const opts = DATASET_TREE_PARAMS[1] as { extra: { flat: boolean; properties: string[] } };
	expect(opts.extra.flat).toBe(true);
	expect(opts.extra.properties).toEqual(['used', 'available', 'quota', 'refquota']);
});

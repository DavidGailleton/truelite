import { describe, expect, it } from 'vitest';
import {
	applySnapshotEvent,
	canDeleteSnapshots,
	canTakeSnapshots,
	defaultSnapshotName,
	parseSnapshot,
	parseSnapshots,
	snapshotError,
	snapshotNameProblem,
	snapshotQueryParams,
	snapshotsHref,
	type Snapshot
} from './snapshots';

// As the server forwards them (fixture of server/src/truenas/snapshots.rs, compacted).
const raw = (name: string, txg: number, extra: Record<string, unknown> = {}) => ({
	id: `tank/media@${name}`,
	dataset: 'tank/media',
	snapshot_name: name,
	txg,
	used: 1024,
	referenced: 5e10,
	created: 1_795_082_400,
	...extra
});

const snap = (name: string, txg: number): Snapshot => parseSnapshot(raw(name, txg))!;

describe('snapshotQueryParams', () => {
	it('asks for one dataset and three properties', () => {
		expect(snapshotQueryParams('tank/media')).toEqual([
			[['dataset', '=', 'tank/media']],
			{
				select: ['id', 'dataset', 'snapshot_name', 'createtxg', 'properties'],
				extra: { properties: ['used', 'referenced', 'creation'] }
			}
		]);
	});
});

describe('parseSnapshot', () => {
	it('reads a compacted entry', () => {
		expect(parseSnapshot(raw('a', 7))).toEqual({
			id: 'tank/media@a',
			name: 'a',
			txg: 7,
			created: new Date(1_795_082_400_000),
			used: 1024,
			referenced: 5e10
		});
	});

	it('keeps missing numbers as null and refuses ids without a name', () => {
		const s = parseSnapshot({ id: 'tank@x', used: -1, created: 'soon' })!;
		expect([s.txg, s.used, s.created, s.referenced]).toEqual([null, null, null, null]);
		for (const bad of [null, 'x', { id: 'tank' }, { id: '@x' }, { id: 'tank@' }, { id: 3 }]) {
			expect(parseSnapshot(bad)).toBeNull();
		}
	});
});

describe('parseSnapshots', () => {
	it('keeps this dataset only, newest first', () => {
		const list = parseSnapshots(
			[raw('old', 1), raw('new', 9), { ...raw('child', 20), id: 'tank/media/x@child' }, 'junk'],
			'tank/media'
		)!;
		expect(list.map((s) => s.name)).toEqual(['new', 'old']);
	});

	it('orders by date, then name, without a txg', () => {
		const list = parseSnapshots(
			[
				raw('b', 0, { txg: null, created: 10 }),
				raw('a', 0, { txg: null, created: 10 }),
				raw('c', 0, { txg: null, created: 20 })
			],
			'tank/media'
		)!;
		expect(list.map((s) => s.name)).toEqual(['c', 'a', 'b']);
	});

	it('refuses an answer that is not a list', () => {
		expect(parseSnapshots({}, 'tank')).toBeNull();
	});
});

describe('applySnapshotEvent', () => {
	const list = [snap('b', 2), snap('a', 1)];

	it('adds, replaces and removes this dataset’s snapshots', () => {
		const added = applySnapshotEvent(
			list,
			{ kind: 'added', id: 'tank/media@c', fields: raw('c', 3) },
			'tank/media'
		)!;
		expect(added.map((s) => s.name)).toEqual(['c', 'b', 'a']);
		const changed = applySnapshotEvent(
			added,
			{ kind: 'changed', id: 'tank/media@c', fields: { ...raw('c', 3), used: 9 } },
			'tank/media'
		)!;
		expect(changed).toHaveLength(3);
		expect(changed[0].used).toBe(9);
		const removed = applySnapshotEvent(
			changed,
			{ kind: 'removed', id: 'tank/media@b' },
			'tank/media'
		)!;
		expect(removed.map((s) => s.name)).toEqual(['c', 'a']);
	});

	it('ignores other datasets and odd events', () => {
		const other = { kind: 'added', id: 'tank/media/x@c', fields: raw('c', 3) } as const;
		expect(applySnapshotEvent(list, other, 'tank/media')).toBeNull();
		expect(
			applySnapshotEvent(list, { kind: 'other', id: 'tank/media@a' }, 'tank/media')
		).toBeNull();
		expect(applySnapshotEvent(list, { kind: 'added' }, 'tank/media')).toBeNull();
	});

	it('takes the id from the fields when the event has none', () => {
		const next = applySnapshotEvent(list, { kind: 'added', fields: raw('c', 3) }, 'tank/media');
		expect(next?.[0].name).toBe('c');
	});
});

describe('snapshot names', () => {
	it('suggests a TrueNAS-like name in local time', () => {
		expect(defaultSnapshotName(new Date(2026, 0, 5, 9, 7))).toBe('manual-2026-01-05_09-07');
	});

	it('explains what is wrong', () => {
		const existing = [snap('taken', 1)];
		expect(snapshotNameProblem('manual-2026-01-05_09-07', existing)).toBeNull();
		expect(snapshotNameProblem('before update: v2.1', existing)).toBeNull();
		expect(snapshotNameProblem('', existing)).toMatch(/name/);
		expect(snapshotNameProblem(' x', existing)).toMatch(/spaces/);
		expect(snapshotNameProblem('a/b', existing)).toMatch(/letters/);
		expect(snapshotNameProblem('a@b', existing)).toMatch(/letters/);
		expect(snapshotNameProblem('é', existing)).toMatch(/letters/);
		expect(snapshotNameProblem('x'.repeat(201), existing)).toMatch(/200/);
		expect(snapshotNameProblem('taken', existing)).toMatch(/exists/);
	});
});

describe('roles', () => {
	it('separates taking and deleting', () => {
		expect(canTakeSnapshots(['SNAPSHOT_WRITE'])).toBe(true);
		expect(canDeleteSnapshots(['SNAPSHOT_WRITE'])).toBe(false);
		expect(canDeleteSnapshots(['SNAPSHOT_DELETE'])).toBe(true);
		expect(canTakeSnapshots(['FULL_ADMIN']) && canDeleteSnapshots(['FULL_ADMIN'])).toBe(true);
		expect(canTakeSnapshots(['READONLY_ADMIN']) || canTakeSnapshots(undefined)).toBe(false);
	});
});

describe('snapshotsHref', () => {
	it('encodes the dataset path', () => {
		expect(snapshotsHref('tank/my media')).toBe('/storage/snapshots?dataset=tank%2Fmy%20media');
	});
});

describe('snapshotError', () => {
	it('speaks plainly', () => {
		expect(snapshotError('take', { code: 'nas', message: 'x', errname: 'EACCES' })).toMatch(
			/SNAPSHOT_WRITE/
		);
		expect(snapshotError('delete', { code: 'nas', message: 'x', errname: 'EACCES' })).toMatch(
			/SNAPSHOT_DELETE/
		);
		expect(snapshotError('delete', { code: 'nas', message: 'x', errname: 'EBUSY' })).toMatch(
			/still uses it/
		);
		expect(snapshotError('take', { code: 'nas', message: 'out of space' })).toBe(
			"Couldn't take the snapshot: out of space"
		);
		expect(snapshotError('delete', { code: 'disconnected', message: '' })).toMatch(/dropped/);
	});
});

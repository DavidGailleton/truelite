import { beforeEach, describe, expect, it, vi } from 'vitest';
import { NasError } from './ws';

const calls: { method: string; params: unknown[] }[] = [];
let answer: (method: string, params: unknown[]) => Promise<unknown>;

vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({
		call: (method: string, params: unknown[]) => {
			calls.push({ method, params });
			return answer(method, params);
		}
	})
}));

const { SnapshotStore } = await import('./snapshots.svelte');

const entry = (name: string, txg: number, dataset = 'tank') => ({
	id: `${dataset}@${name}`,
	txg,
	used: 1,
	created: 100
});
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
	calls.length = 0;
	answer = (method) =>
		Promise.resolve(method === 'pool.snapshot.query' ? [entry('a', 1), entry('b', 2)] : true);
});

describe('SnapshotStore', () => {
	it('reads on subscribe and patches events in', async () => {
		const s = new SnapshotStore('tank');
		s.onMessage({ status: 'subscribed' });
		await settle();
		expect(s.list?.map((x) => x.name)).toEqual(['b', 'a']);
		expect(calls[0].params[0]).toEqual([['dataset', '=', 'tank']]);
		s.onMessage({ data: { kind: 'added', id: 'tank@c', fields: entry('c', 3) } });
		s.onMessage({ data: { kind: 'added', id: 'tank/x@d', fields: entry('d', 4, 'tank/x') } });
		s.onMessage({ data: { kind: 'removed', id: 'tank@a' } });
		expect(s.list?.map((x) => x.name)).toEqual(['c', 'b']);
		expect(calls).toHaveLength(1);
	});

	it('reads again only when stale', async () => {
		const s = new SnapshotStore('tank');
		s.loadIfStale();
		expect(calls).toHaveLength(0);
		s.load();
		await settle();
		s.loadIfStale();
		expect(calls).toHaveLength(1);
		s.loadIfStale(performance.now() + 61_000);
		expect(calls).toHaveLength(2);
	});

	it('takes a snapshot and shows it before its event', async () => {
		const s = new SnapshotStore('tank');
		s.load();
		await settle();
		answer = () => Promise.resolve(entry('new', 9));
		const done = s.take('new', true);
		expect(s.run).toEqual({ action: 'take', name: 'new' });
		await done;
		expect(s.run).toBeNull();
		expect(calls[1]).toEqual({
			method: 'pool.snapshot.create',
			params: [{ dataset: 'tank', name: 'new', recursive: true }]
		});
		expect(s.list?.[0].name).toBe('new');
	});

	it('deletes one snapshot and explains a refusal', async () => {
		const s = new SnapshotStore('tank');
		s.load();
		await settle();
		const [b, a] = s.list!;
		await s.delete(b);
		expect(calls[1]).toEqual({ method: 'pool.snapshot.delete', params: ['tank@b'] });
		expect(s.list?.map((x) => x.name)).toEqual(['a']);
		answer = () => Promise.reject(new NasError({ code: 'nas', message: 'busy', errname: 'EBUSY' }));
		await s.delete(a);
		expect(s.runError).toMatch(/still uses it/);
		expect(s.list?.map((x) => x.name)).toEqual(['a']);
	});

	it('runs one action at a time', async () => {
		const s = new SnapshotStore('tank');
		const first = s.take('x', false);
		await s.take('y', false);
		await first;
		expect(calls.filter((c) => c.method === 'pool.snapshot.create')).toHaveLength(1);
	});

	it('shows a read error and an ended topic', async () => {
		const s = new SnapshotStore('tank');
		answer = () => Promise.reject(new NasError({ code: 'nas', message: 'no' }));
		s.load();
		await settle();
		expect(s.error?.message).toBe('no');
		s.onMessage({ status: 'ended', error: { code: 'not_allowed', message: 'off' } });
		expect(s.error?.code).toBe('not_allowed');
	});
});

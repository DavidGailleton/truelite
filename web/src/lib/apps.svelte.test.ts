import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

interface Pending {
	method: string;
	params: unknown;
	resolve(value: unknown): void;
	reject(error: unknown): void;
}

// Every call waits until the test answers it.
const calls: Pending[] = [];
vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({
		call: (method: string, params: unknown) =>
			new Promise((resolve, reject) => calls.push({ method, params, resolve, reject }))
	})
}));

const { apps, followApps, loadApps, onAppMessage } = await import('./apps.svelte');
const { runAction } = await import('./app-actions.svelte');
const { closeNas } = await import('./nas.svelte');

const raw = (id: string, version: string, upgrade: boolean, state = 'RUNNING') => ({
	id,
	name: id,
	state,
	upgrade_available: upgrade,
	image_updates_available: false,
	custom_app: false,
	version: `1.0.${version}`,
	human_version: `${version}_1.0.${version}`
});

const queries = () => calls.filter((c) => c.method === 'app.query');
const lastQuery = () => queries().at(-1)!;
/** Lets the promise callbacks run. */
const flush = () => new Promise<void>((r) => setTimeout(r, 0));

beforeEach(() => {
	closeNas();
	calls.length = 0;
});
let unfollow = () => {};
afterEach(() => {
	unfollow();
	unfollow = () => {};
	vi.useRealTimers();
	closeNas();
});

describe('apps list', () => {
	it('drops an answer older than an event and queries again', async () => {
		loadApps();
		lastQuery().resolve([raw('a', '1', true)]);
		await flush();
		loadApps(); // e.g. after a missed event
		const stale = lastQuery();
		onAppMessage({ data: { kind: 'changed', id: 'a', fields: raw('a', '2', false) } });
		expect(apps.list?.[0].version).toBe('2');
		stale.resolve([raw('a', '1', true)]);
		await flush();
		// The stale answer did not overwrite the event; one more query is on its way.
		expect(apps.list?.[0]).toMatchObject({ version: '2', update: null });
		expect(queries()).toHaveLength(3);
		lastQuery().resolve([raw('a', '2', false)]);
		await flush();
		expect(apps.list?.[0]).toMatchObject({ version: '2', update: null });
	});

	it('keeps one query in flight', async () => {
		loadApps();
		loadApps();
		loadApps();
		loadApps();
		expect(queries()).toHaveLength(1);
		lastQuery().resolve([]);
		await flush();
		expect(queries()).toHaveLength(2);
		lastQuery().resolve([raw('a', '1', false)]);
		await flush();
		expect(queries()).toHaveLength(2);
		expect(apps.list).toHaveLength(1);
	});

	it('ignores an answer of a closed link', async () => {
		loadApps();
		const old = lastQuery();
		closeNas();
		old.resolve([raw('a', '1', false)]);
		await flush();
		expect(apps.list).toBeNull();
		loadApps();
		expect(queries()).toHaveLength(2);
	});

	for (const order of [
		['a', 'b'],
		['b', 'a']
	]) {
		it(`refreshes every app after overlapping updates ending ${order.join(' then ')}`, async () => {
			vi.useFakeTimers();
			unfollow = followApps();
			loadApps();
			lastQuery().resolve([raw('a', '1', true), raw('b', '1', true)]);
			await vi.advanceTimersByTimeAsync(0);
			const job = (id: string) => ({ method: 'app.upgrade', params: [id] });
			runAction('a', 'update', job('a'), 'running');
			runAction('b', 'update', job('b'), 'running');
			const jobs = new Map(
				calls.filter((c) => c.method === 'app.upgrade').map((c) => [(c.params as string[])[0], c])
			);
			const updated: Record<string, boolean> = { a: false, b: false };
			for (const id of order) {
				updated[id] = true;
				jobs.get(id)!.resolve(null);
				await vi.advanceTimersByTimeAsync(2_000);
				// No event: the job's end queries the list. Answer with the NAS's state now.
				for (let q = lastQuery(); ;) {
					q.resolve(['a', 'b'].map((x) => raw(x, updated[x] ? '2' : '1', !updated[x])));
					await vi.advanceTimersByTimeAsync(0);
					if (lastQuery() === q) break;
					q = lastQuery();
				}
			}
			expect(apps.list).toEqual([
				expect.objectContaining({ id: 'a', version: '2', update: null }),
				expect.objectContaining({ id: 'b', version: '2', update: null })
			]);
		});
	}

	it('does not query on a job end when no page follows the list', async () => {
		vi.useFakeTimers();
		loadApps();
		lastQuery().resolve([raw('a', '1', true)]);
		await vi.advanceTimersByTimeAsync(0);
		runAction('a', 'update', { method: 'app.upgrade', params: ['a'] }, 'running');
		calls.find((c) => c.method === 'app.upgrade')!.resolve(null);
		await vi.advanceTimersByTimeAsync(2_000);
		expect(queries()).toHaveLength(1);
	});
});

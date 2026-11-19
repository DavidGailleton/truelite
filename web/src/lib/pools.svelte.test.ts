import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TopicMessage } from './ws';

const calls: { method: string; params: unknown[] }[] = [];
const handlers = new Map<string, (m: TopicMessage) => void>();
const topology = { data: [], log: [], cache: [], spare: [], special: [], dedup: [] };

vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({
		call: (method: string, params: unknown[]) => {
			calls.push({ method, params });
			const opts = params[1] as { select?: string[] };
			if (method === 'pool.dataset.query') return Promise.resolve([]);
			if (opts.select?.includes('topology')) {
				return Promise.resolve([{ id: 1, topology }]);
			}
			return Promise.resolve([{ id: 1, name: 'tank', status: 'ONLINE' }]);
		}
	}),
	useTopic: (topic: string, h: (m: TopicMessage) => void) => {
		handlers.set(topic, h);
		return { retry() {} };
	}
}));

const { layouts, pools, usePools, wantLayouts } = await import('./pools.svelte');

// Vitest compiles `.svelte.ts` for the server here, so `$effect` does not run: `usePools()` only
// registers its topics (mocked `useTopic`), and `wantLayouts()` stands in for the page's hook.
usePools();
const { closeNas } = await import('./nas.svelte');

const layoutCalls = () =>
	calls.filter((c) => (c.params[1] as { select?: string[] }).select?.includes('topology')).length;
const settle = () => vi.advanceTimersByTimeAsync(0);
const subscribed = () => handlers.get('pool.query')!({ status: 'subscribed' });

beforeEach(() => {
	vi.useFakeTimers();
	calls.length = 0;
});
afterEach(() => {
	closeNas();
	vi.useRealTimers();
});

describe('pool layouts', () => {
	it('are never read for the dashboard alone', async () => {
		subscribed();
		await settle();
		expect(pools.list).toHaveLength(1);
		expect(layoutCalls()).toBe(0);
	});

	it('are read with the pool list on the pools page, and again after a change', async () => {
		const release = wantLayouts();
		expect(layoutCalls()).toBe(0); // no list yet: loadPools reads them
		subscribed();
		await settle();
		expect(layoutCalls()).toBe(1);
		expect(layouts.map?.get(1)?.data).toEqual([]);

		handlers.get('pool.query')!({
			data: { kind: 'changed', id: 1, fields: { id: 1, name: 'tank', status: 'DEGRADED' } }
		});
		await vi.advanceTimersByTimeAsync(999);
		expect(layoutCalls()).toBe(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(layoutCalls()).toBe(2);
		release();
		handlers.get('pool.query')!({
			data: { kind: 'changed', id: 1, fields: { id: 1, name: 'tank', status: 'ONLINE' } }
		});
		await vi.advanceTimersByTimeAsync(2_000);
		expect(layoutCalls()).toBe(2); // page left: no more reads
	});

	it('are read at once when the list is already known, and forgotten on logout', async () => {
		subscribed();
		await settle();
		const release = wantLayouts();
		await settle();
		expect(layoutCalls()).toBe(1);
		release();
		closeNas();
		expect(layouts.map).toBeNull();
	});
});

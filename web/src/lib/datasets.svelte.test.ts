import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NasError, type TopicMessage } from './ws';

const calls: string[] = [];
const handlers = new Map<string, (m: TopicMessage) => void>();
let denied = false;

vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({
		call: (method: string, params: unknown[]) => {
			// The pools store reads root usage with the same method; only the tree asks for `flat`.
			const flat = (params[1] as { extra?: { flat?: boolean } }).extra?.flat === true;
			calls.push(flat ? 'tree' : method);
			if (method === 'pool.dataset.query') {
				if (denied) {
					return Promise.reject(
						new NasError({ code: 'nas', message: 'Not authorized', errname: 'EACCES' })
					);
				}
				return Promise.resolve([
					{ id: 'tank', used: 1, available: 9 },
					{ id: 'tank/.system', used: 1, available: 9 }
				]);
			}
			return Promise.resolve([{ id: 1, name: 'tank', status: 'ONLINE' }]);
		}
	}),
	useTopic: (topic: string, h: (m: TopicMessage) => void) => {
		handlers.set(topic, h);
		return { retry() {} };
	}
}));

const { usePools } = await import('./pools.svelte');
const { datasets, useDatasets, wantDatasets } = await import('./datasets.svelte');
const { closeNas } = await import('./nas.svelte');

// `$effect` does not run here (see pools.svelte.test.ts): the hooks only register their topics
// (mocked `useTopic`), and `wantDatasets()` stands in for the page's effect.
usePools();
useDatasets();

const reads = () => calls.filter((m) => m === 'tree').length;
const settle = () => vi.advanceTimersByTimeAsync(0);
const send = (topic: string, m: TopicMessage) => handlers.get(topic)!(m);
const poolChanged = () =>
	send('pool.query', {
		data: { kind: 'changed', id: 1, fields: { id: 1, name: 'tank', status: 'ONLINE' } }
	});

const mount = async () => wantDatasets();

beforeEach(() => {
	vi.useFakeTimers();
	calls.length = 0;
	denied = false;
	send('pool.query', { status: 'subscribed' });
});
afterEach(() => {
	closeNas();
	vi.useRealTimers();
});

describe('dataset tree', () => {
	it('is read when the subscription starts, internal datasets hidden', async () => {
		const release = await mount();
		await settle();
		calls.length = 0;
		send('pool.dataset.query', { status: 'subscribed' });
		await settle();
		expect(reads()).toBe(1);
		expect(datasets.tree?.roots.map((r) => r.id)).toEqual(['tank']);
		expect(datasets.tree?.hidden).toBe(1);
		release();
	});

	it('is read again once after a burst of dataset or pool changes', async () => {
		const release = await mount();
		await settle();
		calls.length = 0;
		send('pool.dataset.query', { data: { kind: 'added', id: 'tank/a', fields: {} } });
		send('pool.dataset.query', { data: { kind: 'changed', id: 'tank/a', fields: {} } });
		poolChanged();
		await vi.advanceTimersByTimeAsync(1_000);
		expect(reads()).toBe(1);
		release();
	});

	it('is not read after a pool change once the page is gone', async () => {
		const release = await mount();
		release();
		release(); // twice is harmless
		await settle();
		calls.length = 0;
		poolChanged();
		await vi.advanceTimersByTimeAsync(2_000);
		expect(reads()).toBe(0);
	});

	it('stops asking after a missing role', async () => {
		denied = true;
		const release = await mount();
		send('pool.dataset.query', { status: 'subscribed' });
		await settle();
		expect(datasets.error?.errname).toBe('EACCES');
		calls.length = 0;
		poolChanged();
		await vi.advanceTimersByTimeAsync(2_000);
		expect(reads()).toBe(0);
		release();
	});

	it('is forgotten when the NAS link closes', async () => {
		const release = await mount();
		send('pool.dataset.query', { status: 'subscribed' });
		await settle();
		expect(datasets.tree).not.toBeNull();
		closeNas();
		expect(datasets.tree).toBeNull();
		release();
	});
});

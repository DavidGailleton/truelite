import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
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

const { DiskStore, TEMPS_EVERY_MS } = await import('./disks.svelte');

const ANSWERS: Record<string, unknown> = {
	'disk.query': [{ name: 'sda', pool: 'tank' }, { name: 'sdb' }],
	'disk.temperatures': { sda: { temp: 36, crit: 60 } },
	'disk.temperature_agg': { sda: { min: 30, max: 41 } }
};
const settle = () => vi.advanceTimersByTimeAsync(0);
const methods = () => calls.map((c) => c.method);

beforeEach(() => {
	vi.useFakeTimers();
	calls.length = 0;
	answer = (method) => Promise.resolve(ANSWERS[method]);
});
afterEach(() => vi.useRealTimers());

describe('DiskStore', () => {
	it('reads the list, the temperatures, then the 7-day ranges of the listed disks', async () => {
		const s = new DiskStore();
		s.load();
		await settle();
		expect(methods()).toEqual(['disk.query', 'disk.temperatures', 'disk.temperature_agg']);
		expect(calls[1].params).toEqual([[], true]);
		expect(calls[2].params).toEqual([['sda', 'sdb'], 7]);
		expect(s.list?.map((d) => d.name)).toEqual(['sda', 'sdb']);
		expect(s.temps?.get('sda')).toEqual({ temp: 36, crit: 60 });
		expect(s.ranges?.get('sda')).toEqual({ min: 30, max: 41 });
	});

	it('drops a late answer', async () => {
		const s = new DiskStore();
		let release: (v: unknown) => void = () => {};
		answer = () => new Promise((r) => (release = r));
		s.loadList();
		const first = release;
		answer = (method) => Promise.resolve(ANSWERS[method]);
		s.loadList();
		await settle();
		first([{ name: 'old' }]);
		await settle();
		expect(s.list?.map((d) => d.name)).toEqual(['sda', 'sdb']);
	});

	it('keeps a temperature error apart from the list', async () => {
		answer = (method) =>
			method === 'disk.temperatures'
				? Promise.reject(
						new NasError({ code: 'nas', message: 'Not authorized', errname: 'EACCES' })
					)
				: Promise.resolve(ANSWERS[method]);
		const s = new DiskStore();
		s.load();
		await settle();
		expect(s.list).toHaveLength(2);
		expect(s.error).toBeNull();
		expect(s.tempError?.errname).toBe('EACCES');
		expect(s.temps).toBeNull();
	});

	it('explains an unreadable answer', async () => {
		answer = () => Promise.resolve('odd');
		const s = new DiskStore();
		s.load();
		await settle();
		expect(s.error?.code).toBe('bad_request');
		expect(s.tempError?.code).toBe('bad_request');
	});

	it('reads the temperatures on a timer until stopped', async () => {
		const s = new DiskStore();
		const stop = s.refreshTemps();
		await vi.advanceTimersByTimeAsync(TEMPS_EVERY_MS * 2);
		expect(methods()).toEqual(['disk.temperatures', 'disk.temperatures']);
		stop();
		await vi.advanceTimersByTimeAsync(TEMPS_EVERY_MS * 2);
		expect(calls).toHaveLength(2);
	});

	it('reads again only when stale', async () => {
		const s = new DiskStore();
		s.loadIfStale();
		expect(calls).toHaveLength(0);
		s.load();
		await settle();
		calls.length = 0;
		s.loadIfStale();
		expect(calls).toHaveLength(0);
		s.loadIfStale(performance.now() + 61_000);
		expect(methods()).toEqual(['disk.query', 'disk.temperatures']);
	});

	it('reads again when the NAS link comes back', async () => {
		const s = new DiskStore();
		s.onStatus('connecting');
		s.onStatus('up');
		expect(calls).toHaveLength(0); // nothing loaded yet: the first load is under way
		s.load();
		await settle();
		calls.length = 0;
		s.onStatus('up');
		expect(calls).toHaveLength(0);
		s.onStatus('nas-reconnecting');
		s.onStatus('up');
		expect(methods()).toEqual(['disk.query', 'disk.temperatures']);
	});
});

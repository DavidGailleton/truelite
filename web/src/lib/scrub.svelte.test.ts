import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Scan } from './pools';
import { NasError, type CallOptions } from './ws';

interface Pending {
	method: string;
	params: unknown[];
	options: CallOptions;
	resolve(v: unknown): void;
	reject(e: unknown): void;
}
const pending: Pending[] = [];

vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({
		call: (method: string, params: unknown[], options: CallOptions = {}) =>
			new Promise((resolve, reject) => pending.push({ method, params, options, resolve, reject }))
	})
}));
const loadPools = vi.fn();
vi.mock('./pools.svelte', () => ({ loadPools }));

const { dismissScrub, finishScrub, runScrub, scrubRun } = await import('./scrub.svelte');
const { closeNas } = await import('./nas.svelte');

const running: Scan = {
	kind: 'scrub',
	state: 'running',
	percent: 1,
	end: null,
	paused: null,
	errors: 0,
	secondsLeft: null
};
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
	pending.length = 0;
	loadPools.mockClear();
});
afterEach(() => {
	closeNas();
	vi.useRealTimers();
});

describe('scrub runs', () => {
	it('show the job until the scan line shows the check, then ignore its late end', async () => {
		runScrub('tank', 'start');
		expect(pending[0]).toMatchObject({ method: 'pool.scrub.scrub', params: ['tank', 'START'] });
		expect(pending[0].options.timeout).toBeNull();
		pending[0].options.onProgress?.({ percent: 3, description: 'Scrubbing' });
		expect(scrubRun('tank')).toMatchObject({ action: 'start', progress: { percent: 3 } });
		finishScrub('tank', null);
		expect(scrubRun('tank')).toBeDefined();
		finishScrub('tank', running);
		expect(scrubRun('tank')).toBeUndefined();
		// Hours later the socket drops: the START job's call fails, but nobody waits for it.
		pending[0].reject(new NasError({ code: 'reconnecting', message: '' }));
		await settle();
		expect(scrubRun('tank')).toBeUndefined();
		expect(loadPools).not.toHaveBeenCalled();
	});

	it('read the pools again when a pause ends', async () => {
		runScrub('tank', 'pause');
		runScrub('tank', 'stop');
		expect(pending).toHaveLength(1);
		pending[0].resolve(null);
		await settle();
		expect(scrubRun('tank')).toBeUndefined();
		expect(loadPools).toHaveBeenCalledOnce();
	});

	it('keep a failure until dismissed, and allow a new try', async () => {
		runScrub('tank', 'start');
		pending[0].reject(new NasError({ code: 'nas', message: 'x', errname: 'EACCES' }));
		await settle();
		expect(scrubRun('tank')?.error).toContain('POOL_WRITE');
		finishScrub('tank', running);
		expect(scrubRun('tank')?.error).toBeTruthy();
		runScrub('tank', 'start');
		expect(pending).toHaveLength(2);
		expect(scrubRun('tank')?.error).toBeNull();
		pending[1].reject(new NasError({ code: 'nas', message: 'busy' }));
		await settle();
		dismissScrub('tank');
		expect(scrubRun('tank')).toBeUndefined();
	});

	it('step aside after a minute without news, and are forgotten on logout', () => {
		vi.useFakeTimers();
		runScrub('tank', 'start');
		runScrub('apps', 'start');
		expect(pending).toHaveLength(2);
		vi.advanceTimersByTime(60_000);
		expect(scrubRun('tank')).toBeUndefined();
		runScrub('tank', 'stop');
		closeNas();
		expect(scrubRun('tank')).toBeUndefined();
	});
});

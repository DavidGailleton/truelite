// Running data-check actions (#38), per pool name for the whole visit. One action per pool at a
// time. A run shows "Starting…" (and the job's progress) until the pool's scan line shows the
// change (`finishScrub`, called by the card), the job ends, or SETTLE_MAX_MS pass; then the scan
// line takes over. START's job lasts the whole scrub: once its run is gone, its reply (or a
// rejection when the socket drops hours later) is ignored.
import { SvelteMap } from 'svelte/reactivity';
import { nas, onNasClose } from './nas.svelte';
import { loadPools } from './pools.svelte';
import type { Scan } from './pools';
import { scrubCall, scrubError, scrubSettled, type ScrubAction } from './scrub';
import { failure, type JobProgress } from './ws';

/** A run is dropped after this even if the scan never showed the change. */
const SETTLE_MAX_MS = 60_000;

export interface ScrubRun {
	action: ScrubAction;
	/** `null` until the NAS reports the job. */
	progress: JobProgress | null;
	/** Set when the action failed. */
	error: string | null;
}

const runs = new SvelteMap<string, ScrubRun>();
onNasClose(() => runs.clear());

/** The running or failed action on pool `name`, if any. */
export function scrubRun(name: string): ScrubRun | undefined {
	return runs.get(name);
}

/** Drops the run on pool `name` once `scan` shows what it asked for. */
export function finishScrub(name: string, scan: Scan | null): void {
	const run = runs.get(name);
	if (run && run.error === null && scrubSettled(run.action, scan)) runs.delete(name);
}

/** Forgets a failed action on pool `name`. */
export function dismissScrub(name: string): void {
	if (runs.get(name)?.error != null) runs.delete(name);
}

/** Runs `action` on pool `name` and follows it until the scan line can take over. */
export function runScrub(name: string, action: ScrubAction): void {
	const existing = runs.get(name);
	if (existing && existing.error === null) return;
	let current: ScrubRun = { action, progress: null, error: null };
	runs.set(name, current);
	// Drops the update once the map holds another run (settled, dismissed, logged out, later).
	const update = (next: ScrubRun | null) => {
		if (runs.get(name) !== current) return false;
		if (next) {
			current = next;
			runs.set(name, next);
		} else runs.delete(name);
		return true;
	};
	setTimeout(() => {
		if (current.error === null) update(null);
	}, SETTLE_MAX_MS);
	const call = scrubCall(name, action);
	nas()
		.call(call.method, call.params, {
			timeout: null,
			onProgress: (progress) => update({ ...current, progress })
		})
		.then(
			() => {
				// A pause or stop may show as a pool change rather than a scan event.
				if (update(null)) loadPools();
			},
			(e: unknown) => update({ ...current, progress: null, error: scrubError(action, failure(e)) })
		);
}

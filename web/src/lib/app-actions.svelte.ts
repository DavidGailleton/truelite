// Running app actions (#33), kept per app for the whole visit, so a job's progress survives
// leaving the app's page and coming back. One action per app at a time. A job ends before the app
// has settled: its run stays `done` until the page sees the new state ({@link finishAction}).
import { SvelteMap } from 'svelte/reactivity';
import { ACTION_METHODS, actionError, type ActionRun, type AppAction } from './app-actions';
import { nas } from './nas.svelte';
import { NasError } from './ws';

/** A done run is dropped after this even if the app never reached its new state. */
const SETTLE_MAX_MS = 120_000;

const runs = new SvelteMap<string, ActionRun>();

/** The running or failed action of app `id`, if any. */
export function actionRun(id: string): ActionRun | undefined {
	return runs.get(id);
}

/** Whether app `id` has an action on its way. */
export function isRunning(id: string): boolean {
	const run = runs.get(id);
	return run !== undefined && run.error === null && !run.done;
}

/** Drops the done run of app `id`: the page saw the app's new state. */
export function finishAction(id: string): void {
	if (runs.get(id)?.done) runs.delete(id);
}

/** Forgets a failed action of app `id`. */
export function dismissAction(id: string): void {
	if (runs.get(id)?.error !== null) runs.delete(id);
}

/**
 * Runs `action` on app `id` and follows its job. A success leaves the run `done`: the app's new
 * state comes later through the `app.query` events, and the page then calls {@link finishAction}.
 */
export function runAction(id: string, action: AppAction): void {
	if (isRunning(id)) return;
	const mine: ActionRun = { action, progress: null, done: false, error: null };
	runs.set(id, mine);
	// This call's latest run; an update is dropped once the map holds another one (a later
	// action, or the run was dismissed or finished).
	let current: ActionRun = mine;
	const update = (next: ActionRun) => {
		if (runs.get(id) !== current) return false;
		current = next;
		runs.set(id, next);
		return true;
	};
	nas()
		.call(ACTION_METHODS[action], [id], {
			timeout: null,
			onProgress: (progress) => update({ ...current, progress })
		})
		.then(
			() => {
				const done = { ...current, done: true };
				if (!update(done)) return;
				setTimeout(() => {
					if (runs.get(id) === done) runs.delete(id);
				}, SETTLE_MAX_MS);
			},
			(e: unknown) => {
				const body =
					e instanceof NasError
						? { code: e.code, message: e.message, errname: e.errname }
						: { code: 'bad_request' as const, message: 'Something went wrong.' };
				update({ action, progress: null, done: false, error: actionError(action, body) });
			}
		);
}

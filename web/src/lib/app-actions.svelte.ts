// Running app actions (#33, #34), kept per app for the whole visit, so a job's progress survives
// leaving the app's page and coming back. One action per app at a time. A job ends before the app
// has settled: its run stays `done` until the page sees the new state ({@link finishAction}).
import { SvelteMap } from 'svelte/reactivity';
import { actionError, type ActionRun, type AppAction, type JobCall } from './app-actions';
import type { AppState } from './apps';
import { nas, onNasClose } from './nas.svelte';
import { failure } from './ws';

/** A done run is dropped after this even if the app never reached its new state. */
const SETTLE_MAX_MS = 120_000;
/**
 * The status line stays at least this long after the click. A restart's job ends when compose
 * exits, often in about a second, and the app is RUNNING before and after.
 */
const MIN_SHOWN_MS = 2_000;

const runs = new SvelteMap<string, ActionRun>();
// Another account may log in next in this tab: its pages never show these runs. Callbacks of
// pending calls find their run gone and drop their updates.
onNasClose(() => runs.clear());

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

/** The page saw app `id` in `state`: a run started in another state now knows the app moved. */
export function noteState(id: string, state: AppState): void {
	const run = runs.get(id);
	if (run && !run.left && state !== run.from) runs.set(id, { ...run, left: true });
}

/** Forgets a failed action of app `id`. */
export function dismissAction(id: string): void {
	if (runs.get(id)?.error !== null) runs.delete(id);
}

/**
 * Runs `action` on app `id` through the job `call` (`actionCall` in app-actions.ts) and follows
 * it. A success leaves the run `done`: the app's new state comes later through the `app.query`
 * events, and the page then calls {@link finishAction}.
 */
export function runAction(id: string, action: AppAction, call: JobCall, from: AppState): void {
	if (isRunning(id)) return;
	const started = Date.now();
	const mine: ActionRun = { action, progress: null, done: false, error: null, from, left: false };
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
		.call(call.method, call.params, {
			timeout: null,
			onProgress: (progress) => update({ ...current, progress })
		})
		.then(
			() => {
				setTimeout(
					() => {
						const done = { ...current, done: true };
						if (!update(done)) return;
						setTimeout(() => {
							if (runs.get(id) === done) runs.delete(id);
						}, SETTLE_MAX_MS);
					},
					Math.max(0, MIN_SHOWN_MS - (Date.now() - started))
				);
			},
			(e: unknown) => {
				const error = actionError(action, failure(e));
				update({ ...current, progress: null, done: false, error });
			}
		);
}

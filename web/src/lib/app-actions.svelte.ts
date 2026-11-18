// Running app actions (#33), kept per app for the whole visit, so a job's progress survives
// leaving the app's page and coming back. One action per app at a time.
import { SvelteMap } from 'svelte/reactivity';
import { ACTION_METHODS, actionError, type AppAction } from './app-actions';
import { nas } from './nas.svelte';
import { NasError, type JobProgress } from './ws';

export interface ActionRun {
	action: AppAction;
	/** `null` until the NAS reports the job. */
	progress: JobProgress | null;
	/** Set when the action failed; the run then stays until dismissed or replaced. */
	error: string | null;
}

const runs = new SvelteMap<string, ActionRun>();

/** The running or failed action of app `id`, if any. */
export function actionRun(id: string): ActionRun | undefined {
	return runs.get(id);
}

/** Whether app `id` has an action on its way. */
export function isRunning(id: string): boolean {
	const run = runs.get(id);
	return run !== undefined && run.error === null;
}

/** Forgets a failed action of app `id`. */
export function dismissAction(id: string): void {
	if (runs.get(id)?.error !== null) runs.delete(id);
}

/**
 * Runs `action` on app `id` and follows its job. The app's state then comes through the
 * `app.query` events, so a success simply ends the run.
 */
export function runAction(id: string, action: AppAction): void {
	if (isRunning(id)) return;
	runs.set(id, { action, progress: null, error: null });
	nas()
		.call(ACTION_METHODS[action], [id], {
			timeout: null,
			onProgress: (progress) => {
				const run = runs.get(id);
				if (run?.action === action && run.error === null) runs.set(id, { ...run, progress });
			}
		})
		.then(
			() => runs.delete(id),
			(e: unknown) => {
				const body =
					e instanceof NasError
						? { code: e.code, message: e.message, errname: e.errname }
						: { code: 'bad_request' as const, message: 'Something went wrong.' };
				runs.set(id, { action, progress: null, error: actionError(action, body) });
			}
		);
}

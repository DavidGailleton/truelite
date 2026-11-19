// Start, stop and restart an app (#33): TrueNAS jobs `app.start`, `app.stop`, `app.redeploy`
// (api.truenas.com v25.10; role APPS_WRITE); update it (#34, app-upgrade.ts). Which actions fit
// the app's state, and the words of their buttons and dialogs. Framework-free so it can be
// unit-tested: the Svelte binding lives in app-actions.svelte.ts.
import type { App, AppState, AppUpdate } from './apps';
import { updateCall } from './app-upgrade';
import { hasRole } from './system';
import { type ErrorBody, type JobProgress, BUSY_MESSAGE } from './ws';

export type AppAction = 'start' | 'stop' | 'restart' | 'update';

/** The jobs of the actions taking only the app name. */
export const ACTION_METHODS: Record<Exclude<AppAction, 'update'>, string> = {
	start: 'app.start',
	stop: 'app.stop',
	// TrueNAS's own Restart: recreates the app's containers.
	restart: 'app.redeploy'
};

/** A job method and its params, as the bridge allowlists them. */
export interface JobCall {
	method: string;
	params: unknown[];
}

/** The job call of `action` on `app`; `snapshot` asks an upgrade to snapshot host paths first. */
export function actionCall(
	action: AppAction,
	app: Pick<App, 'id' | 'update'>,
	snapshot = false
): JobCall {
	return action === 'update'
		? updateCall(app, snapshot)
		: { method: ACTION_METHODS[action], params: [app.id] };
}

/**
 * The actions that make sense in `state`, in button order. None while the app is changing.
 * Update needs a running (or crashed) app: TrueNAS refuses to upgrade a stopped one.
 */
export function appActions(state: AppState, update: AppUpdate | null = null): AppAction[] {
	switch (state) {
		case 'running':
			return update ? ['update', 'restart', 'stop'] : ['restart', 'stop'];
		case 'crashed':
			return update ? ['start', 'update', 'stop'] : ['start', 'stop'];
		case 'stopped':
			return ['start'];
		case 'deploying':
		case 'stopping':
			return [];
	}
}

export const ACTION_LABELS: Record<AppAction, string> = {
	start: 'Start',
	stop: 'Stop',
	restart: 'Restart',
	update: 'Update'
};

/** What the progress line says while the action runs and the NAS gives no description. */
export const ACTION_RUNNING: Record<AppAction, string> = {
	start: 'Starting…',
	stop: 'Stopping…',
	restart: 'Restarting…',
	update: 'Updating…'
};

/**
 * The states that end an action. The job ends as soon as the NAS has asked Docker; the app then
 * stays DEPLOYING or STOPPING for a while, so the page waits for one of these.
 */
const ACTION_DONE: Record<AppAction, readonly AppState[]> = {
	start: ['running', 'crashed'],
	stop: ['stopped', 'crashed'],
	restart: ['running', 'crashed'],
	update: ['running', 'crashed']
};

/** What the progress line says once the job is over but the app hasn't settled yet. */
const ACTION_SETTLING: Record<AppAction, string> = {
	start: 'Waiting for the app to start…',
	stop: 'Waiting for the app to stop…',
	restart: 'Waiting for the app to come back…',
	update: 'Waiting for the app to come back…'
};

/** An action as the page tracks it (app-actions.svelte.ts). */
export interface ActionRun {
	action: AppAction;
	/** `null` until the NAS reports the job. */
	progress: JobProgress | null;
	/** The job ended well; the app may still be on its way to its new state. */
	done: boolean;
	/** Set when the action failed. */
	error: string | null;
	/** The app's state when the action was confirmed. */
	from: AppState;
	/** The app has been seen in another state since (`noteState` in app-actions.svelte.ts). */
	left: boolean;
}

export interface Busy {
	label: string;
	progress: JobProgress | null;
}

/**
 * What the progress line shows for an app in `state` with `run` (if any): the job's progress,
 * then a wait until the app reaches its new state; also while the app is starting or stopping
 * for another reason (another tab, the NAS itself). `null` when nothing is going on.
 */
export function busyLine(run: ActionRun | undefined, state: AppState): Busy | null {
	if (run && run.error === null) {
		if (!run.done) return { label: ACTION_RUNNING[run.action], progress: run.progress };
		// A crashed app that is started or updated is crashed until the NAS reports it deploying:
		// still crashed is not the end of the action until it has left that state.
		const unmoved = state === 'crashed' && run.from === 'crashed' && !run.left;
		const settled = state !== 'deploying' && state !== 'stopping' && !unmoved;
		if (!settled || !ACTION_DONE[run.action].includes(state)) {
			return { label: ACTION_SETTLING[run.action], progress: null };
		}
	}
	if (state === 'deploying') return { label: 'Starting…', progress: null };
	if (state === 'stopping') return { label: 'Stopping…', progress: null };
	return null;
}

export interface ActionDialog {
	title: string;
	text: string;
	confirm: string;
	/** Stopping cuts the app off: a destructive button. */
	destructive: boolean;
}

/** The dialog of `action` on app `name`; `update` says what an update brings. */
export function actionDialog(
	action: AppAction,
	name: string,
	update: AppUpdate | null = null
): ActionDialog {
	switch (action) {
		case 'start':
			return {
				title: `Start ${name}?`,
				text: 'Its containers start; this can take a minute.',
				confirm: 'Start',
				destructive: false
			};
		case 'stop':
			return {
				title: `Stop ${name}?`,
				text: 'Its web page and services stop working until you start it again.',
				confirm: 'Stop',
				destructive: true
			};
		case 'restart':
			return {
				title: `Restart ${name}?`,
				text: 'Its containers are recreated. It is unavailable for a moment.',
				confirm: 'Restart',
				destructive: false
			};
		case 'update':
			return {
				title: `Update ${name}?`,
				text:
					update === 'images'
						? 'Newer images of the same version are downloaded, then the app restarts. It is unavailable for a moment.'
						: 'It stops, downloads the new version and starts again. This can take a few minutes.',
				confirm: 'Update',
				destructive: false
			};
	}
}

/** Roles that may start, stop and update apps. */
export function canChangeApps(roles: readonly string[] | undefined): boolean {
	return hasRole(roles, 'APPS_WRITE');
}

/** The failure of an action in plain words. */
export function actionError(action: AppAction, error: ErrorBody): string {
	const verb = ACTION_LABELS[action].toLowerCase();
	if (error.errname === 'EACCES') {
		return "Your account can't change apps. It needs the Apps write role (APPS_WRITE) on the NAS.";
	}
	switch (error.code) {
		case 'disconnected':
		case 'reconnecting':
			return `The connection dropped while the app was asked to ${verb}. It may still be working on it: watch its state.`;
		case 'busy':
			return BUSY_MESSAGE;
		default:
			return `Couldn't ${verb} the app: ${error.message}`;
	}
}

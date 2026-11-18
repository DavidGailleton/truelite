// Start, stop and restart an app (#33): TrueNAS jobs `app.start`, `app.stop`, `app.redeploy`
// (api.truenas.com v25.10; role APPS_WRITE). Which actions fit the app's state, and the words of
// their buttons and dialogs. Framework-free so it can be unit-tested: the Svelte binding lives in
// app-actions.svelte.ts.
import type { AppState } from './apps';
import type { ErrorBody } from './ws';

export type AppAction = 'start' | 'stop' | 'restart';

export const ACTION_METHODS: Record<AppAction, string> = {
	start: 'app.start',
	stop: 'app.stop',
	// TrueNAS's own Restart: recreates the app's containers.
	restart: 'app.redeploy'
};

/** The actions that make sense in `state`, in button order. None while the app is changing. */
export function appActions(state: AppState): AppAction[] {
	switch (state) {
		case 'running':
			return ['restart', 'stop'];
		case 'crashed':
			return ['start', 'stop'];
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
	restart: 'Restart'
};

/** What the progress line says while the action runs and the NAS gives no description. */
export const ACTION_RUNNING: Record<AppAction, string> = {
	start: 'Starting…',
	stop: 'Stopping…',
	restart: 'Restarting…'
};

export interface ActionDialog {
	title: string;
	text: string;
	confirm: string;
	/** Stopping cuts the app off: a destructive button. */
	destructive: boolean;
}

export function actionDialog(action: AppAction, name: string): ActionDialog {
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
	}
}

/** Roles that may start and stop apps (TrueNAS expands FULL_ADMIN, but don't rely on it). */
export function canChangeApps(roles: readonly string[] | undefined): boolean {
	return roles !== undefined && (roles.includes('APPS_WRITE') || roles.includes('FULL_ADMIN'));
}

/** The failure of an action in plain words. */
export function actionError(action: AppAction, error: ErrorBody): string {
	const verb = ACTION_LABELS[action].toLowerCase();
	if (error.errname === 'EACCES') {
		return "Your account can't start or stop apps. It needs the Apps write role (APPS_WRITE) on the NAS.";
	}
	switch (error.code) {
		case 'disconnected':
		case 'reconnecting':
			return `The connection dropped while the app was asked to ${verb}. It may still be working on it: watch its state.`;
		case 'busy':
			return 'Too many things at once on this page. Try again in a moment.';
		default:
			return `Couldn't ${verb} the app: ${error.message}`;
	}
}

// Apps summary from `app.query` (api.truenas.com v25.10; role APPS_READ). Framework-free so it
// can be unit-tested: the Svelte binding lives in apps.svelte.ts.
import { isObj, isStr } from './system';
import type { EventData } from './ws';

export const APP_QUERY = 'app.query';

/** Fields asked of `app.query`; the server forwards the same ones in events (`truenas/apps.rs`). */
export const APP_SELECT = [
	'id',
	'name',
	'state',
	'upgrade_available',
	'image_updates_available',
	'custom_app'
] as const;

export type AppState = 'running' | 'deploying' | 'stopping' | 'stopped' | 'crashed';

const STATES: Record<string, AppState> = {
	RUNNING: 'running',
	DEPLOYING: 'deploying',
	STOPPING: 'stopping',
	STOPPED: 'stopped',
	CRASHED: 'crashed'
};

export interface App {
	/** The app name; TrueNAS uses it as the id. */
	id: string;
	name: string;
	state: AppState;
	/** A new catalog version or newer container images. */
	updateAvailable: boolean;
	/** Installed from a compose file, not the catalog. */
	custom: boolean;
}

/** One `app.query` entry, or `null` when it lacks the fields the card needs. */
export function parseApp(raw: unknown): App | null {
	if (!isObj(raw)) return null;
	const { id, name } = raw;
	const state = isStr(raw.state) ? STATES[raw.state.toUpperCase()] : undefined;
	if (!isStr(id) || id === '' || !state) return null;
	return {
		id,
		name: isStr(name) && name !== '' ? name : id,
		state,
		updateAvailable: raw.upgrade_available === true || raw.image_updates_available === true,
		custom: raw.custom_app === true
	};
}

/** The `app.query` result; unreadable entries are dropped. `null` when it isn't a list. */
export function parseApps(raw: unknown): App[] | null {
	if (!Array.isArray(raw)) return null;
	return raw.map(parseApp).filter((a): a is App => a !== null);
}

/** The app list after an `app.query` event, or `null` when it should be queried again. */
export function applyAppEvent(apps: readonly App[], event: EventData): App[] | null {
	if (!isStr(event.id)) return null;
	const id = event.id;
	if (event.kind === 'removed') return apps.filter((a) => a.id !== id);
	if (event.kind !== 'added' && event.kind !== 'changed') return null;
	const app = parseApp(event.fields);
	if (!app || app.id !== id) return null;
	const at = apps.findIndex((a) => a.id === id);
	if (at < 0) return [...apps, app];
	return apps.map((a, i) => (i === at ? app : a));
}

export interface AppSummary {
	running: number;
	stopped: number;
	/** Deploying or stopping: on their way to running or stopped. */
	starting: number;
	/** Apps with an update available. */
	updates: number;
	/** Crashed apps, by name. */
	crashed: App[];
}

/** Counts for the dashboard card. */
export function summarize(apps: readonly App[]): AppSummary {
	const s: AppSummary = { running: 0, stopped: 0, starting: 0, updates: 0, crashed: [] };
	for (const app of apps) {
		if (app.updateAvailable) s.updates++;
		if (app.state === 'running') s.running++;
		else if (app.state === 'stopped') s.stopped++;
		else if (app.state === 'crashed') s.crashed.push(app);
		else s.starting++;
	}
	s.crashed.sort((a, b) => a.name.localeCompare(b.name));
	return s;
}

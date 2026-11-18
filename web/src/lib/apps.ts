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
	'custom_app',
	'version',
	'human_version'
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
	/** The version of the app itself (`10.11.2`), not the catalog revision; `null` if unknown. */
	version: string | null;
}

/**
 * The app version from `human_version` (`<app version>_<catalog revision>`, e.g.
 * `10.11.2_1.3.1`) and `version` (the revision): the revision suffix is dropped. A
 * `human_version` without that suffix (custom apps) is shown as it is.
 */
export function appVersion(human: unknown, version: unknown): string | null {
	const h = isStr(human) ? human.trim() : '';
	const v = isStr(version) ? version.trim() : '';
	if (h !== '') {
		const suffix = `_${v}`;
		if (v !== '' && h.endsWith(suffix) && h.length > suffix.length) {
			return h.slice(0, -suffix.length);
		}
		return h;
	}
	return v !== '' ? v : null;
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
		custom: raw.custom_app === true,
		version: appVersion(raw.human_version, raw.version)
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

/** The filters of the apps page. `not-running` holds every app that isn't running. */
export type AppFilter = 'all' | 'running' | 'not-running' | 'updates';

export const APP_FILTERS: readonly { value: AppFilter; label: string }[] = [
	{ value: 'all', label: 'All' },
	{ value: 'running', label: 'Running' },
	{ value: 'not-running', label: 'Not running' },
	{ value: 'updates', label: 'Updates' }
];

/** The `?show=` value of the apps page; anything unknown shows all apps. */
export function parseFilter(param: string | null): AppFilter {
	return APP_FILTERS.some((f) => f.value === param) ? (param as AppFilter) : 'all';
}

function matches(app: App, filter: AppFilter): boolean {
	switch (filter) {
		case 'all':
			return true;
		case 'running':
			return app.state === 'running';
		case 'not-running':
			return app.state !== 'running';
		case 'updates':
			return app.updateAvailable;
	}
}

/** The apps in `filter` whose name contains `query` (any case; blank matches all). */
export function filterApps(apps: readonly App[], filter: AppFilter, query: string): App[] {
	const q = query.trim().toLowerCase();
	return apps.filter((a) => matches(a, filter) && (q === '' || a.name.toLowerCase().includes(q)));
}

/** How many apps each filter shows (search not applied). */
export function filterCounts(apps: readonly App[]): Record<AppFilter, number> {
	const counts: Record<AppFilter, number> = { all: 0, running: 0, 'not-running': 0, updates: 0 };
	for (const app of apps) {
		for (const { value } of APP_FILTERS) if (matches(app, value)) counts[value]++;
	}
	return counts;
}

const RANK: Record<AppState, number> = {
	crashed: 0,
	deploying: 1,
	stopping: 1,
	running: 2,
	stopped: 2
};

/** Attention first: crashed, then starting or stopping, then the rest; by name in each group. */
export function sortApps(apps: readonly App[]): App[] {
	return [...apps].sort((a, b) => RANK[a.state] - RANK[b.state] || a.name.localeCompare(b.name));
}

export type Tone = 'success' | 'warning' | 'destructive' | 'neutral';

/** How a state reads: words first, the colour only repeats them. */
export function stateLabel(state: AppState): { label: string; tone: Tone; dot: boolean } {
	switch (state) {
		case 'running':
			return { label: 'Running', tone: 'success', dot: true };
		case 'stopped':
			return { label: 'Stopped', tone: 'neutral', dot: false };
		case 'deploying':
			return { label: 'Starting…', tone: 'neutral', dot: true };
		case 'stopping':
			return { label: 'Stopping…', tone: 'neutral', dot: true };
		case 'crashed':
			return { label: 'Crashed', tone: 'destructive', dot: false };
	}
}

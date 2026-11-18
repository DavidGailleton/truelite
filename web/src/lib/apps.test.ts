import { describe, expect, it } from 'vitest';
import {
	appVersion,
	applyAppEvent,
	filterApps,
	filterCounts,
	parseApp,
	parseApps,
	parseFilter,
	sortApps,
	stateLabel,
	summarize,
	type App
} from './apps';

const raw = (over: Record<string, unknown> = {}) => ({
	id: 'jellyfin',
	name: 'jellyfin',
	state: 'RUNNING',
	upgrade_available: false,
	image_updates_available: false,
	custom_app: false,
	version: '1.3.1',
	human_version: '10.11.2_1.3.1',
	...over
});

const app = (id: string, state: App['state'], updateAvailable = false): App => ({
	id,
	name: id,
	state,
	update: updateAvailable ? 'version' : null,
	custom: false,
	version: null
});

describe('parseApp', () => {
	it('reads an entry', () => {
		expect(parseApp(raw({ custom_app: true }))).toEqual({
			id: 'jellyfin',
			name: 'jellyfin',
			state: 'running',
			update: null,
			custom: true,
			version: '10.11.2'
		});
	});

	it('flags an update from either field', () => {
		expect(parseApp(raw({ upgrade_available: true }))?.update).toBe('version');
		expect(parseApp(raw({ image_updates_available: true }))?.update).toBe('images');
		expect(parseApp(raw({ upgrade_available: true, image_updates_available: true }))?.update).toBe(
			'version'
		);
	});

	it('reads every state', () => {
		for (const [s, want] of [
			['RUNNING', 'running'],
			['DEPLOYING', 'deploying'],
			['STOPPING', 'stopping'],
			['STOPPED', 'stopped'],
			['CRASHED', 'crashed']
		]) {
			expect(parseApp(raw({ state: s }))?.state).toBe(want);
		}
	});

	it('drops what it cannot read', () => {
		expect(parseApp(raw({ state: 'PAUSED' }))).toBeNull();
		expect(parseApp(raw({ id: 3 }))).toBeNull();
		expect(parseApp(raw({ id: '' }))).toBeNull();
		expect(parseApp(null)).toBeNull();
	});

	it('falls back to the id for a missing name', () => {
		expect(parseApp(raw({ name: undefined }))?.name).toBe('jellyfin');
	});
});

describe('parseApps', () => {
	it('keeps readable entries', () => {
		expect(parseApps([raw(), { id: 'x' }])?.map((a) => a.id)).toEqual(['jellyfin']);
		expect(parseApps({})).toBeNull();
	});
});

describe('applyAppEvent', () => {
	const list = [app('a', 'running'), app('b', 'stopped')];

	it('adds, changes and removes', () => {
		const added = applyAppEvent(list, { kind: 'added', id: 'c', fields: raw({ id: 'c' }) });
		expect(added?.map((a) => a.id)).toEqual(['a', 'b', 'c']);
		const changed = applyAppEvent(list, {
			kind: 'changed',
			id: 'b',
			fields: raw({ id: 'b', name: 'b', state: 'CRASHED' })
		});
		expect(changed?.[1].state).toBe('crashed');
		expect(applyAppEvent(list, { kind: 'removed', id: 'a' })?.map((a) => a.id)).toEqual(['b']);
	});

	it('asks to query again on anything else', () => {
		expect(applyAppEvent(list, { kind: 'changed', id: 1, fields: raw() })).toBeNull();
		expect(applyAppEvent(list, { kind: 'changed', id: 'a', fields: raw() })).toBeNull();
		expect(applyAppEvent(list, { kind: 'changed', id: 'a', fields: { id: 'a' } })).toBeNull();
		expect(applyAppEvent(list, { kind: 'other', id: 'a' })).toBeNull();
	});
});

describe('summarize', () => {
	it('counts states and updates, crashed apps by name', () => {
		const s = summarize([
			app('z', 'crashed'),
			app('a', 'running', true),
			app('b', 'running'),
			app('c', 'stopped', true),
			app('d', 'deploying'),
			app('e', 'stopping'),
			app('m', 'crashed')
		]);
		expect(s).toMatchObject({ running: 2, stopped: 1, starting: 2, updates: 2 });
		expect(s.crashed.map((a) => a.name)).toEqual(['m', 'z']);
	});

	it('is all zeros for no apps', () => {
		expect(summarize([])).toEqual({ running: 0, stopped: 0, starting: 0, updates: 0, crashed: [] });
	});
});

describe('appVersion', () => {
	it('drops the catalog revision', () => {
		expect(appVersion('10.11.2_1.3.1', '1.3.1')).toBe('10.11.2');
	});

	it('keeps a version without that suffix', () => {
		expect(appVersion('1.0.0', '1.0.0')).toBe('1.0.0');
		expect(appVersion('10.11.2_1.3.1', '1.3.2')).toBe('10.11.2_1.3.1');
		expect(appVersion('2.0_beta', '')).toBe('2.0_beta');
		expect(appVersion('_1.3.1', '1.3.1')).toBe('_1.3.1');
	});

	it('falls back to the revision, then nothing', () => {
		expect(appVersion(undefined, '1.3.1')).toBe('1.3.1');
		expect(appVersion('', ' ')).toBeNull();
		expect(appVersion(null, 3)).toBeNull();
	});
});

describe('filters', () => {
	const list = [
		app('Jellyfin', 'running', true),
		app('immich', 'crashed'),
		app('plex', 'stopped', true),
		app('nextcloud', 'deploying'),
		app('jelly-two', 'running')
	];
	const ids = (l: App[]) => l.map((a) => a.id);

	it('filters by state and update', () => {
		expect(ids(filterApps(list, 'all', ''))).toEqual(ids(list));
		expect(ids(filterApps(list, 'running', ''))).toEqual(['Jellyfin', 'jelly-two']);
		expect(ids(filterApps(list, 'not-running', ''))).toEqual(['immich', 'plex', 'nextcloud']);
		expect(ids(filterApps(list, 'updates', ''))).toEqual(['Jellyfin', 'plex']);
	});

	it('searches names in any case, within the filter', () => {
		expect(ids(filterApps(list, 'all', '  JELLY '))).toEqual(['Jellyfin', 'jelly-two']);
		expect(ids(filterApps(list, 'updates', 'jelly'))).toEqual(['Jellyfin']);
		expect(filterApps(list, 'running', 'plex')).toEqual([]);
	});

	it('counts each filter', () => {
		expect(filterCounts(list)).toEqual({ all: 5, running: 2, 'not-running': 3, updates: 2 });
		expect(filterCounts([])).toEqual({ all: 0, running: 0, 'not-running': 0, updates: 0 });
	});

	it('reads the URL value', () => {
		expect(parseFilter('updates')).toBe('updates');
		expect(parseFilter('not-running')).toBe('not-running');
		expect(parseFilter('nope')).toBe('all');
		expect(parseFilter(null)).toBe('all');
	});
});

describe('sortApps', () => {
	it('puts crashed, then starting or stopping, first; by name inside', () => {
		const sorted = sortApps([
			app('b', 'running'),
			app('z', 'crashed'),
			app('a', 'stopped'),
			app('s', 'stopping'),
			app('d', 'deploying'),
			app('c', 'crashed')
		]);
		expect(sorted.map((a) => a.id)).toEqual(['c', 'z', 'd', 's', 'a', 'b']);
	});

	it('leaves the input alone', () => {
		const list = [app('b', 'running'), app('a', 'running')];
		sortApps(list);
		expect(list.map((a) => a.id)).toEqual(['b', 'a']);
	});
});

describe('stateLabel', () => {
	it('names every state in words', () => {
		expect(stateLabel('running')).toEqual({ label: 'Running', tone: 'success', dot: true });
		expect(stateLabel('crashed').tone).toBe('destructive');
		expect(stateLabel('deploying').label).toBe('Starting…');
		expect(stateLabel('stopping').label).toBe('Stopping…');
		expect(stateLabel('stopped').tone).toBe('neutral');
	});
});

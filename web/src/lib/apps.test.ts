import { describe, expect, it } from 'vitest';
import { applyAppEvent, parseApp, parseApps, summarize, type App } from './apps';

const raw = (over: Record<string, unknown> = {}) => ({
	id: 'jellyfin',
	name: 'jellyfin',
	state: 'RUNNING',
	upgrade_available: false,
	image_updates_available: false,
	custom_app: false,
	...over
});

const app = (id: string, state: App['state'], updateAvailable = false): App => ({
	id,
	name: id,
	state,
	updateAvailable,
	custom: false
});

describe('parseApp', () => {
	it('reads an entry', () => {
		expect(parseApp(raw({ custom_app: true }))).toEqual({
			id: 'jellyfin',
			name: 'jellyfin',
			state: 'running',
			updateAvailable: false,
			custom: true
		});
	});

	it('flags an update from either field', () => {
		expect(parseApp(raw({ upgrade_available: true }))?.updateAvailable).toBe(true);
		expect(parseApp(raw({ image_updates_available: true }))?.updateAvailable).toBe(true);
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

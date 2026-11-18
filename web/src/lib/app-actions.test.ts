import { describe, expect, it } from 'vitest';
import {
	ACTION_METHODS,
	actionCall,
	actionDialog,
	actionError,
	appActions,
	busyLine,
	type ActionRun,
	canChangeApps
} from './app-actions';

describe('appActions', () => {
	it('fit the state, and none while the app is changing', () => {
		expect(appActions('running')).toEqual(['restart', 'stop']);
		expect(appActions('stopped')).toEqual(['start']);
		expect(appActions('crashed')).toEqual(['start', 'stop']);
		expect(appActions('deploying')).toEqual([]);
		expect(appActions('stopping')).toEqual([]);
	});

	it('offer Update first when one exists, never on a stopped or changing app', () => {
		expect(appActions('running', 'version')).toEqual(['update', 'restart', 'stop']);
		expect(appActions('crashed', 'images')).toEqual(['start', 'update', 'stop']);
		expect(appActions('stopped', 'version')).toEqual(['start']);
		expect(appActions('deploying', 'version')).toEqual([]);
	});

	it('build the job call of each action', () => {
		const app = { id: 'plex', update: 'version' as const };
		expect(actionCall('stop', app)).toEqual({ method: 'app.stop', params: ['plex'] });
		expect(actionCall('restart', app)).toEqual({ method: 'app.redeploy', params: ['plex'] });
		expect(actionCall('update', app, true)).toEqual({
			method: 'app.upgrade',
			params: ['plex', { snapshot_hostpaths: true }]
		});
		expect(actionCall('update', { id: 'plex', update: 'images' })).toEqual({
			method: 'app.pull_images',
			params: ['plex']
		});
	});

	it('map to the allowlisted job methods', () => {
		expect(ACTION_METHODS).toEqual({
			start: 'app.start',
			stop: 'app.stop',
			restart: 'app.redeploy'
		});
	});
});

describe('actionDialog', () => {
	it('names the app and makes only stop destructive', () => {
		expect(actionDialog('stop', 'plex')).toMatchObject({ title: 'Stop plex?', destructive: true });
		expect(actionDialog('start', 'plex').destructive).toBe(false);
		expect(actionDialog('restart', 'plex').confirm).toBe('Restart');
		expect(actionDialog('update', 'plex', 'version')).toMatchObject({
			title: 'Update plex?',
			destructive: false
		});
		expect(actionDialog('update', 'plex', 'images').text).toContain('same version');
	});
});

describe('canChangeApps', () => {
	it('needs APPS_WRITE or FULL_ADMIN', () => {
		expect(canChangeApps(['READONLY_ADMIN', 'APPS_READ'])).toBe(false);
		expect(canChangeApps(['APPS_READ', 'APPS_WRITE'])).toBe(true);
		expect(canChangeApps(['FULL_ADMIN'])).toBe(true);
		expect(canChangeApps(undefined)).toBe(false);
	});
});

describe('actionError', () => {
	it('explains a missing role, a dropped link and other failures', () => {
		expect(actionError('stop', { code: 'nas', message: 'x', errname: 'EACCES' })).toContain(
			'APPS_WRITE'
		);
		expect(actionError('restart', { code: 'disconnected', message: 'x' })).toContain(
			'may still be working'
		);
		expect(actionError('start', { code: 'nas', message: 'Image pull failed' })).toBe(
			"Couldn't start the app: Image pull failed"
		);
	});
});

describe('busyLine', () => {
	const run = (patch: Partial<ActionRun>): ActionRun => ({
		action: 'start',
		progress: null,
		done: false,
		error: null,
		from: 'stopped',
		left: false,
		...patch
	});
	const progress = { percent: 40, description: 'Pulling' };

	it("shows the job's progress while it runs", () => {
		expect(busyLine(run({ progress }), 'stopped')).toEqual({ label: 'Starting…', progress });
	});

	it('waits after the job until the app reaches its new state', () => {
		const done = run({ done: true, progress });
		// The state event may come after the job's reply: still the old state.
		expect(busyLine(done, 'stopped')).toEqual({
			label: 'Waiting for the app to start…',
			progress: null
		});
		expect(busyLine(done, 'deploying')?.label).toBe('Waiting for the app to start…');
		expect(busyLine(done, 'running')).toBeNull();
		expect(busyLine(done, 'crashed')).toBeNull();
		expect(busyLine(run({ action: 'stop', done: true }), 'stopping')?.label).toBe(
			'Waiting for the app to stop…'
		);
		expect(busyLine(run({ action: 'stop', done: true }), 'stopped')).toBeNull();
	});

	it('keeps an update on its progress while the NAS stops the app, then waits for it', () => {
		const update = run({ action: 'update', progress });
		expect(busyLine(update, 'stopping')).toEqual({ label: 'Updating…', progress });
		expect(busyLine(update, 'stopped')).toEqual({ label: 'Updating…', progress });
		const done = run({ action: 'update', done: true });
		expect(busyLine(done, 'stopped')?.label).toBe('Waiting for the app to come back…');
		expect(busyLine(done, 'running')).toBeNull();
	});

	it('does not end an action on a crashed app before the app has left that state', () => {
		for (const action of ['start', 'update', 'restart'] as const) {
			const done = run({ action, done: true, from: 'crashed' });
			expect(busyLine(done, 'crashed')?.label).toMatch(/^Waiting/);
			expect(busyLine(done, 'running')).toBeNull();
			// It deployed, then crashed again: that is the end.
			expect(busyLine({ ...done, left: true }, 'crashed')).toBeNull();
		}
		expect(busyLine(run({ action: 'stop', done: true, from: 'crashed' }), 'stopped')).toBeNull();
	});

	it('follows a start or stop it did not ask for, and stays quiet otherwise', () => {
		expect(busyLine(undefined, 'deploying')).toEqual({ label: 'Starting…', progress: null });
		expect(busyLine(undefined, 'stopping')?.label).toBe('Stopping…');
		expect(busyLine(undefined, 'running')).toBeNull();
		expect(busyLine(run({ error: 'Nope' }), 'stopped')).toBeNull();
	});
});

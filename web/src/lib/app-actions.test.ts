import { describe, expect, it } from 'vitest';
import {
	ACTION_METHODS,
	actionDialog,
	actionError,
	appActions,
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

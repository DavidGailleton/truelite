import { describe, expect, it } from 'vitest';
import type { Scan } from './pools';
import {
	canScrub,
	scrubActions,
	scrubCall,
	scrubDialog,
	scrubError,
	scrubSettled,
	type ScrubAction
} from './scrub';

const scan = (over: Partial<Scan> = {}): Scan => ({
	kind: 'scrub',
	state: 'running',
	percent: 40,
	end: null,
	paused: null,
	errors: 0,
	secondsLeft: 600,
	...over
});
const finished = scan({ state: 'finished', end: new Date(0), percent: 100 });
const paused = scan({ paused: new Date(0) });

describe('scrubActions', () => {
	it('fits the buttons to the scan', () => {
		expect(scrubActions('ONLINE', null)).toEqual(['start']);
		expect(scrubActions('ONLINE', finished)).toEqual(['start']);
		expect(scrubActions('DEGRADED', scan({ state: 'canceled' }))).toEqual(['start']);
		expect(scrubActions('ONLINE', scan())).toEqual(['pause', 'stop']);
		expect(scrubActions('ONLINE', paused)).toEqual(['resume', 'stop']);
	});

	it('offers nothing while rebuilding or offline', () => {
		expect(scrubActions('DEGRADED', scan({ kind: 'resilver' }))).toEqual([]);
		expect(scrubActions('ONLINE', scan({ kind: 'resilver', state: 'finished' }))).toEqual([
			'start'
		]);
		for (const status of ['FAULTED', 'UNAVAIL', 'OFFLINE', 'REMOVED', 'SUSPENDED']) {
			expect(scrubActions(status, finished)).toEqual([]);
		}
	});
});

describe('scrubCall', () => {
	it('maps each action to the job argument', () => {
		const args = (['start', 'resume', 'pause', 'stop'] as ScrubAction[]).map(
			(a) => scrubCall('tank', a).params
		);
		expect(args).toEqual([
			['tank', 'START'],
			['tank', 'START'],
			['tank', 'PAUSE'],
			['tank', 'STOP']
		]);
		expect(scrubCall('tank', 'start').method).toBe('pool.scrub.scrub');
	});
});

describe('scrubSettled', () => {
	it('waits for the scan to show the change', () => {
		expect(scrubSettled('start', finished)).toBe(false);
		expect(scrubSettled('start', scan())).toBe(true);
		expect(scrubSettled('start', paused)).toBe(false);
		expect(scrubSettled('resume', scan())).toBe(true);
		expect(scrubSettled('pause', scan())).toBe(false);
		expect(scrubSettled('pause', paused)).toBe(true);
		expect(scrubSettled('stop', scan())).toBe(false);
		expect(scrubSettled('stop', scan({ state: 'canceled' }))).toBe(true);
		expect(scrubSettled('stop', null)).toBe(true);
		expect(scrubSettled('start', scan({ kind: 'resilver' }))).toBe(false);
	});
});

describe('dialogs and errors', () => {
	it('makes only stop destructive', () => {
		expect(scrubDialog('start', 'tank')).toMatchObject({
			title: 'Check tank for errors?',
			confirm: 'Start check',
			destructive: false
		});
		expect(scrubDialog('pause', 'tank').destructive).toBe(false);
		expect(scrubDialog('resume', 'tank').destructive).toBe(false);
		expect(scrubDialog('stop', 'tank')).toMatchObject({ destructive: true, confirm: 'Stop check' });
	});

	it('needs POOL_WRITE', () => {
		expect(canScrub(['POOL_WRITE'])).toBe(true);
		expect(canScrub(['FULL_ADMIN'])).toBe(true);
		expect(canScrub(['POOL_READ', 'POOL_SCRUB_WRITE'])).toBe(false);
		expect(canScrub(undefined)).toBe(false);
	});

	it('says what went wrong in plain words', () => {
		expect(scrubError('start', { code: 'nas', message: 'x', errname: 'EACCES' })).toContain(
			'POOL_WRITE'
		);
		expect(scrubError('pause', { code: 'reconnecting', message: '' })).toContain('pause the check');
		expect(scrubError('stop', { code: 'nas', message: 'No scrub running' })).toBe(
			"Couldn't stop the check: No scrub running"
		);
	});
});

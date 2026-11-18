import { describe, expect, it } from 'vitest';
import { badgeLabel, cardView, isDenied } from './card-state';
import type { ErrorBody } from './ws';

const denied: ErrorBody = { code: 'nas', message: 'Not authorized', errname: 'EACCES' };
const busy: ErrorBody = { code: 'busy', message: 'Too many requests.' };
const nasOther: ErrorBody = { code: 'nas', message: 'Boom', errname: 'EINVAL' };

describe('isDenied', () => {
	it('is a NAS EACCES only', () => {
		expect(isDenied(denied)).toBe(true);
		expect(isDenied(nasOther)).toBe(false);
		expect(isDenied({ code: 'not_allowed', message: 'x', errname: 'EACCES' })).toBe(false);
		expect(isDenied(null)).toBe(false);
	});
});

describe('cardView', () => {
	it('loads until the first answer, whatever the connection', () => {
		for (const status of ['connecting', 'up', 'nas-reconnecting', 'offline'] as const) {
			expect(cardView({ hasData: false, error: null, status })).toEqual({ kind: 'loading' });
		}
	});

	it('shows the error when nothing arrived', () => {
		expect(cardView({ hasData: false, error: busy, status: 'up' })).toEqual({
			kind: 'error',
			error: busy
		});
	});

	it('says denied, with or without data', () => {
		expect(cardView({ hasData: false, error: denied, status: 'up' })).toEqual({ kind: 'denied' });
		expect(cardView({ hasData: true, error: denied, status: 'up' })).toEqual({ kind: 'denied' });
	});

	it('keeps data after a later error, marked stale', () => {
		expect(cardView({ hasData: true, error: nasOther, status: 'up' })).toEqual({
			kind: 'ready',
			stale: nasOther,
			paused: false
		});
	});

	it('shows data with paused when the link is down', () => {
		expect(cardView({ hasData: true, error: null, status: 'nas-reconnecting' })).toEqual({
			kind: 'ready',
			stale: null,
			paused: true
		});
		expect(cardView({ hasData: true, error: null, status: 'up' })).toEqual({
			kind: 'ready',
			stale: null,
			paused: false
		});
	});

	it('is empty only with data', () => {
		expect(cardView({ hasData: true, isEmpty: true, error: null, status: 'up' })).toEqual({
			kind: 'empty',
			stale: null,
			paused: false
		});
		expect(cardView({ hasData: false, isEmpty: true, error: null, status: 'up' })).toEqual({
			kind: 'loading'
		});
		expect(cardView({ hasData: true, isEmpty: true, error: busy, status: 'up' })).toEqual({
			kind: 'empty',
			stale: busy,
			paused: false
		});
	});
});

describe('badgeLabel', () => {
	it('names why the numbers stand still', () => {
		expect(badgeLabel({ kind: 'ready', stale: busy, paused: true })).toBe('Not live');
		expect(badgeLabel({ kind: 'ready', stale: null, paused: true })).toBe('Paused');
		expect(badgeLabel({ kind: 'empty', stale: null, paused: true })).toBe('Paused');
		expect(badgeLabel({ kind: 'empty', stale: busy, paused: false })).toBe('Not live');
		expect(badgeLabel({ kind: 'ready', stale: null, paused: false })).toBeNull();
		expect(badgeLabel({ kind: 'loading' })).toBeNull();
		expect(badgeLabel({ kind: 'error', error: busy })).toBeNull();
		expect(badgeLabel({ kind: 'denied' })).toBeNull();
	});
});

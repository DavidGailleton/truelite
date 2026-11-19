import { describe, expect, it } from 'vitest';
import { httpsUrl, parseUpdateStatus } from './update';

const status = (newVersion: unknown = null, code = 'NORMAL') => ({
	code,
	status: {
		current_version: { train: 'TrueNAS-SCALE-Goldeye', profile: 'GENERAL', matches_profile: true },
		new_version: newVersion
	},
	error: null,
	update_download_progress: null
});

const available = (over: Record<string, unknown> = {}) =>
	status({
		version: '25.10.9',
		release_notes: '<p>Fixes for <b>SMB</b> &amp; apps.</p>',
		release_notes_url: 'https://www.truenas.com/docs/scale/25.10/',
		...over
	});

describe('parseUpdateStatus', () => {
	it('reads an up-to-date NAS', () => {
		expect(parseUpdateStatus(status())).toEqual({
			kind: 'current',
			train: 'TrueNAS-SCALE-Goldeye',
			newVersion: null,
			notes: null,
			notesUrl: null,
			error: null
		});
	});

	it('reads an available update with its notes as plain text', () => {
		expect(parseUpdateStatus(available())).toEqual({
			kind: 'available',
			train: 'TrueNAS-SCALE-Goldeye',
			newVersion: '25.10.9',
			notes: 'Fixes for SMB & apps.',
			notesUrl: 'https://www.truenas.com/docs/scale/25.10/',
			error: null
		});
	});

	it('accepts an event wrapped in {status}', () => {
		expect(parseUpdateStatus({ status: available() })).toEqual(parseUpdateStatus(available()));
	});

	it('keeps only https release-note links', () => {
		for (const url of ['javascript:alert(1)', 'http://x.example/', 'not a url', 7, null]) {
			expect(parseUpdateStatus(available({ release_notes_url: url }))?.notesUrl).toBeNull();
		}
	});

	it('treats missing notes and a blank version sensibly', () => {
		const noNotes = parseUpdateStatus(available({ release_notes: null }));
		expect(noNotes?.kind).toBe('available');
		expect(noNotes?.notes).toBeNull();
		expect(parseUpdateStatus(available({ version: ' ' }))?.kind).toBe('current');
	});

	it('explains a failed check', () => {
		const raw = {
			code: 'ERROR',
			status: null,
			error: { errname: 'ENONET', reason: 'Unable to reach the update server' },
			update_download_progress: null
		};
		expect(parseUpdateStatus(raw)?.error).toBe(
			"Couldn't check for updates: Unable to reach the update server"
		);
		expect(parseUpdateStatus({ ...raw, error: null })?.error).toBe("Couldn't check for updates.");
	});

	it('refuses other shapes', () => {
		for (const raw of [null, 'x', [], {}, { code: 'NORMAL', status: null }, { code: 'OTHER' }]) {
			expect(parseUpdateStatus(raw)).toBeNull();
		}
	});
});

describe('httpsUrl', () => {
	it('keeps https only', () => {
		expect(httpsUrl('https://a.example/x')).toBe('https://a.example/x');
		expect(httpsUrl('HTTP://a.example')).toBeNull();
	});
});

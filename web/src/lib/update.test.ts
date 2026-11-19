import { describe, expect, it } from 'vitest';
import {
	canInstallUpdates,
	httpsUrl,
	parseUpdateStatus,
	runFailure,
	updateRunCall
} from './update';

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

describe('restart required', () => {
	it('reads an installed update waiting for a restart', () => {
		const raw = {
			code: 'ERROR',
			status: null,
			error: {
				errname: 'EREBOOTREQUIRED',
				reason: 'System update was already applied, system reboot is required.'
			},
			update_download_progress: null
		};
		expect(parseUpdateStatus(raw)?.kind).toBe('restart');
		// Another errname spelling: the reason still tells.
		expect(parseUpdateStatus({ ...raw, error: { ...raw.error, errname: 'E1' } })?.kind).toBe(
			'restart'
		);
		expect(
			parseUpdateStatus({ ...raw, error: { errname: 'ENONET', reason: 'no network' } })?.kind
		).toBe('error');
	});
});

describe('updateRunCall', () => {
	it('pins the train and version on offer', () => {
		const s = parseUpdateStatus(available())!;
		expect(updateRunCall(s, { reboot: true })).toEqual({
			method: 'update.run',
			params: [{ train: 'TrueNAS-SCALE-Goldeye', version: '25.10.9', reboot: true, resume: false }]
		});
		expect(updateRunCall(s, { reboot: false, resume: true })?.params).toEqual([
			{ train: 'TrueNAS-SCALE-Goldeye', version: '25.10.9', reboot: false, resume: true }
		]);
	});

	it('gives nothing without an update or a train', () => {
		expect(updateRunCall(parseUpdateStatus(status())!, { reboot: true })).toBeNull();
		const noTrain = { ...parseUpdateStatus(available())!, train: null };
		expect(updateRunCall(noTrain, { reboot: true })).toBeNull();
	});
});

describe('canInstallUpdates', () => {
	it('needs SYSTEM_UPDATE_WRITE or FULL_ADMIN', () => {
		expect(canInstallUpdates(['SYSTEM_UPDATE_WRITE'])).toBe(true);
		expect(canInstallUpdates(['FULL_ADMIN'])).toBe(true);
		expect(canInstallUpdates(['SYSTEM_UPDATE_READ'])).toBe(false);
		expect(canInstallUpdates(undefined)).toBe(false);
	});
});

describe('runFailure', () => {
	const nas = (errname: string, message = 'm') => ({ code: 'nas' as const, message, errname });
	const drop = { code: 'reconnecting' as const, message: 'gone' };
	const browserDrop = { code: 'disconnected' as const, message: 'gone' };

	it('turns a pre-check warning into a question', () => {
		expect(
			runFailure(nas('EAGAIN', 'Old boot environments'), { reboot: true, percent: 60 })
		).toEqual({ kind: 'warning', message: 'Old boot environments' });
	});

	it('treats a drop during the install with a restart as the restart', () => {
		expect(runFailure(drop, { reboot: true, percent: 80 })).toEqual({ kind: 'restarting' });
		expect(runFailure(drop, { reboot: true, percent: 20 }).kind).toBe('error');
		expect(runFailure(drop, { reboot: false, percent: 80 }).kind).toBe('error');
		expect(runFailure(drop, { reboot: true, percent: null }).kind).toBe('error');
	});

	it("doesn't take the browser's own drop for the restart", () => {
		expect(runFailure(browserDrop, { reboot: true, percent: 80 })).toMatchObject({
			kind: 'error',
			message: expect.stringMatching(/may still be going on/)
		});
	});

	it('words role, space and other errors plainly', () => {
		const opts = { reboot: true, percent: null };
		expect(runFailure(nas('EACCES'), opts)).toMatchObject({
			message: expect.stringMatching(/SYSTEM_UPDATE_WRITE/)
		});
		expect(runFailure(nas('ENOSPC'), opts)).toMatchObject({
			message: expect.stringMatching(/space/)
		});
		expect(runFailure(nas('EINVAL', 'No update available'), opts)).toEqual({
			kind: 'error',
			message: "Couldn't install the update: No update available"
		});
	});
});

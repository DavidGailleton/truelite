// TrueNAS updates from `update.status` (api.truenas.com v25.10, role SYSTEM_UPDATE_READ). The
// server trims results and events to one flat shape without `manifest` (docs/ws-protocol.md).
// Installing one (#41) is the `update.run` job (role SYSTEM_UPDATE_WRITE): download 0–50 %,
// install 50–100 %, then an optional restart. Framework-free so it can be unit-tested: the Svelte
// bindings live in update.svelte.ts and update-run.svelte.ts.
import type { JobCall } from './app-actions';
import { changelogText } from './app-upgrade';
import { hasRole, isObj, isStr } from './system';
import { type ErrorBody, BUSY_MESSAGE } from './ws';

export const UPDATE_STATUS = 'update.status';

export interface UpdateStatus {
	/**
	 * Up to date, an update is available, one is installed and waits for a restart, or the check
	 * failed.
	 */
	kind: 'current' | 'available' | 'restart' | 'error';
	/** The update channel (TrueNAS "train"), `null` when unknown. */
	train: string | null;
	/** The version on offer, when `available`. */
	newVersion: string | null;
	/** The release notes as plain text, `null` when there are none. */
	notes: string | null;
	/** An https link to the full release notes, `null` otherwise. */
	notesUrl: string | null;
	/** Why the check failed, in plain words, when `error`. */
	error: string | null;
}

/** `url` when it is an https link, `null` otherwise (never a `javascript:` or `http:` one). */
export function httpsUrl(url: unknown): string | null {
	if (!isStr(url)) return null;
	try {
		return new URL(url).protocol === 'https:' ? url : null;
	} catch {
		return null;
	}
}

/**
 * An `update.status` result, or the `fields` of its event (flat, or wrapped in `{status}` if
 * the server ever passes one through). `null` when it does not have the documented shape.
 */
export function parseUpdateStatus(raw: unknown): UpdateStatus | null {
	if (isObj(raw) && !('code' in raw) && isObj(raw.status)) raw = raw.status;
	if (!isObj(raw)) return null;
	const empty = { train: null, newVersion: null, notes: null, notesUrl: null, error: null };
	if (raw.code === 'ERROR') {
		const reason = isObj(raw.error) && isStr(raw.error.reason) ? raw.error.reason.trim() : '';
		const errname = isObj(raw.error) && isStr(raw.error.errname) ? raw.error.errname : '';
		// TrueNAS answers this way once an update is installed without restarting.
		if (errname === 'EREBOOTREQUIRED' || /reboot is required/i.test(reason)) {
			return { ...empty, kind: 'restart' };
		}
		return {
			...empty,
			kind: 'error',
			error: reason ? `Couldn't check for updates: ${reason}` : "Couldn't check for updates."
		};
	}
	if (raw.code !== 'NORMAL' || !isObj(raw.status)) return null;
	const { current_version: current, new_version: next } = raw.status;
	const train = isObj(current) && isStr(current.train) && current.train ? current.train : null;
	if (!isObj(next) || !isStr(next.version) || next.version.trim() === '') {
		return { ...empty, kind: 'current', train };
	}
	return {
		...empty,
		kind: 'available',
		train,
		newVersion: next.version.trim(),
		notes: isStr(next.release_notes) ? changelogText(next.release_notes) : null,
		notesUrl: httpsUrl(next.release_notes_url)
	};
}

export const UPDATE_RUN = 'update.run';

/** The `update.run` call installing what `status` offers; `null` when nothing is on offer. */
export function updateRunCall(
	status: UpdateStatus,
	{ reboot, resume = false }: { reboot: boolean; resume?: boolean }
): JobCall | null {
	if (status.kind !== 'available' || !status.train || !status.newVersion) return null;
	return {
		method: UPDATE_RUN,
		params: [{ train: status.train, version: status.newVersion, reboot, resume }]
	};
}

/** Whether the account may install updates. */
export const canInstallUpdates = (roles: readonly string[] | undefined) =>
	hasRole(roles, 'SYSTEM_UPDATE_WRITE');

/** The job reached its install half (progress ≥ 50 %): the download is done. */
export const INSTALL_PERCENT = 50;

/**
 * How an `update.run` call that failed ended: a pre-check `warning` the user may accept (call
 * again with `resume`), the NAS `restarting` (truelite's link to the NAS dropped once the install
 * was under way and a restart was asked), or an `error` in plain words. The browser's own link
 * dropping (`disconnected`, a locked phone) says nothing about the NAS: the job goes on there.
 */
export type RunFailure =
	| { kind: 'warning'; message: string }
	| { kind: 'restarting' }
	| { kind: 'error'; message: string };

export function runFailure(
	error: ErrorBody,
	{ reboot, percent }: { reboot: boolean; percent: number | null }
): RunFailure {
	if (error.errname === 'EAGAIN') return { kind: 'warning', message: error.message };
	if (error.errname === 'EACCES') {
		return {
			kind: 'error',
			message:
				"Your account can't install updates. It needs the update write role (SYSTEM_UPDATE_WRITE) on the NAS."
		};
	}
	if (error.errname === 'ENOSPC') {
		return {
			kind: 'error',
			message:
				'Not enough free space on the boot disk for the update. Free some space and try again.'
		};
	}
	switch (error.code) {
		case 'reconnecting':
		case 'disconnected':
			if (
				error.code === 'reconnecting' &&
				reboot &&
				percent !== null &&
				percent >= INSTALL_PERCENT
			) {
				return { kind: 'restarting' };
			}
			return {
				kind: 'error',
				message:
					'The connection dropped while the update was running. It may still be going on: check this page again in a few minutes.'
			};
		case 'busy':
			return { kind: 'error', message: BUSY_MESSAGE };
		default:
			return { kind: 'error', message: `Couldn't install the update: ${error.message}` };
	}
}

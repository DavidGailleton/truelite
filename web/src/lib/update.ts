// TrueNAS updates from `update.status` (api.truenas.com v25.10, role SYSTEM_UPDATE_READ). The
// server trims results and events to one flat shape without `manifest` (docs/ws-protocol.md).
// Framework-free so it can be unit-tested: the Svelte binding lives in update.svelte.ts.
import { changelogText } from './app-upgrade';
import { isObj, isStr } from './system';

export const UPDATE_STATUS = 'update.status';

export interface UpdateStatus {
	/** Up to date, an update is available, or the check failed. */
	kind: 'current' | 'available' | 'error';
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

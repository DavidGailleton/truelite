// What a dashboard card shows, from its data, its last error and the connection. Pure, so the
// rules are unit-tested once instead of living in five templates.
import type { ErrorBody, Status } from './ws';

export type CardView =
	/** Nothing yet: skeleton. */
	| { kind: 'loading' }
	/** The account lacks the TrueNAS role: explain, no retry (it would fail the same way). */
	| { kind: 'denied' }
	/** Failed before any data arrived: message and "Try again". */
	| { kind: 'error'; error: ErrorBody }
	/**
	 * Loaded: `empty` has nothing to list, `ready` has data to show. `stale`: updates stopped
	 * after it arrived (show it dimmed with a notice and "Try again"). `paused`: the link is down
	 * for now and comes back on its own.
	 */
	| { kind: 'empty' | 'ready'; stale: ErrorBody | null; paused: boolean };

export interface CardInput {
	/** Whether a first answer arrived. */
	hasData: boolean;
	/** Whether that answer has nothing to show; ignored without data. */
	isEmpty?: boolean;
	/** The card's last error, `null` after a success. */
	error: ErrorBody | null;
	status: Status;
}

/** A NAS refusal for a missing role. */
export function isDenied(error: ErrorBody | null): boolean {
	return error?.code === 'nas' && error.errname === 'EACCES';
}

export function cardView({ hasData, isEmpty = false, error, status }: CardInput): CardView {
	// Old data stays hidden after a role change: it would look live and isn't.
	if (isDenied(error)) return { kind: 'denied' };
	if (!hasData) return error ? { kind: 'error', error } : { kind: 'loading' };
	return { kind: isEmpty ? 'empty' : 'ready', stale: error, paused: status !== 'up' };
}

/** The header badge of a card with data: why its numbers aren't moving, if they aren't. */
export function badgeLabel(view: CardView): string | null {
	if (view.kind !== 'ready' && view.kind !== 'empty') return null;
	if (view.stale) return 'Not live';
	return view.paused ? 'Paused' : null;
}

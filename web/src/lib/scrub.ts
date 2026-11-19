// Data checks (#38): start, pause, resume and stop a scrub with the TrueNAS job
// `pool.scrub.scrub` (api.truenas.com v25.10; role POOL_WRITE). START's job lasts the whole scrub
// and PAUSE/STOP end at once (middlewared TS-25.10.8); the scan line follows the `pool.scan`
// events, not the job. Which buttons fit a pool, and the words of their dialogs. Framework-free
// so it can be unit-tested: the Svelte binding lives in scrub.svelte.ts.
import type { JobCall } from './app-actions';
import { UNAVAILABLE, type Scan } from './pools';
import { hasRole } from './system';
import type { ErrorBody } from './ws';

export const SCRUB_METHOD = 'pool.scrub.scrub';

export type ScrubAction = 'start' | 'resume' | 'pause' | 'stop';

/** The job's action argument; a paused scrub resumes with START. */
const ACTION_ARGS: Record<ScrubAction, 'START' | 'PAUSE' | 'STOP'> = {
	start: 'START',
	resume: 'START',
	pause: 'PAUSE',
	stop: 'STOP'
};

/** The job call of `action` on pool `name`, as the bridge allowlists it. */
export function scrubCall(name: string, action: ScrubAction): JobCall {
	return { method: SCRUB_METHOD, params: [name, ACTION_ARGS[action]] };
}

const isRunningScrub = (scan: Scan | null): scan is Scan =>
	scan?.kind === 'scrub' && scan.state === 'running';

/**
 * The buttons for a pool in `status` with `scan`, in order. None for a pool that is offline or
 * rebuilding onto a new disk (ZFS can't check data then).
 */
export function scrubActions(status: string, scan: Scan | null): ScrubAction[] {
	if (UNAVAILABLE.has(status)) return [];
	if (scan?.kind === 'resilver' && scan.state === 'running') return [];
	if (isRunningScrub(scan)) return scan.paused ? ['resume', 'stop'] : ['pause', 'stop'];
	return ['start'];
}

/** Whether the scan shows what `action` asked for: the run can step aside for the scan line. */
export function scrubSettled(action: ScrubAction, scan: Scan | null): boolean {
	switch (action) {
		case 'start':
		case 'resume':
			return isRunningScrub(scan) && !scan.paused;
		case 'pause':
			return isRunningScrub(scan) && scan.paused !== null;
		case 'stop':
			return !isRunningScrub(scan);
	}
}

export const SCRUB_LABELS: Record<ScrubAction, string> = {
	start: 'Check now',
	resume: 'Resume',
	pause: 'Pause',
	stop: 'Stop'
};

/** What the progress line says until the scan shows the change. */
export const SCRUB_RUNNING: Record<ScrubAction, string> = {
	start: 'Starting the data check…',
	resume: 'Resuming…',
	pause: 'Pausing…',
	stop: 'Stopping…'
};

export interface ScrubDialog {
	title: string;
	text: string;
	confirm: string;
	/** Stopping loses the progress: a destructive button. */
	destructive: boolean;
}

/** The dialog of `action` on pool `name`. */
export function scrubDialog(action: ScrubAction, name: string): ScrubDialog {
	switch (action) {
		case 'start':
			return {
				title: `Check ${name} for errors?`,
				text: 'Reads all data on the pool and repairs what it can. It can take hours and slows the pool down while it runs; you can pause it.',
				confirm: 'Start check',
				destructive: false
			};
		case 'resume':
			return {
				title: `Resume the check of ${name}?`,
				text: 'It goes on where it stopped and slows the pool down again until it ends.',
				confirm: 'Resume',
				destructive: false
			};
		case 'pause':
			return {
				title: `Pause the check of ${name}?`,
				text: 'The pool gets its full speed back. Resume later to go on where it stopped.',
				confirm: 'Pause',
				destructive: false
			};
		case 'stop':
			return {
				title: `Stop the check of ${name}?`,
				text: 'Its progress is lost: the next check starts from the beginning.',
				confirm: 'Stop check',
				destructive: true
			};
	}
}

/** Whether the account may start, pause and stop checks. */
export const canScrub = (roles: readonly string[] | undefined) => hasRole(roles, 'POOL_WRITE');

/** Why `action` failed, in plain words. */
export function scrubError(action: ScrubAction, error: ErrorBody): string {
	if (error.errname === 'EACCES') {
		return "Your account can't check pools. It needs the Pool write role (POOL_WRITE) on the NAS.";
	}
	const what = {
		start: 'start the check',
		resume: 'resume the check',
		pause: 'pause the check',
		stop: 'stop the check'
	}[action];
	switch (error.code) {
		case 'disconnected':
		case 'reconnecting':
			return `The connection dropped while truelite asked the NAS to ${what}. It may have worked: watch the pool's status line.`;
		case 'busy':
			return 'Too many things at once on this page. Try again in a moment.';
		default:
			return `Couldn't ${what}: ${error.message}`;
	}
}

// Restarting and shutting down the NAS (#42): the `system.reboot` and `system.shutdown` jobs
// (role FULL_ADMIN). The server sets their params; the browser sends none. The job runs the
// restart or power-off itself, so the reply may never come: a dropped link means it worked.
// Framework-free so it can be unit-tested: the Svelte binding lives in power.svelte.ts.
import type { JobCall } from './app-actions';
import { hasRole } from './system';
import { type ErrorBody, BUSY_MESSAGE } from './ws';

export type PowerAction = 'reboot' | 'shutdown';

const METHODS: Record<PowerAction, string> = {
	reboot: 'system.reboot',
	shutdown: 'system.shutdown'
};

export const powerCall = (action: PowerAction): JobCall => ({
	method: METHODS[action],
	params: []
});

/** Only a full admin may restart or shut down the NAS (TrueNAS has no narrower role for it). */
export const canPower = (roles: readonly string[] | undefined) => hasRole(roles, 'FULL_ADMIN');

/** The words of each action: the button, the dialog, and the note once the NAS goes down. */
export const POWER_TEXT: Record<
	PowerAction,
	{ button: string; title: string; text: string; goingDown: string; error: string }
> = {
	reboot: {
		button: 'Restart',
		title: 'Restart',
		text: 'Apps, shares and VMs stop. The NAS is unreachable for a few minutes; then log in again.',
		goingDown: 'Restarting. Log in again when the NAS is back, in a few minutes.',
		error: "Couldn't restart the NAS"
	},
	shutdown: {
		button: 'Shut down',
		title: 'Shut down',
		text: "Apps, shares and VMs stop. The NAS stays off until someone powers it on: truelite can't turn it back on.",
		goingDown: 'Shutting down. The NAS stays off until someone powers it on.',
		error: "Couldn't shut down the NAS"
	}
};

/**
 * How a `system.reboot` / `system.shutdown` call ended: `null` (it answered), the NAS link
 * dropping (`reconnecting`) or the browser's own link dropping after the call went out
 * (`disconnected`, a guess the restart wait undoes if the NAS stays up) mean the NAS is going
 * down. An ended session (`logged_out`) proves nothing: the call may never have reached the NAS.
 * Anything else is an error in plain words.
 */
export function powerOutcome(
	action: PowerAction,
	error: ErrorBody | null
): { kind: 'going-down' } | { kind: 'error'; message: string } {
	if (error === null) return { kind: 'going-down' };
	switch (error.code) {
		case 'disconnected':
		case 'reconnecting':
			return { kind: 'going-down' };
		case 'logged_out':
			return {
				kind: 'error',
				message:
					'Your session ended before the NAS answered. Log in again and check whether it restarted.'
			};
		case 'busy':
			return { kind: 'error', message: BUSY_MESSAGE };
	}
	if (error.errname === 'EACCES') {
		return {
			kind: 'error',
			message:
				"Your account can't do this. Restarting or shutting down needs a full admin account on the NAS."
		};
	}
	return { kind: 'error', message: `${POWER_TEXT[action].error}: ${error.message}` };
}

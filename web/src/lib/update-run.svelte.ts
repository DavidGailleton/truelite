// Installing a TrueNAS update (#41): one `update.run` job for the whole visit, so its progress
// survives navigation. Cleared on logout (a restart ends the NAS session, so it clears too).
// The wording and the failure rules live in update.ts.
import { nas, onNasClose } from './nas.svelte';
import { powerBusy } from './power.svelte';
import { beginRestartWait, onRestartEnd } from './restart.svelte';
import { system } from './system.svelte';
import { runFailure, updateRunCall, type UpdateStatus } from './update';
import { loadUpdateStatus, update } from './update.svelte';
import { failure, type JobProgress } from './ws';

export type UpdateRun =
	| { phase: 'running'; reboot: boolean; progress: JobProgress | null }
	/**
	 * A pre-check warning: the user may install anyway ({@link resumeUpdate}). `status` is the
	 * offer the user confirmed: going on installs that one, never a newer one.
	 */
	| { phase: 'warning'; status: UpdateStatus; reboot: boolean; message: string }
	/** Installed; the NAS restarts on its own. */
	| { phase: 'restarting' }
	/** Installed without a restart: `update.status` now says "restart needed". */
	| { phase: 'installed' }
	| { phase: 'failed'; message: string };

export const updateRun = $state<{ run: UpdateRun | null }>({ run: null });

let generation = 0;

onNasClose(() => {
	generation++;
	updateRun.run = null;
});

// The wait ended without a new session, whatever the reason (an update wait is never a guess):
// the card no longer says it restarts. `update.status` then tells what got installed.
onRestartEnd(() => {
	if (updateRun.run?.phase !== 'restarting') return;
	generation++;
	updateRun.run = null;
});

/** Whether a job is running or waits for the user (so no second one can start). */
export function updateBusy(): boolean {
	const phase = updateRun.run?.phase;
	return phase === 'running' || phase === 'warning' || phase === 'restarting';
}

/**
 * Installs what `status` offers. Nothing starts while a job runs or waits for the user, or while
 * a restart or shutdown is under way.
 */
export function installUpdate(status: UpdateStatus, { reboot }: { reboot: boolean }): void {
	if (updateBusy() || powerBusy()) return;
	run(status, reboot, false);
}

/**
 * Accepts the warning the NAS gave: installs the offer the user confirmed, with `resume`. If
 * the NAS now offers something else (or nothing), the warning is dropped instead.
 */
export function resumeUpdate(): void {
	const current = updateRun.run;
	if (current?.phase !== 'warning' || powerBusy()) return;
	const now = update.status;
	if (now?.kind !== 'available' || now.newVersion !== current.status.newVersion) {
		dismissUpdateRun();
		return;
	}
	run(current.status, current.reboot, true);
}

function run(status: UpdateStatus, reboot: boolean, resume: boolean): void {
	const call = updateRunCall(status, { reboot, resume });
	if (!call) return;
	const mine = ++generation;
	updateRun.run = { phase: 'running', reboot, progress: null };
	let percent: number | null = null;
	nas()
		.call(call.method, call.params, {
			timeout: null,
			onProgress: (progress) => {
				if (mine !== generation) return;
				if (progress.percent !== null) percent = progress.percent;
				updateRun.run = { phase: 'running', reboot, progress };
			}
		})
		.then(
			() => {
				if (mine !== generation) return;
				updateRun.run = reboot ? { phase: 'restarting' } : { phase: 'installed' };
				if (reboot) waitForRestart(false);
				else loadUpdateStatus();
			},
			(e: unknown) => {
				if (mine !== generation) return;
				const end = runFailure(failure(e), { reboot, percent });
				updateRun.run =
					end.kind === 'warning'
						? { phase: 'warning', status, reboot, message: end.message }
						: end.kind === 'restarting'
							? { phase: 'restarting' }
							: { phase: 'failed', message: end.message };
				if (end.kind === 'restarting') waitForRestart(true);
			}
		);
}

/** `viaDrop`: truelite's link to the NAS dropped, so the NAS is already seen down. */
function waitForRestart(viaDrop: boolean): void {
	beginRestartWait('reboot', system.info?.hostname ?? null, { down: viaDrop });
}

/** Forgets a failed run, a declined warning or an "installed" note. */
export function dismissUpdateRun(): void {
	if (updateRun.run?.phase !== 'running' && updateRun.run?.phase !== 'restarting') {
		generation++;
		updateRun.run = null;
	}
}

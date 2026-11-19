// Installing a TrueNAS update (#41): one `update.run` job for the whole visit, so its progress
// survives navigation. Cleared on logout (a restart ends the NAS session, so it clears too).
// The wording and the failure rules live in update.ts.
import { nas, onNasClose } from './nas.svelte';
import { beginRestartWait } from './restart.svelte';
import { system } from './system.svelte';
import { runFailure, updateRunCall, type UpdateStatus } from './update';
import { loadUpdateStatus } from './update.svelte';
import { failure, type JobProgress } from './ws';

export type UpdateRun =
	| { phase: 'running'; reboot: boolean; progress: JobProgress | null }
	/** A pre-check warning: the user may install anyway (`resume`). */
	| { phase: 'warning'; reboot: boolean; message: string }
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

/** Whether a job is running or waits for the user (so no second one can start). */
export function updateBusy(): boolean {
	const phase = updateRun.run?.phase;
	return phase === 'running' || phase === 'warning' || phase === 'restarting';
}

/** Installs what `status` offers; `resume` accepts a warning the NAS gave on the last try. */
export function installUpdate(
	status: UpdateStatus,
	{ reboot, resume = false }: { reboot: boolean; resume?: boolean }
): void {
	const call = updateRunCall(status, { reboot, resume });
	if (!call || updateRun.run?.phase === 'running') return;
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
				if (reboot) waitForRestart();
				else loadUpdateStatus();
			},
			(e: unknown) => {
				if (mine !== generation) return;
				const end = runFailure(failure(e), { reboot, percent });
				updateRun.run =
					end.kind === 'warning'
						? { phase: 'warning', reboot, message: end.message }
						: end.kind === 'restarting'
							? { phase: 'restarting' }
							: { phase: 'failed', message: end.message };
				if (end.kind === 'restarting') waitForRestart();
			}
		);
}

function waitForRestart(): void {
	beginRestartWait('reboot', system.info?.hostname ?? null);
}

/** Forgets a failed run, a declined warning or an "installed" note. */
export function dismissUpdateRun(): void {
	if (updateRun.run?.phase !== 'running' && updateRun.run?.phase !== 'restarting') {
		generation++;
		updateRun.run = null;
	}
}

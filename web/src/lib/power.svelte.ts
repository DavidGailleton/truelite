// Restart / shut down (#42): one state for the whole visit, so "Restarting…" survives navigation.
// Cleared on logout (once the NAS is back, its old session is gone and the user logs in again).
// The wording and the outcome rules live in power.ts.
import { nas, onNasClose } from './nas.svelte';
import { POWER_TEXT, powerCall, powerOutcome, type PowerAction } from './power';
import { beginRestartWait, onRestartEnd } from './restart.svelte';
import { system } from './system.svelte';
import { failure } from './ws';

export type PowerRun =
	| { phase: 'running'; action: PowerAction }
	/** The NAS accepted it, or a link dropped: it is going down (until the restart wait ends). */
	| { phase: 'going-down'; action: PowerAction }
	| { phase: 'failed'; message: string };

export const power = $state<{ run: PowerRun | null }>({ run: null });

let generation = 0;

onNasClose(() => {
	generation++;
	power.run = null;
});

// The wait ended without a new session: the NAS came back, the wait expired, or the user stopped
// waiting: nothing is going down any more. A NAS that never went down gets a failure note.
onRestartEnd((reason) => {
	if (power.run?.phase !== 'going-down') return;
	const { action } = power.run;
	generation++;
	power.run =
		reason === 'not-down' ? { phase: 'failed', message: POWER_TEXT[action].notDown } : null;
});

/** Whether a restart or shutdown is under way (so nothing else starts). */
export function powerBusy(): boolean {
	const phase = power.run?.phase;
	return phase === 'running' || phase === 'going-down';
}

export function startPower(action: PowerAction): void {
	// No `updateBusy()` check here: update-run imports this module, so that would be a cycle.
	// PowerCard disables its buttons while an update installs.
	if (powerBusy()) return;
	const mine = ++generation;
	power.run = { phase: 'running', action };
	const call = powerCall(action);
	const end = (error: ReturnType<typeof failure> | null) => {
		if (mine !== generation) return;
		const outcome = powerOutcome(action, error);
		if (outcome.kind === 'going-down') {
			power.run = { phase: 'going-down', action };
			// Only the browser's own link dropping is a guess; the NAS's link dropping
			// (`reconnecting`) means it is already down.
			beginRestartWait(action, system.info?.hostname ?? null, {
				guess: error?.code === 'disconnected',
				down: error?.code === 'reconnecting'
			});
		} else {
			power.run = { phase: 'failed', message: outcome.message };
		}
	};
	nas()
		.call(call.method, call.params, { timeout: null })
		.then(
			() => end(null),
			(e: unknown) => end(failure(e))
		);
}

/** Forgets a failed attempt. */
export function dismissPower(): void {
	if (power.run?.phase === 'failed') {
		generation++;
		power.run = null;
	}
}

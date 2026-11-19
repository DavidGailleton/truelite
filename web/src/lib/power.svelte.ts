// Restart / shut down (#42): one state for the whole visit, so "Restarting…" survives navigation.
// Cleared on logout (once the NAS is back, its old session is gone and the user logs in again).
// The wording and the outcome rules live in power.ts.
import { nas, onNasClose } from './nas.svelte';
import { powerCall, powerOutcome, type PowerAction } from './power';
import { failure } from './ws';

export type PowerRun =
	| { phase: 'running'; action: PowerAction }
	/** The NAS accepted it, or the link dropped: it is going down. */
	| { phase: 'going-down'; action: PowerAction }
	| { phase: 'failed'; message: string };

export const power = $state<{ run: PowerRun | null }>({ run: null });

let generation = 0;

onNasClose(() => {
	generation++;
	power.run = null;
});

/** Whether a restart or shutdown is under way (so nothing else starts). */
export function powerBusy(): boolean {
	const phase = power.run?.phase;
	return phase === 'running' || phase === 'going-down';
}

export function startPower(action: PowerAction): void {
	if (powerBusy()) return;
	const mine = ++generation;
	power.run = { phase: 'running', action };
	const call = powerCall(action);
	const end = (error: ReturnType<typeof failure> | null) => {
		if (mine !== generation) return;
		const outcome = powerOutcome(action, error);
		power.run =
			outcome.kind === 'going-down'
				? { phase: 'going-down', action }
				: { phase: 'failed', message: outcome.message };
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

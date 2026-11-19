// The wait for a restarting or shut-down NAS (#43), as reactive state kept in sessionStorage so
// it survives the session ending and a page reload. Deliberately not reset by onNasClose: the
// login page goes on with it. The rules live in restart.ts.
import { untrack } from 'svelte';
import {
	parseWait,
	RESTART_KEY,
	serializeWait,
	stepWait,
	waitDeadline,
	type RestartKind,
	type RestartWait,
	type WaitEnd,
	type WaitSignal
} from './restart';

function storage(): Storage | null {
	try {
		return sessionStorage;
	} catch {
		return null;
	}
}

function read(): RestartWait | null {
	try {
		return parseWait(storage()?.getItem(RESTART_KEY) ?? null, Date.now());
	} catch {
		return null;
	}
}

export const restart = $state<{ wait: RestartWait | null }>({ wait: read() });

const ends: ((reason: WaitEnd) => void)[] = [];

/**
 * Registers a reset run by {@link endRestartWait} with why the wait ended, so a store that took
 * the NAS for going down stops saying so (and stops blocking its buttons) once the wait ends.
 * Stores call it once, at module load.
 */
export function onRestartEnd(reset: (reason: WaitEnd) => void): void {
	ends.push(reset);
}

function store(wait: RestartWait): void {
	restart.wait = wait;
	try {
		storage()?.setItem(RESTART_KEY, serializeWait(wait));
	} catch {
		// Private mode or full storage: the wait only lasts until a reload.
	}
}

/**
 * The NAS is going down: show the restart screen until it is back. `guess`: only the browser's
 * link dropped, so the NAS may still be up. `down`: the NAS is already seen down.
 */
export function beginRestartWait(
	kind: RestartKind,
	host: string | null,
	{ guess = false, down = false }: { guess?: boolean; down?: boolean } = {}
): void {
	store({ kind, host, since: Date.now(), guess, down });
}

/** The NAS is back, never went down, the wait expired, or the user stopped waiting. */
export function endRestartWait(reason: WaitEnd = 'stopped'): void {
	restart.wait = null;
	try {
		storage()?.removeItem(RESTART_KEY);
	} catch {
		// Nothing stored then.
	}
	for (const reset of ends) reset(reason);
}

/**
 * Applies what is known of the NAS to the wait (see `stepWait`): stores it once the NAS is seen
 * down, ends it when the NAS is back, never went down, or the wait expired. Returns why it ended,
 * else `null`.
 */
export function applyWaitSignal(signal: WaitSignal): WaitEnd | null {
	const wait = restart.wait;
	if (!wait) return null;
	const next = stepWait(wait, signal, Date.now());
	if ('end' in next) {
		endRestartWait(next.end);
		return next.end;
	}
	if (next !== wait) store(next);
	return null;
}

/** The NAS is seen down (no session: a restart always ends it). */
export function markWaitDown(): void {
	applyWaitSignal('down');
}

/**
 * Follows the wait while the calling component is mounted: applies `signal()` whenever it or the
 * wait changes, and again at the wait's deadline. `onend` runs when this watch ended the wait,
 * with the wait that ended.
 * Call from component setup.
 */
export function useRestartWatch(
	signal: () => WaitSignal,
	onend?: (reason: WaitEnd, wait: RestartWait) => void
): void {
	const apply = () => {
		const wait = restart.wait;
		const end = applyWaitSignal(signal());
		if (end && wait) onend?.(end, wait);
	};
	$effect(() => {
		const wait = restart.wait;
		if (!wait) return;
		signal(); // re-run when it changes
		untrack(apply);
		// A changed or ended wait runs this effect again, which sets the right timer.
		if (untrack(() => restart.wait) !== wait) return;
		const timer = setTimeout(apply, Math.max(0, waitDeadline(wait) - Date.now()));
		return () => clearTimeout(timer);
	});
}

// The wait for a restarting or shut-down NAS (#43), as reactive state kept in sessionStorage so
// it survives the session ending and a page reload. Deliberately not reset by onNasClose: the
// login page goes on with it. The rules live in restart.ts.
import {
	parseWait,
	RESTART_KEY,
	serializeWait,
	type RestartKind,
	type RestartWait
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

/** The NAS is going down: show the restart screen until it is back. */
export function beginRestartWait(kind: RestartKind, host: string | null): void {
	const wait: RestartWait = { kind, host, since: Date.now() };
	restart.wait = wait;
	try {
		storage()?.setItem(RESTART_KEY, serializeWait(wait));
	} catch {
		// Private mode or full storage: the wait only lasts until a reload.
	}
}

/** The NAS is back, or the user stopped waiting. */
export function endRestartWait(): void {
	restart.wait = null;
	try {
		storage()?.removeItem(RESTART_KEY);
	} catch {
		// Nothing stored then.
	}
}

// Svelte binding of `system.info`: one call per connection, shared by the dashboard header and
// the System card. No event exists for it, so uptime ticks in the browser between calls.
import { nas, onNasClose, pageVisible } from './nas.svelte';
import { SYSTEM_INFO, parseSystemInfo, refetchOnUp, type SystemInfo } from './system';
import { shownError, type ErrorBody } from './ws';

/** Clock tick for the uptime; minute precision is all it shows. */
const TICK_MS = 30_000;

export const system = $state<{
	info: SystemInfo | null;
	/** Why the last call failed; `null` after a success. */
	error: ErrorBody | null;
}>({ info: null, error: null });

/** `performance.now()` when `info` arrived, and the latest tick. */
const clock = $state({ received: 0, now: 0 });
let request = 0;

onNasClose(() => {
	request++;
	system.info = null;
	system.error = null;
});

/** Asks the NAS again. Errors from a dropped link are not kept: the next `up` refetches. */
export function loadSystemInfo(): void {
	const mine = ++request;
	nas()
		.call(SYSTEM_INFO)
		.then((raw) => {
			if (mine !== request) return;
			const info = parseSystemInfo(raw);
			if (!info) {
				system.error = {
					code: 'bad_request',
					message: "The NAS sent details truelite can't read."
				};
				return;
			}
			system.info = info;
			system.error = null;
			clock.received = clock.now = performance.now();
		})
		.catch((e: unknown) => {
			if (mine !== request) return;
			const error = shownError(e);
			if (error) system.error = error;
		});
}

/** Seconds since boot now, or `null` before the first answer. Reactive. */
export function uptimeNow(): number | null {
	const info = system.info;
	if (!info) return null;
	return info.uptimeSeconds + Math.max(0, clock.now - clock.received) / 1000;
}

/**
 * Loads the system details while the calling component is mounted: when connected, again after
 * every reconnect, and ticks the uptime while the page is visible. Call from component setup.
 */
export function useSystemInfo(): void {
	$effect(() => nas().onStatus(refetchOnUp(loadSystemInfo)));
	$effect(() => {
		if (!pageVisible()) return;
		clock.now = performance.now();
		const timer = setInterval(() => (clock.now = performance.now()), TICK_MS);
		return () => clearInterval(timer);
	});
}

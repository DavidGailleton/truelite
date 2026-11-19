// Waiting for the NAS through a restart or a shutdown (#43). The NAS session never survives a
// restart (the NAS forgets truelite's token), so the wait outlives it: it is kept in
// sessionStorage and the login page goes on with it, asking `GET /api/nas` until the NAS
// answers. Framework-free so it can be unit-tested: the Svelte binding lives in
// restart.svelte.ts, the screen in components/RestartScreen.svelte.
//
// The wait ends on what the NAS did, never on a local guess: it must first be seen down, then
// up (`back`). A *sure* wait (the NAS answered the call, or truelite's link to it dropped) has no
// time limit but MAX_WAIT_MS: a busy NAS can take minutes stopping apps and VMs. A *guess* (the
// browser's own link dropped after the call went out) ends as `not-down` when the NAS is still
// up after GUESS_GRACE_MS without having been seen down.
import type { Status } from './ws';

export type RestartKind = 'reboot' | 'shutdown';

export interface RestartWait {
	kind: RestartKind;
	/** The NAS name, when it was known. */
	host: string | null;
	/** When the wait began (ms since the epoch). */
	since: number;
	/** Only a guess that the NAS goes down: the browser's link dropped, not the NAS's. */
	guess: boolean;
	/** The NAS was seen down since the wait began. */
	down: boolean;
}

/** sessionStorage key: per tab, gone when the tab closes. */
export const RESTART_KEY = 'truelite.restart';

/** A wait older than this is forgotten: the NAS is not coming back on its own. */
export const MAX_WAIT_MS = 60 * 60 * 1000;

const MINUTE = 60 * 1000;

/** How long a guessed wait lasts while the NAS is never seen down. */
export const GUESS_GRACE_MS = 5 * MINUTE;

/** The words of the restart screen. */
export function restartTitle({ kind, host }: RestartWait): string {
	const name = host ?? 'the NAS';
	return kind === 'reboot' ? `Restarting ${name}…` : `Shutting down ${name}`;
}

export function restartText(kind: RestartKind): string {
	return kind === 'reboot'
		? 'This takes a few minutes. This page goes on by itself when the NAS is back.'
		: "The NAS stays off until someone powers it on: truelite can't turn it on. This page goes on by itself when it is back.";
}

/** The note on the login form once the NAS answers again. */
export function backText(host: string | null): string {
	return `${host ?? 'The NAS'} is back. Log in again.`;
}

/** "less than a minute", "1 min", "12 min". */
export function elapsedText(ms: number): string {
	const minutes = Math.floor(Math.max(0, ms) / MINUTE);
	return minutes < 1 ? 'less than a minute' : `${minutes} min`;
}

/**
 * How long to wait before asking again whether the NAS is back: often during a restart, rarely
 * for a NAS that was shut down or that takes far longer than a restart should.
 */
export function pollDelay(kind: RestartKind, elapsedMs: number): number {
	return kind === 'reboot' && elapsedMs < 15 * MINUTE ? 3000 : 15000;
}

/** The stored wait, or `null` when there is none, it is malformed or too old. */
export function parseWait(raw: string | null, now: number): RestartWait | null {
	if (!raw) return null;
	let value: unknown;
	try {
		value = JSON.parse(raw);
	} catch {
		return null;
	}
	if (typeof value !== 'object' || value === null) return null;
	const { kind, host, since, guess, down } = value as Record<string, unknown>;
	if (kind !== 'reboot' && kind !== 'shutdown') return null;
	if (host !== null && typeof host !== 'string') return null;
	if (typeof since !== 'number' || !Number.isFinite(since)) return null;
	// A clock set back would make a fresh wait look like it comes from the future.
	if (now - since > MAX_WAIT_MS || since - now > MINUTE) return null;
	// Entries stored before `guess` / `down` existed load as a sure wait not yet seen down.
	return { kind, host, since, guess: guess === true, down: down === true };
}

export function serializeWait(wait: RestartWait): string {
	return JSON.stringify(wait);
}

/** Answer of `GET /api/nas`, plus `unreachable` when truelite itself did not answer. */
export type NasReach = 'up' | 'down' | 'cert_mismatch' | 'unreachable';

/** What is known of the NAS right now: down, up, or nothing (truelite itself is unreachable). */
export type WaitSignal = 'down' | 'up' | 'unknown';

/** Why a wait ended: the NAS came back, never went down, the wait expired, or the user stopped. */
export type WaitEnd = 'back' | 'not-down' | 'expired' | 'stopped';

/**
 * The signal of the live connection's status. truelite says when its link to the NAS drops;
 * the browser being `offline` says nothing about the NAS.
 */
export function statusSignal(status: Status): WaitSignal {
	return status === 'nas-reconnecting' ? 'down' : status === 'up' ? 'up' : 'unknown';
}

/** The signal of a `GET /api/nas` answer. A changed certificate still means the NAS answers. */
export function reachSignal(reach: NasReach | null): WaitSignal {
	if (reach === 'up' || reach === 'cert_mismatch') return 'up';
	return reach === 'down' ? 'down' : 'unknown';
}

/**
 * The wait after `signal`: the same object when nothing changed, a new one once the NAS is seen
 * down, or why it ended.
 */
export function stepWait(
	wait: RestartWait,
	signal: WaitSignal,
	now: number
): RestartWait | { end: WaitEnd } {
	if (now - wait.since >= MAX_WAIT_MS) return { end: 'expired' };
	if (signal === 'down') return wait.down ? wait : { ...wait, down: true };
	if (signal === 'up') {
		if (wait.down) return { end: 'back' };
		if (wait.guess && now - wait.since >= GUESS_GRACE_MS) return { end: 'not-down' };
	}
	return wait;
}

/** When `stepWait` may end the wait with no new signal (ms since the epoch). */
export function waitDeadline(wait: RestartWait): number {
	return wait.since + (wait.guess && !wait.down ? GUESS_GRACE_MS : MAX_WAIT_MS);
}

/** Asks truelite whether the NAS answers. Never throws. */
export async function probeNas(fetchFn: typeof fetch = fetch): Promise<NasReach> {
	try {
		const res = await fetchFn('/api/nas', { cache: 'no-store', credentials: 'same-origin' });
		if (!res.ok) return 'unreachable';
		const body = (await res.json()) as { nas?: unknown };
		return body.nas === 'up' || body.nas === 'down' || body.nas === 'cert_mismatch'
			? body.nas
			: 'unreachable';
	} catch {
		return 'unreachable';
	}
}

/** The login page, waiting for the NAS, then back to `next`. */
export function restartLoginUrl(next: string): string {
	return `/login?restarting=1&next=${encodeURIComponent(next)}`;
}

export const CERT_CHANGED =
	"The NAS now uses another security certificate than the one truelite trusts. An admin must update truelite's TRUELITE_NAS_FINGERPRINT setting.";

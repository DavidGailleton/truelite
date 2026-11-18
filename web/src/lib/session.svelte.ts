// Who is logged in (`GET /api/session`) and logging out (`POST /api/logout`).
import { closeNas } from './nas.svelte';

/** What the server tells the web app about the session (`session_json` in the server). */
export interface SessionInfo {
	username: string;
	roles: string[];
	two_factor: boolean;
	link: 'up' | 'reconnecting' | 'ended';
}

/** The logged-in user; `null` until loaded, or when nobody is logged in. */
export const session = $state<{ user: SessionInfo | null }>({ user: null });

/** Outcome of {@link loadSession}: logged in, not logged in (401), or the server is unreachable. */
export type SessionState = 'in' | 'out' | 'unreachable';

let loading: Promise<SessionState> | null = null;

/**
 * Loads the session once; later calls share the first request. An unreachable server is not
 * remembered, so the next call asks again.
 */
export function loadSession(): Promise<SessionState> {
	loading ??= fetch('/api/session', { credentials: 'same-origin', cache: 'no-store' })
		.then(async (res): Promise<SessionState> => {
			if (res.status === 401) {
				session.user = null;
				return 'out';
			}
			if (!res.ok) throw new Error(`HTTP ${res.status}`);
			session.user = (await res.json()) as SessionInfo;
			return 'in';
		})
		.catch((): SessionState => {
			loading = null;
			return 'unreachable';
		});
	return loading;
}

/** Stores the user a login just returned, so the app does not ask the server again. */
export function setSession(user: SessionInfo): void {
	session.user = user;
	loading = Promise.resolve('in');
}

/** Forgets the session locally (it ended on the server); the next {@link loadSession} asks again. */
export function clearSession(): void {
	session.user = null;
	loading = null;
}

/**
 * Ends the session on the server (which logs out of the NAS) and stops the WebSocket. Succeeds
 * even if the server is unreachable: the local state is cleared either way.
 */
export async function logout(): Promise<void> {
	try {
		await fetch('/api/logout', { method: 'POST', credentials: 'same-origin' });
	} catch {
		// Unreachable server: the cookie expires on its own and the session ends server-side.
	}
	closeNas();
	clearSession();
}

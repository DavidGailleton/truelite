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

let loading: Promise<void> | null = null;

/** Loads the session once; later calls share the first request. A failed request can be retried. */
export function loadSession(): Promise<void> {
	loading ??= fetch('/api/session', { credentials: 'same-origin', cache: 'no-store' })
		.then(async (res) => {
			session.user = res.ok ? ((await res.json()) as SessionInfo) : null;
		})
		.catch(() => {
			loading = null;
		});
	return loading;
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
	session.user = null;
	loading = null;
}

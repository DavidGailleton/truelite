// Logging in: `POST /api/login` (user name + password), then `POST /api/login/otp` when the
// account asks for a one-time code. Framework-free so it can be unit-tested; the page is
// routes/login/+page.svelte.
import type { SessionInfo } from './session.svelte';

/** What the login page should do next. `error` is in plain words, safe to show. */
export type LoginResult =
	/** Logged in. */
	| { kind: 'done'; user: SessionInfo }
	/** The account needs a one-time code; `error` is set after a wrong code. */
	| { kind: 'otp'; error?: string }
	/** The server dropped the half-done login (expired, too many wrong codes, NAS error): start
	 * again with the password. */
	| { kind: 'restart'; error: string }
	/** Refused or failed; stay on the current step. */
	| { kind: 'error'; error: string };

type Fetch = typeof fetch;

const UNREACHABLE = 'Can’t reach truelite. Check your connection.';

/** Logs in with a user name and password. */
export function loginWithPassword(
	username: string,
	password: string,
	fetchFn: Fetch = fetch
): Promise<LoginResult> {
	return post('/api/login', { username, password }, fetchFn, false);
}

/**
 * Sends the one-time code of a login that {@link loginWithPassword} started. Spaces are removed:
 * authenticators show "123 456".
 */
export function loginWithCode(code: string, fetchFn: Fetch = fetch): Promise<LoginResult> {
	return post('/api/login/otp', { code: code.replace(/\s+/g, '') }, fetchFn, true);
}

/** Messages for the `reason` of a "log in again" answer. */
const RESTART: Record<string, string> = {
	expired: 'The code expired. Log in again.',
	too_many_codes: 'Too many wrong codes. Log in again.'
};

async function post(
	url: string,
	body: unknown,
	fetchFn: Fetch,
	codeStep: boolean
): Promise<LoginResult> {
	let res: Response;
	try {
		res = await fetchFn(url, {
			method: 'POST',
			// The server requires JSON: a cross-site form cannot send it without CORS.
			headers: { 'Content-Type': 'application/json' },
			credentials: 'same-origin',
			cache: 'no-store',
			body: JSON.stringify(body)
		});
	} catch {
		return { kind: 'error', error: UNREACHABLE };
	}

	let data: Record<string, unknown>;
	try {
		data = (await res.json()) as Record<string, unknown>;
	} catch {
		// Not our server (a proxy error page, say).
		return { kind: 'error', error: UNREACHABLE };
	}

	if (res.ok) {
		if (data.otp_required === true) return { kind: 'otp' };
		if (typeof data.username === 'string') return { kind: 'done', user: data as unknown as SessionInfo };
		return { kind: 'error', error: UNREACHABLE };
	}

	if (res.status === 429) return { kind: 'error', error: tooMany(res.headers.get('Retry-After')) };
	const message = typeof data.error === 'string' ? data.error : '';
	if (message === 'wrong code' && data.otp_required === true) {
		return { kind: 'otp', error: 'Wrong code. Try again.' };
	}
	if (message === 'log in again') {
		const reason = typeof data.reason === 'string' ? data.reason : '';
		return { kind: 'restart', error: RESTART[reason] ?? 'Log in again.' };
	}
	const error = message ? sentence(message) : `Login failed (error ${res.status}).`;
	// On the code step only a wrong code, a bad request and the rate limit keep the half-done
	// login (`otp()` in server/src/login.rs); any other error has ended it.
	if (codeStep && res.status !== 400) return { kind: 'restart', error: `${error} Log in again.` };
	return { kind: 'error', error };
}

function tooMany(retryAfter: string | null): string {
	const seconds = Number(retryAfter);
	if (!retryAfter || !Number.isFinite(seconds) || seconds <= 0) {
		return 'Too many attempts. Try again later.';
	}
	const minutes = Math.ceil(seconds / 60);
	return `Too many attempts. Try again in ${minutes === 1 ? '1 minute' : `${minutes} minutes`}.`;
}

/** "wrong user name or password" → "Wrong user name or password." */
function sentence(message: string): string {
	const text = message.charAt(0).toUpperCase() + message.slice(1);
	return /[.!?]$/.test(text) ? text : `${text}.`;
}

/**
 * Where to go after logging in: `raw` when it is a path on this site, else the dashboard. Refuses
 * `//host` and `/\host` (other sites for the browser) and the login page itself.
 */
export function safeNext(raw: string | null | undefined): string {
	if (!raw || !raw.startsWith('/') || raw.startsWith('//') || raw.startsWith('/\\')) return '/';
	let path: string;
	try {
		const url = new URL(raw, 'http://truelite.invalid');
		if (url.origin !== 'http://truelite.invalid') return '/';
		path = url.pathname;
		if (path === '/login' || path.startsWith('/login/')) return '/';
		return path + url.search + url.hash;
	} catch {
		return '/';
	}
}

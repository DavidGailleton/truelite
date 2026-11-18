import { describe, expect, it, vi } from 'vitest';
import { loginWithCode, loginWithPassword, safeNext } from './login';

const USER = { username: 'admin', roles: ['FULL_ADMIN'], two_factor: false, link: 'up' };

/** A fetch that answers once with `status` and a JSON (or raw text) body. */
function reply(status: number, body: unknown, headers: Record<string, string> = {}) {
	return vi.fn(
		async () =>
			new Response(typeof body === 'string' ? body : JSON.stringify(body), { status, headers })
	);
}

describe('loginWithPassword', () => {
	it('sends JSON and returns the user', async () => {
		const fetchFn = reply(200, USER);
		expect(await loginWithPassword('admin', 'secret', fetchFn)).toEqual({ kind: 'done', user: USER });
		const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
		expect(url).toBe('/api/login');
		expect(init.method).toBe('POST');
		expect((init.headers as Record<string, string>)['Content-Type']).toBe('application/json');
		expect(JSON.parse(init.body as string)).toEqual({ username: 'admin', password: 'secret' });
	});

	it('asks for a code', async () => {
		expect(await loginWithPassword('a', 'b', reply(200, { otp_required: true }))).toEqual({
			kind: 'otp'
		});
	});

	it('turns server errors into sentences', async () => {
		const r = await loginWithPassword('a', 'b', reply(401, { error: 'wrong user name or password' }));
		expect(r).toEqual({ kind: 'error', error: 'Wrong user name or password.' });
		const r2 = await loginWithPassword('a', 'b', reply(502, { error: 'cannot reach the NAS' }));
		expect(r2).toEqual({ kind: 'error', error: 'Cannot reach the NAS.' });
	});

	it('says how long to wait when rate limited', async () => {
		const r = await loginWithPassword(
			'a',
			'b',
			reply(429, { error: 'too many attempts, try again later' }, { 'Retry-After': '61' })
		);
		expect(r).toEqual({ kind: 'error', error: 'Too many attempts. Try again in 2 minutes.' });
		const r1 = await loginWithPassword('a', 'b', reply(429, {}, { 'Retry-After': '30' }));
		expect(r1).toEqual({ kind: 'error', error: 'Too many attempts. Try again in 1 minute.' });
		const r0 = await loginWithPassword('a', 'b', reply(429, {}));
		expect(r0).toEqual({ kind: 'error', error: 'Too many attempts. Try again later.' });
	});

	it('reports an unreachable server, never raw HTTP text', async () => {
		const down = vi.fn(async () => {
			throw new TypeError('Failed to fetch');
		});
		const unreachable = { kind: 'error', error: 'Can’t reach truelite. Check your connection.' };
		expect(await loginWithPassword('a', 'b', down)).toEqual(unreachable);
		expect(await loginWithPassword('a', 'b', reply(502, '<html>Bad Gateway</html>'))).toEqual(
			unreachable
		);
		expect(await loginWithPassword('a', 'b', reply(500, {}))).toEqual({
			kind: 'error',
			error: 'Login failed (error 500).'
		});
	});

	it('reports an unknown 200 answer as unexpected, not unreachable', async () => {
		expect(await loginWithPassword('a', 'b', reply(200, {}))).toEqual({
			kind: 'error',
			error: 'Unexpected answer from truelite. Try again.'
		});
	});
});

describe('loginWithCode', () => {
	it('sends the trimmed code and returns the user', async () => {
		const fetchFn = reply(200, USER);
		expect(await loginWithCode(' 123456 ', fetchFn)).toEqual({ kind: 'done', user: USER });
		const [url, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
		expect(url).toBe('/api/login/otp');
		expect(JSON.parse(init.body as string)).toEqual({ code: '123456' });
	});

	it('stays on the code step after a wrong code', async () => {
		const r = await loginWithCode('000000', reply(401, { error: 'wrong code', otp_required: true }));
		expect(r).toEqual({ kind: 'otp', error: 'Wrong code. Try again.' });
	});

	it('removes every space from the code', async () => {
		const fetchFn = reply(200, USER);
		await loginWithCode(' 123 456 ', fetchFn);
		const [, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
		expect(JSON.parse(init.body as string)).toEqual({ code: '123456' });
	});

	it.each([
		['expired', 'The code expired. Log in again.'],
		['too_many_codes', 'Too many wrong codes. Log in again.'],
		[undefined, 'Log in again.']
	])('starts again with the reason (%s)', async (reason, error) => {
		const r = await loginWithCode('000000', reply(401, { error: 'log in again', reason }));
		expect(r).toEqual({ kind: 'restart', error });
	});

	it.each([
		[504, 'the NAS did not answer in time', 'The NAS did not answer in time. Log in again.'],
		[502, 'cannot reach the NAS', 'Cannot reach the NAS. Log in again.'],
		[
			403,
			'this account is not allowed to use a web interface',
			'This account is not allowed to use a web interface. Log in again.'
		]
	])('starts again after a %i, which ended the login', async (status, message, error) => {
		const r = await loginWithCode('123456', reply(status, { error: message }));
		expect(r).toEqual({ kind: 'restart', error });
	});

	it('stays on the code step when the login is still waiting', async () => {
		expect(await loginWithCode('12', reply(400, { error: 'invalid request' }))).toEqual({
			kind: 'error',
			error: 'Invalid request.'
		});
		const limited = await loginWithCode('123456', reply(429, { error: 'too many attempts' }));
		expect(limited.kind).toBe('error');
	});
});

describe('safeNext', () => {
	it.each([
		['/apps', '/apps'],
		['/apps/plex?tab=logs#top', '/apps/plex?tab=logs#top'],
		['/', '/'],
		[null, '/'],
		[undefined, '/'],
		['', '/'],
		['//evil.example', '/'],
		['/\\evil.example', '/'],
		['https://evil.example/', '/'],
		['javascript:alert(1)', '/'],
		['apps', '/'],
		['/login', '/'],
		['/login?next=/apps', '/'],
		['/loginx', '/loginx']
	])('%s → %s', (raw, expected) => {
		expect(safeNext(raw)).toBe(expected);
	});
});

import { describe, expect, it } from 'vitest';
import {
	GUESS_GRACE_MS,
	MAX_WAIT_MS,
	backText,
	elapsedText,
	parseWait,
	pollDelay,
	probeNas,
	reachSignal,
	restartLoginUrl,
	restartText,
	restartTitle,
	serializeWait,
	statusSignal,
	stepWait,
	waitDeadline,
	type RestartWait
} from './restart';

const NOW = 1_800_000_000_000;
const MIN = 60_000;

describe('restart wording', () => {
	it('names the NAS when known', () => {
		const wait = { since: NOW, guess: false, down: false };
		expect(restartTitle({ ...wait, kind: 'reboot', host: 'nas' })).toBe('Restarting nas…');
		expect(restartTitle({ ...wait, kind: 'reboot', host: null })).toBe('Restarting the NAS…');
		expect(restartTitle({ ...wait, kind: 'shutdown', host: 'nas' })).toBe('Shutting down nas');
		expect(backText('nas')).toBe('nas is back. Log in again.');
		expect(backText(null)).toBe('The NAS is back. Log in again.');
	});

	it('says a shut-down NAS needs someone to power it on', () => {
		expect(restartText('shutdown')).toContain('powers it on');
		expect(restartText('reboot')).toContain('few minutes');
	});

	it('counts whole minutes', () => {
		expect(elapsedText(0)).toBe('less than a minute');
		expect(elapsedText(59_999)).toBe('less than a minute');
		expect(elapsedText(MIN)).toBe('1 min');
		expect(elapsedText(12.5 * MIN)).toBe('12 min');
		expect(elapsedText(-5)).toBe('less than a minute');
	});
});

describe('poll delay', () => {
	it('asks often during a restart, rarely after a shutdown or a very long restart', () => {
		expect(pollDelay('reboot', 0)).toBe(3000);
		expect(pollDelay('reboot', 14 * MIN)).toBe(3000);
		expect(pollDelay('reboot', 15 * MIN)).toBe(15000);
		expect(pollDelay('shutdown', 0)).toBe(15000);
	});
});

describe('stored wait', () => {
	const wait: RestartWait = {
		kind: 'reboot',
		host: 'nas',
		since: NOW - MIN,
		guess: false,
		down: false
	};

	it('round-trips', () => {
		expect(parseWait(serializeWait(wait), NOW)).toEqual(wait);
		expect(parseWait(serializeWait({ ...wait, host: null }), NOW)).toEqual({ ...wait, host: null });
		const guessed = { ...wait, guess: true, down: true };
		expect(parseWait(serializeWait(guessed), NOW)).toEqual(guessed);
	});

	it('loads entries from before guess and down as a sure wait not seen down', () => {
		const old = JSON.stringify({ kind: 'shutdown', host: null, since: NOW - MIN });
		expect(parseWait(old, NOW)).toEqual({
			kind: 'shutdown',
			host: null,
			since: NOW - MIN,
			guess: false,
			down: false
		});
		const odd = JSON.stringify({ ...wait, guess: 'yes', down: 1 });
		expect(parseWait(odd, NOW)).toEqual(wait);
	});

	it('drops missing, malformed, old and future waits', () => {
		expect(parseWait(null, NOW)).toBeNull();
		expect(parseWait('', NOW)).toBeNull();
		expect(parseWait('{', NOW)).toBeNull();
		expect(parseWait('42', NOW)).toBeNull();
		expect(parseWait(JSON.stringify({ ...wait, kind: 'halt' }), NOW)).toBeNull();
		expect(parseWait(JSON.stringify({ ...wait, host: 3 }), NOW)).toBeNull();
		expect(parseWait(JSON.stringify({ ...wait, since: 'x' }), NOW)).toBeNull();
		expect(parseWait(serializeWait({ ...wait, since: NOW - MAX_WAIT_MS - 1 }), NOW)).toBeNull();
		expect(parseWait(serializeWait({ ...wait, since: NOW + 2 * MIN }), NOW)).toBeNull();
	});
});

describe('wait signals', () => {
	it('reads only the NAS link from the connection status', () => {
		expect(statusSignal('nas-reconnecting')).toBe('down');
		expect(statusSignal('up')).toBe('up');
		expect(statusSignal('offline')).toBe('unknown');
		expect(statusSignal('connecting')).toBe('unknown');
		expect(statusSignal('logged-out')).toBe('unknown');
	});

	it('reads a changed certificate as the NAS answering', () => {
		expect(reachSignal('up')).toBe('up');
		expect(reachSignal('cert_mismatch')).toBe('up');
		expect(reachSignal('down')).toBe('down');
		expect(reachSignal('unreachable')).toBe('unknown');
		expect(reachSignal(null)).toBe('unknown');
	});
});

describe('stepWait', () => {
	const sure: RestartWait = { kind: 'reboot', host: 'nas', since: NOW, guess: false, down: false };
	const guess: RestartWait = { ...sure, guess: true };
	const down: RestartWait = { ...sure, down: true };

	it.each([
		['sure, up right away', sure, 'up', NOW + MIN, sure],
		['sure, up much later', sure, 'up', NOW + 30 * MIN, sure],
		['sure, unknown', sure, 'unknown', NOW + MIN, sure],
		['sure, seen down', sure, 'down', NOW + MIN, down],
		['down, still down', down, 'down', NOW + MIN, down],
		['down, back up', down, 'up', NOW + MIN, { end: 'back' }],
		['down, unknown', down, 'unknown', NOW + MIN, down],
		['guess, up within the grace', guess, 'up', NOW + GUESS_GRACE_MS - 1, guess],
		['guess, up after the grace', guess, 'up', NOW + GUESS_GRACE_MS, { end: 'not-down' }],
		['guess, unknown after the grace', guess, 'unknown', NOW + GUESS_GRACE_MS, guess],
		['guess, seen down', guess, 'down', NOW + MIN, { ...guess, down: true }],
		['guess seen down, back up', { ...guess, down: true }, 'up', NOW + MIN, { end: 'back' }],
		['too old', down, 'down', NOW + MAX_WAIT_MS, { end: 'expired' }],
		['too old, unknown', sure, 'unknown', NOW + MAX_WAIT_MS + 1, { end: 'expired' }]
	] as const)('%s', (_, wait, signal, now, expected) => {
		expect(stepWait(wait, signal, now)).toEqual(expected);
	});

	it('keeps the same object when nothing changes', () => {
		expect(stepWait(down, 'down', NOW + MIN)).toBe(down);
		expect(stepWait(sure, 'up', NOW + MIN)).toBe(sure);
	});

	it('gives the time to look again', () => {
		expect(waitDeadline(guess)).toBe(NOW + GUESS_GRACE_MS);
		expect(waitDeadline({ ...guess, down: true })).toBe(NOW + MAX_WAIT_MS);
		expect(waitDeadline(sure)).toBe(NOW + MAX_WAIT_MS);
	});
});

describe('probeNas', () => {
	const answer = (status: number, body: unknown) =>
		(() => Promise.resolve(new Response(JSON.stringify(body), { status }))) as typeof fetch;

	it('passes on what the server says', async () => {
		expect(await probeNas(answer(200, { nas: 'up' }))).toBe('up');
		expect(await probeNas(answer(200, { nas: 'down' }))).toBe('down');
		expect(await probeNas(answer(200, { nas: 'cert_mismatch' }))).toBe('cert_mismatch');
	});

	it('is unreachable on an error, a strange answer or no answer', async () => {
		expect(await probeNas(answer(503, { error: 'no NAS is configured' }))).toBe('unreachable');
		expect(await probeNas(answer(200, { nas: 'maybe' }))).toBe('unreachable');
		expect(await probeNas((() => Promise.reject(new TypeError('offline'))) as typeof fetch)).toBe(
			'unreachable'
		);
	});
});

it('sends the login page back to where the user was', () => {
	expect(restartLoginUrl('/system?x=1#power')).toBe(
		'/login?restarting=1&next=%2Fsystem%3Fx%3D1%23power'
	);
});

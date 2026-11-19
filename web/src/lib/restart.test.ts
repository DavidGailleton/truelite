import { describe, expect, it } from 'vitest';
import {
	MAX_WAIT_MS,
	backText,
	elapsedText,
	parseWait,
	pollDelay,
	probeNas,
	restartLoginUrl,
	restartText,
	restartTitle,
	serializeWait,
	type RestartWait
} from './restart';

const NOW = 1_800_000_000_000;
const MIN = 60_000;

describe('restart wording', () => {
	it('names the NAS when known', () => {
		expect(restartTitle({ kind: 'reboot', host: 'nas', since: NOW })).toBe('Restarting nas…');
		expect(restartTitle({ kind: 'reboot', host: null, since: NOW })).toBe('Restarting the NAS…');
		expect(restartTitle({ kind: 'shutdown', host: 'nas', since: NOW })).toBe('Shutting down nas');
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
	const wait: RestartWait = { kind: 'reboot', host: 'nas', since: NOW - MIN };

	it('round-trips', () => {
		expect(parseWait(serializeWait(wait), NOW)).toEqual(wait);
		expect(parseWait(serializeWait({ ...wait, host: null }), NOW)).toEqual({ ...wait, host: null });
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

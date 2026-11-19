import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { NasError, type CallOptions } from './ws';

interface Pending {
	method: string;
	params: unknown[];
	options: CallOptions;
	resolve(v: unknown): void;
	reject(e: unknown): void;
}
const pending: Pending[] = [];

vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({
		call: (method: string, params: unknown[], options: CallOptions = {}) =>
			new Promise((resolve, reject) => pending.push({ method, params, options, resolve, reject }))
	})
}));

const { dismissPower, power, powerBusy, startPower } = await import('./power.svelte');
const { closeNas } = await import('./nas.svelte');
const { endRestartWait, restart } = await import('./restart.svelte');
const { POWER_TEXT } = await import('./power');
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
	pending.length = 0;
});
afterEach(() => {
	closeNas();
	endRestartWait();
});

describe('power runs', () => {
	it('restart once, with no timeout, and go down on the reply', async () => {
		startPower('reboot');
		startPower('reboot');
		expect(pending).toHaveLength(1);
		expect(pending[0]).toMatchObject({ method: 'system.reboot', params: [] });
		expect(pending[0].options.timeout).toBeNull();
		expect(power.run).toEqual({ phase: 'running', action: 'reboot' });
		pending[0].resolve(null);
		await settle();
		expect(power.run).toEqual({ phase: 'going-down', action: 'reboot' });
		expect(powerBusy()).toBe(true);
	});

	it('go down when the link drops before the reply', async () => {
		startPower('shutdown');
		pending[0].reject(new NasError({ code: 'reconnecting', message: 'x' }));
		await settle();
		expect(power.run).toEqual({ phase: 'going-down', action: 'shutdown' });
		expect(restart.wait).toMatchObject({ kind: 'shutdown', guess: false, down: true });
	});

	it('wait for sure, not yet seen down, on a reply', async () => {
		startPower('reboot');
		pending[0].resolve(null);
		await settle();
		expect(restart.wait).toMatchObject({ kind: 'reboot', guess: false, down: false });
	});

	it('only guess when the browser link drops', async () => {
		startPower('reboot');
		pending[0].reject(new NasError({ code: 'disconnected', message: 'x' }));
		await settle();
		expect(power.run).toEqual({ phase: 'going-down', action: 'reboot' });
		expect(restart.wait).toMatchObject({ kind: 'reboot', guess: true, down: false });
	});

	it('fail with a plain note when the NAS never went down', async () => {
		startPower('reboot');
		pending[0].reject(new NasError({ code: 'disconnected', message: 'x' }));
		await settle();
		endRestartWait('not-down');
		expect(power.run).toEqual({ phase: 'failed', message: POWER_TEXT.reboot.notDown });
		expect(powerBusy()).toBe(false);
	});

	it('are over once the NAS is back', async () => {
		startPower('shutdown');
		pending[0].resolve(null);
		await settle();
		endRestartWait('back');
		expect(power.run).toBeNull();
	});

	it('do not wait when the session ends first', async () => {
		startPower('reboot');
		pending[0].reject(new NasError({ code: 'logged_out', message: 'x' }));
		await settle();
		expect(power.run?.phase).toBe('failed');
		expect(restart.wait).toBeNull();
	});

	it('show a failure until dismissed', async () => {
		startPower('reboot');
		pending[0].reject(new NasError({ code: 'nas', message: 'nope', errname: 'EACCES' }));
		await settle();
		expect(power.run?.phase).toBe('failed');
		expect(powerBusy()).toBe(false);
		dismissPower();
		expect(power.run).toBeNull();
	});

	it('are forgotten on logout, late replies ignored', async () => {
		startPower('reboot');
		closeNas();
		expect(power.run).toBeNull();
		pending[0].resolve(null);
		await settle();
		expect(power.run).toBeNull();
	});
});

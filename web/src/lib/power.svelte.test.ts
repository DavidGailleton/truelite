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
const settle = () => new Promise((r) => setTimeout(r, 0));

beforeEach(() => {
	pending.length = 0;
});
afterEach(() => closeNas());

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

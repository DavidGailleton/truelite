import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { UpdateStatus } from './update';
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

const { dismissUpdateRun, installUpdate, resumeUpdate, updateRun } =
	await import('./update-run.svelte');
const { update } = await import('./update.svelte');
const { startPower } = await import('./power.svelte');
const { closeNas } = await import('./nas.svelte');
const { endRestartWait } = await import('./restart.svelte');
const settle = () => new Promise((r) => setTimeout(r, 0));

const offer = (newVersion: string): UpdateStatus => ({
	kind: 'available',
	train: 'TrueNAS-25.10',
	newVersion,
	notes: null,
	notesUrl: null,
	error: null
});

const warn = () =>
	pending.at(-1)!.reject(new NasError({ code: 'nas', message: 'Careful.', errname: 'EAGAIN' }));

beforeEach(() => {
	pending.length = 0;
});
afterEach(() => {
	closeNas();
	endRestartWait();
});

describe('update runs', () => {
	it('keep the confirmed offer with a pre-check warning', async () => {
		const status = offer('25.10.1');
		update.status = status;
		installUpdate(status, { reboot: true });
		expect(pending).toHaveLength(1);
		expect(pending[0]).toMatchObject({
			method: 'update.run',
			params: [{ version: '25.10.1', reboot: true, resume: false }]
		});
		warn();
		await settle();
		expect(updateRun.run).toEqual({ phase: 'warning', status, reboot: true, message: 'Careful.' });
	});

	it('resume the confirmed version when the offer did not change', async () => {
		update.status = offer('25.10.1');
		installUpdate(offer('25.10.1'), { reboot: false });
		warn();
		await settle();
		resumeUpdate();
		expect(pending).toHaveLength(2);
		expect(pending[1].params).toEqual([
			{ train: 'TrueNAS-25.10', version: '25.10.1', reboot: false, resume: true }
		]);
		expect(updateRun.run?.phase).toBe('running');
	});

	it('drop the warning when a newer version is on offer, installing nothing', async () => {
		update.status = offer('25.10.1');
		installUpdate(offer('25.10.1'), { reboot: true });
		warn();
		await settle();
		update.status = offer('25.10.2');
		resumeUpdate();
		expect(pending).toHaveLength(1);
		expect(updateRun.run).toBeNull();
	});

	it('drop the warning when nothing is on offer any more', async () => {
		update.status = offer('25.10.1');
		installUpdate(offer('25.10.1'), { reboot: true });
		warn();
		await settle();
		update.status = { ...offer('25.10.1'), kind: 'restart', newVersion: null };
		resumeUpdate();
		expect(pending).toHaveLength(1);
		expect(updateRun.run).toBeNull();
	});

	it('start nothing while running, warning or restarting', async () => {
		const status = offer('25.10.1');
		update.status = status;
		installUpdate(status, { reboot: true });
		installUpdate(status, { reboot: true });
		expect(pending).toHaveLength(1);

		warn();
		await settle();
		installUpdate(status, { reboot: true });
		expect(pending).toHaveLength(1);

		dismissUpdateRun();
		installUpdate(status, { reboot: true });
		expect(pending).toHaveLength(2);
		pending[1].resolve(null);
		await settle();
		expect(updateRun.run).toEqual({ phase: 'restarting' });
		installUpdate(status, { reboot: true });
		expect(pending).toHaveLength(2);
	});

	it('start nothing while the NAS restarts or shuts down', async () => {
		const status = offer('25.10.1');
		update.status = status;
		startPower('reboot');
		installUpdate(status, { reboot: true });
		expect(pending.map((p) => p.method)).toEqual(['system.reboot']);
		pending[0].resolve(null);
		await settle();
		installUpdate(status, { reboot: true });
		expect(pending).toHaveLength(1);
		expect(updateRun.run).toBeNull();
	});

	it('resume nothing while the NAS goes down', async () => {
		const status = offer('25.10.1');
		update.status = status;
		installUpdate(status, { reboot: true });
		warn();
		await settle();
		startPower('reboot');
		resumeUpdate();
		expect(pending.map((p) => p.method)).toEqual(['update.run', 'system.reboot']);
		expect(updateRun.run?.phase).toBe('warning');
	});
});

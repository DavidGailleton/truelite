import { describe, expect, it, vi } from 'vitest';

// Every call hangs: runs stay in flight.
vi.mock('./nas.svelte', async (actual) => ({
	...(await actual<typeof import('./nas.svelte')>()),
	nas: () => ({ call: () => new Promise(() => {}) })
}));

const { actionRun, isRunning, noteState, runAction } = await import('./app-actions.svelte');
const { closeNas } = await import('./nas.svelte');

const call = { method: 'app.start', params: ['plex'] };

describe('action runs', () => {
	it('are forgotten when the account logs out', () => {
		runAction('plex', 'start', call, 'stopped');
		expect(isRunning('plex')).toBe(true);
		closeNas();
		expect(actionRun('plex')).toBeUndefined();
	});

	it('learn once the app has left the state it was in', () => {
		runAction('plex', 'start', call, 'crashed');
		noteState('plex', 'crashed');
		expect(actionRun('plex')?.left).toBe(false);
		noteState('plex', 'deploying');
		expect(actionRun('plex')?.left).toBe(true);
		closeNas();
	});
});

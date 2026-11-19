import { afterEach, describe, expect, it } from 'vitest';
import { RESTART_KEY, type WaitEnd } from './restart';
import {
	applyWaitSignal,
	beginRestartWait,
	endRestartWait,
	markWaitDown,
	onRestartEnd,
	restart
} from './restart.svelte';

const ended: WaitEnd[] = [];
onRestartEnd((reason) => ended.push(reason));

afterEach(() => {
	endRestartWait();
	ended.length = 0;
});

describe('restart wait', () => {
	it('stores the NAS seen down, then ends once it is back', () => {
		beginRestartWait('reboot', 'nas');
		expect(applyWaitSignal('up')).toBeNull();
		expect(restart.wait).toMatchObject({ guess: false, down: false });
		markWaitDown();
		expect(restart.wait).toMatchObject({ down: true });
		expect(JSON.parse(sessionStorage.getItem(RESTART_KEY) ?? '{}')).toMatchObject({ down: true });
		expect(applyWaitSignal('up')).toBe('back');
		expect(restart.wait).toBeNull();
		expect(sessionStorage.getItem(RESTART_KEY)).toBeNull();
		expect(ended).toEqual(['back']);
	});

	it('does nothing without a wait', () => {
		expect(applyWaitSignal('up')).toBeNull();
		markWaitDown();
		expect(restart.wait).toBeNull();
		expect(ended).toEqual([]);
	});

	it('says the user stopped by default', () => {
		beginRestartWait('shutdown', null, { guess: true });
		endRestartWait();
		expect(ended).toEqual(['stopped']);
	});
});

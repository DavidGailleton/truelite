import { describe, expect, it } from 'vitest';
import { canPower, powerCall, powerOutcome } from './power';

describe('power calls', () => {
	it('send no params: the server sets the reason', () => {
		expect(powerCall('reboot')).toEqual({ method: 'system.reboot', params: [] });
		expect(powerCall('shutdown')).toEqual({ method: 'system.shutdown', params: [] });
	});

	it('need FULL_ADMIN', () => {
		expect(canPower(['FULL_ADMIN'])).toBe(true);
		expect(canPower(['SYSTEM_UPDATE_WRITE', 'APPS_WRITE', 'POOL_WRITE'])).toBe(false);
		expect(canPower([])).toBe(false);
		expect(canPower(undefined)).toBe(false);
	});
});

describe('power outcome', () => {
	it('is going down on a reply or a dropped link', () => {
		expect(powerOutcome('reboot', null)).toEqual({ kind: 'going-down' });
		for (const code of ['disconnected', 'reconnecting', 'logged_out'] as const) {
			expect(powerOutcome('shutdown', { code, message: 'x' })).toEqual({ kind: 'going-down' });
		}
	});

	it('explains a missing role', () => {
		const out = powerOutcome('reboot', {
			code: 'nas',
			message: 'Not authorized',
			errname: 'EACCES'
		});
		expect(out.kind).toBe('error');
		expect(out.kind === 'error' && out.message).toMatch(/full admin/);
	});

	it('passes on other NAS errors', () => {
		expect(powerOutcome('shutdown', { code: 'nas', message: 'boom' })).toEqual({
			kind: 'error',
			message: "Couldn't shut down the NAS: boom"
		});
	});
});

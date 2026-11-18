import { describe, expect, it } from 'vitest';
import { ago } from './time';

const T0 = Date.UTC(2026, 10, 18, 12, 0, 0);

describe('ago', () => {
	it('reads like a person would say it', () => {
		const at = (s: number) => ago(new Date(T0), T0 + s * 1000);
		expect(at(-5)).toBe('just now');
		expect(at(59)).toBe('just now');
		expect(at(60)).toBe('1 min ago');
		expect(at(3599)).toBe('59 min ago');
		expect(at(3600)).toBe('1 h ago');
		expect(at(86_399)).toBe('23 h ago');
		expect(at(86_400)).toBe('yesterday');
		expect(at(2 * 86_400)).toBe('2 days ago');
		expect(at(12 * 86_400)).toBe('12 days ago');
	});
});

import { describe, expect, it } from 'vitest';
import { sparkPath } from './spark';

const opts = { max: 100, width: 60, height: 20, capacity: 7 };

describe('sparkPath', () => {
	it('draws a full window from left to right, scaled to max', () => {
		const { line, area } = sparkPath([0, 50, 100, 50, 0, 50, 100], opts);
		expect(line).toBe('M0 20L10 10L20 0L30 10L40 20L50 10L60 0');
		expect(area).toBe('M0 20L0 20L10 10L20 0L30 10L40 20L50 10L60 0L60 20Z');
	});

	it('puts the newest value at the right edge while the window fills', () => {
		expect(sparkPath([0, 100], opts).line).toBe('M50 20L60 0');
	});

	it('clips values outside 0..max', () => {
		expect(sparkPath([-10, 250], opts).line).toBe('M50 20L60 0');
	});

	it('breaks the line at a gap', () => {
		const { line, area } = sparkPath([0, 0, null, 100, 100], opts);
		expect(line).toBe('M20 20L30 20M50 0L60 0');
		expect(area.match(/Z/g)).toHaveLength(2);
		expect(sparkPath([0, NaN, 0], opts).line).toBe('M40 20h0M60 20h0');
	});

	it('draws a lone point as a dot and no area', () => {
		expect(sparkPath([50], opts)).toEqual({ line: 'M60 10h0', area: '' });
	});

	it('has nothing to draw without values', () => {
		expect(sparkPath([], opts)).toEqual({ line: '', area: '' });
	});

	it('does not divide by a zero max', () => {
		expect(sparkPath([0, 0], { ...opts, max: 0 }).line).toBe('M50 20L60 20');
	});
});

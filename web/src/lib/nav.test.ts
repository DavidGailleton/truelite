import { describe, expect, it } from 'vitest';
import { NAV_ITEMS, isActive } from './nav';
import { ICONS } from './icons';

describe('isActive', () => {
	it('matches the dashboard only on /', () => {
		expect(isActive('/', '/')).toBe(true);
		expect(isActive('/apps', '/')).toBe(false);
	});

	it('matches a section and the pages below it', () => {
		expect(isActive('/apps', '/apps')).toBe(true);
		expect(isActive('/apps/', '/apps')).toBe(true);
		expect(isActive('/apps/plex/logs', '/apps')).toBe(true);
	});

	it('does not match a longer name or another section', () => {
		expect(isActive('/appsx', '/apps')).toBe(false);
		expect(isActive('/storage', '/apps')).toBe(false);
		expect(isActive('/', '/apps')).toBe(false);
	});
});

describe('NAV_ITEMS', () => {
	it('has unique paths and only known icons', () => {
		const hrefs = NAV_ITEMS.map((i) => i.href);
		expect(new Set(hrefs).size).toBe(hrefs.length);
		for (const item of NAV_ITEMS) expect(ICONS[item.icon]).toBeDefined();
	});

	it('marks exactly one item active on every section page', () => {
		for (const { href } of NAV_ITEMS) {
			const active = NAV_ITEMS.filter((i) => isActive(href, i.href));
			expect(active.map((i) => i.href)).toEqual([href]);
		}
	});
});

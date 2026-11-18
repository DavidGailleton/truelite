import { describe, expect, it } from 'vitest';
import { applyTheme, parseTheme, readTheme, THEME_KEY, themeColorMedia, writeTheme } from './theme';

function memoryStorage(initial: Record<string, string> = {}) {
	const data = new Map(Object.entries(initial));
	return {
		data,
		getItem: (k: string) => data.get(k) ?? null,
		setItem: (k: string, v: string) => void data.set(k, v),
		removeItem: (k: string) => void data.delete(k)
	};
}

const blocked = {
	getItem: (): string | null => {
		throw new Error('SecurityError');
	},
	setItem: () => {
		throw new Error('SecurityError');
	},
	removeItem: () => {
		throw new Error('SecurityError');
	}
};

describe('parseTheme', () => {
	it('keeps light and dark, anything else is system', () => {
		expect(parseTheme('light')).toBe('light');
		expect(parseTheme('dark')).toBe('dark');
		expect(parseTheme(null)).toBe('system');
		expect(parseTheme('Dark')).toBe('system');
		expect(parseTheme('system')).toBe('system');
	});
});

describe('readTheme / writeTheme', () => {
	it('round-trips an explicit choice under the shared key', () => {
		const s = memoryStorage();
		writeTheme(s, 'dark');
		expect(s.data.get(THEME_KEY)).toBe('dark');
		expect(readTheme(s)).toBe('dark');
	});

	it('removes the key for system', () => {
		const s = memoryStorage({ [THEME_KEY]: 'light' });
		writeTheme(s, 'system');
		expect(s.data.has(THEME_KEY)).toBe(false);
		expect(readTheme(s)).toBe('system');
	});

	it('falls back to system when storage is missing or blocked', () => {
		expect(readTheme(null)).toBe('system');
		expect(readTheme(blocked)).toBe('system');
		expect(() => writeTheme(blocked, 'dark')).not.toThrow();
		expect(() => writeTheme(null, 'dark')).not.toThrow();
	});
});

describe('applyTheme', () => {
	it('sets data-theme for an explicit choice and removes it for system', () => {
		const root = { dataset: {} as Record<string, string | undefined> };
		applyTheme(root, 'dark');
		expect(root.dataset.theme).toBe('dark');
		applyTheme(root, 'light');
		expect(root.dataset.theme).toBe('light');
		applyTheme(root, 'system');
		expect('theme' in root.dataset).toBe(false);
	});
});

describe('themeColorMedia', () => {
	it('follows the OS for system and forces one colour otherwise', () => {
		expect(themeColorMedia('system')).toEqual([
			'(prefers-color-scheme: light)',
			'(prefers-color-scheme: dark)'
		]);
		expect(themeColorMedia('light')).toEqual(['all', 'not all']);
		expect(themeColorMedia('dark')).toEqual(['not all', 'all']);
	});
});

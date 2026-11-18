// Light/dark theme choice, remembered per browser. "system" follows `prefers-color-scheme`;
// "light" and "dark" set `data-theme` on <html>, which tokens.css reads. The inline script in
// app.html applies a stored choice before first paint; keep its key and values in sync with these.

export type Theme = 'system' | 'light' | 'dark';

export const THEMES: readonly Theme[] = ['system', 'light', 'dark'];

/** localStorage key; app.html's inline script reads it too. */
export const THEME_KEY = 'truelite-theme';

type ThemeStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

/** A stored value as a theme: anything but "light" or "dark" means "system". */
export function parseTheme(value: string | null | undefined): Theme {
	return value === 'light' || value === 'dark' ? value : 'system';
}

/** The stored choice; "system" when storage is missing or blocked (private mode). */
export function readTheme(storage: ThemeStorage | null): Theme {
	try {
		return parseTheme(storage?.getItem(THEME_KEY));
	} catch {
		return 'system';
	}
}

/** Stores the choice; "system" removes the key. Blocked storage: the choice lasts for this page. */
export function writeTheme(storage: ThemeStorage | null, theme: Theme): void {
	try {
		if (theme === 'system') storage?.removeItem(THEME_KEY);
		else storage?.setItem(THEME_KEY, theme);
	} catch {
		// Private mode or blocked storage.
	}
}

/** Sets `data-theme` on the root element, or removes it for "system". */
export function applyTheme(root: { dataset: Record<string, string | undefined> }, theme: Theme) {
	if (theme === 'system') delete root.dataset.theme;
	else root.dataset.theme = theme;
}

const LIGHT_QUERY = '(prefers-color-scheme: light)';
const DARK_QUERY = '(prefers-color-scheme: dark)';

/**
 * `media` for app.html's two `<meta name="theme-color">` tags (light first, then dark), so the
 * browser bar matches an explicit choice instead of the OS.
 */
export function themeColorMedia(theme: Theme): [light: string, dark: string] {
	if (theme === 'light') return ['all', 'not all'];
	if (theme === 'dark') return ['not all', 'all'];
	return [LIGHT_QUERY, DARK_QUERY];
}

/** The theme on screen: "system" resolves through the OS preference. */
export function resolveTheme(theme: Theme, prefersDark: boolean): 'light' | 'dark' {
	if (theme === 'system') return prefersDark ? 'dark' : 'light';
	return theme;
}

/** What the one-click sun/moon button switches to: the opposite of what is on screen. */
export function toggledTheme(theme: Theme, prefersDark: boolean): 'light' | 'dark' {
	return resolveTheme(theme, prefersDark) === 'dark' ? 'light' : 'dark';
}

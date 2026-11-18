// The theme choice as reactive state, applied to <html> and the theme-color metas.
import { applyTheme, parseTheme, readTheme, THEME_KEY, themeColorMedia, writeTheme } from './theme';
import type { Theme } from './theme';

function storage(): Storage | null {
	try {
		return localStorage;
	} catch {
		return null;
	}
}

/** The current choice; the inline script in app.html already applied it before first paint. */
export const theme = $state<{ value: Theme }>({ value: readTheme(storage()) });

function apply(value: Theme) {
	applyTheme(document.documentElement, value);
	const media = themeColorMedia(value);
	document.querySelectorAll('meta[name="theme-color"]').forEach((meta, i) => {
		if (i < media.length) meta.setAttribute('media', media[i]);
	});
}

// The script in app.html only sets data-theme; match the browser bar too.
apply(theme.value);

/** Applies and remembers a choice. */
export function setTheme(value: Theme) {
	writeTheme(storage(), value);
	theme.value = value;
	apply(value);
}

/** Follows a choice made in another tab (`storage` events fire only in the other tabs). */
export function onThemeStorage(e: StorageEvent) {
	if (e.key !== THEME_KEY && e.key !== null) return;
	const value = parseTheme(e.newValue);
	theme.value = value;
	apply(value);
}

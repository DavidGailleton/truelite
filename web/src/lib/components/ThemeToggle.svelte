<script lang="ts">
	// One-click light/dark switch, fixed in the top-right corner of every page (rendered once by
	// AppShell and by /login). "System" stays in the Account menu.
	import { MediaQuery } from 'svelte/reactivity';
	import { resolveTheme, toggledTheme } from '#lib/theme.js';
	import { setTheme, theme } from '#lib/theme.svelte.js';
	import Icon from './Icon.svelte';

	const prefersDark = new MediaQuery('(prefers-color-scheme: dark)');

	const dark = $derived(resolveTheme(theme.value, prefersDark.current) === 'dark');
	const label = $derived(dark ? 'Switch to light theme' : 'Switch to dark theme');
</script>

<button
	class="theme-toggle"
	type="button"
	aria-label={label}
	title={label}
	onclick={() => setTheme(toggledTheme(theme.value, prefersDark.current))}
>
	<Icon name={dark ? 'sun' : 'moon'} />
</button>

<style>
	.theme-toggle {
		position: fixed;
		top: max(var(--space-3), env(safe-area-inset-top));
		right: max(var(--space-3), env(safe-area-inset-right));
		z-index: 25; /* over the page and the reconnect banner (z 10); the nav (z 20) never reaches this corner */
		display: grid;
		place-items: center;
		width: var(--corner-size);
		height: var(--corner-size);
		padding: 0;
		border: 0;
		border-radius: var(--radius-lg);
		background: var(--container);
		box-shadow: var(--shadow-sm);
		color: var(--text-secondary);
		cursor: pointer;
		transition:
			background-color var(--duration-fast),
			color var(--duration-fast);
	}
	.theme-toggle:hover {
		background: var(--container-hover);
		color: var(--text-primary);
	}
</style>

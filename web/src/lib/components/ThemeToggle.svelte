<script lang="ts">
	// One-click light/dark switch. "System" stays in the Account menu.
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
		display: grid;
		flex: none;
		place-items: center;
		width: 2.5rem;
		height: 2.5rem;
		padding: 0;
		border: 0;
		border-radius: var(--radius-lg);
		background: transparent;
		color: var(--text-secondary);
		cursor: pointer;
		transition:
			background-color var(--duration-fast),
			color var(--duration-fast);
	}
	.theme-toggle:hover {
		background: var(--container-inset-hover);
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.theme-toggle {
			width: var(--touch);
			height: var(--touch);
		}
	}
</style>

<script lang="ts">
	import '#lib/styles/tokens.css';
	import '#lib/styles/base.css';
	import favicon from '#lib/assets/favicon.svg';
	import { onThemeStorage } from '#lib/theme.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<!-- A theme picked in another tab applies here too (login page included). -->
<svelte:window onstorage={onThemeStorage} />

<svelte:head>
	<link rel="icon" href={favicon} type="image/svg+xml" />
	<title>truelite</title>
</svelte:head>

{@render children()}

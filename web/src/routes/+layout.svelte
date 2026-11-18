<script lang="ts">
	import '#lib/styles/tokens.css';
	import favicon from '#lib/assets/favicon.svg';
	import ConnectionBanner from '#lib/components/ConnectionBanner.svelte';
	import { nas } from '#lib/nas.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	// Open the connection to truelite as soon as the app starts.
	$effect(() => {
		nas();
	});
</script>

<svelte:head>
	<link rel="icon" href={favicon} />
	<title>truelite</title>
</svelte:head>

<ConnectionBanner />
{@render children()}

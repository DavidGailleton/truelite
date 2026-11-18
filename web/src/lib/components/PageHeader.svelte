<script lang="ts">
	import type { Snippet } from 'svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	let {
		title,
		subtitle,
		loading = false,
		actions
	}: {
		title: string;
		subtitle?: string;
		/** Hold the subtitle's place with a skeleton until it arrives (no layout jump). */
		loading?: boolean;
		actions?: Snippet;
	} = $props();
</script>

<svelte:head>
	<title>{title} · truelite</title>
</svelte:head>

<header class="page-header">
	<div class="text">
		<h1>{title}</h1>
		{#if subtitle}
			<p>{subtitle}</p>
		{:else if loading}
			<p aria-hidden="true"><span class="skeleton line"></span></p>
		{/if}
	</div>
	{#if actions}<div class="actions">{@render actions()}</div>{/if}
	<div class="toggle"><ThemeToggle /></div>
</header>

<style>
	.page-header {
		display: flex;
		flex-wrap: wrap;
		align-items: flex-end;
		justify-content: space-between;
		gap: var(--space-4);
		margin-bottom: var(--space-6);
	}
	/* The title takes the free width and wraps, so the theme button stays top right at 360 px. */
	.text {
		flex: 1 1 0;
		min-width: 0;
	}
	.toggle {
		align-self: flex-start;
		margin: calc(-1 * var(--space-1)) calc(-1 * var(--space-2)) 0 0;
	}
	h1 {
		font-size: var(--text-xl);
		line-height: 1.25;
		overflow-wrap: anywhere;
	}
	p {
		margin: var(--space-1) 0 0;
		color: var(--text-secondary);
	}
	.skeleton.line {
		width: 12rem;
		height: 1lh;
	}
	.actions {
		display: flex;
		gap: var(--space-2);
	}
	@media (min-width: 1024px) {
		h1 {
			font-size: var(--text-3xl);
		}
	}
</style>

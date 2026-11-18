<script lang="ts">
	import type { Snippet } from 'svelte';

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
	.text {
		min-width: 0;
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

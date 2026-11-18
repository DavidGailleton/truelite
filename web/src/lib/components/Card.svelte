<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { IconName } from '#lib/icons.js';
	import Icon from './Icon.svelte';

	let {
		title,
		icon,
		control,
		children
	}: {
		title: string;
		icon?: IconName;
		/** Small control on the right of the header (a select, an info button). */
		control?: Snippet;
		children: Snippet;
	} = $props();

	const id = $props.id();
</script>

<section class="card" aria-labelledby="{id}-title">
	<header>
		{#if icon}<Icon name={icon} size={16} />{/if}
		<h2 id="{id}-title">{title}</h2>
		{#if control}<div class="control">{@render control()}</div>{/if}
	</header>
	{@render children()}
</section>

<style>
	.card {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		min-width: 0;
		padding: var(--space-5);
		border-radius: var(--radius-xl);
		background: var(--container);
		box-shadow: var(--shadow-border-xs);
	}
	header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 1.75rem;
		color: var(--text-secondary);
	}
	h2 {
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		color: var(--text-primary);
	}
	.control {
		margin-left: auto;
	}
</style>

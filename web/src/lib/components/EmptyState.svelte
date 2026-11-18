<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { IconName } from '#lib/icons.js';
	import Icon from './Icon.svelte';

	let {
		icon,
		title,
		text,
		action
	}: { icon: IconName; title: string; text: string; action?: Snippet } = $props();
</script>

<section class="empty">
	<span class="badge"><Icon name={icon} size={24} /></span>
	<h2>{title}</h2>
	<p>{text}</p>
	{#if action}{@render action()}{/if}
</section>

<style>
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-10) var(--space-5);
		border-radius: var(--radius-xl);
		background: var(--container);
		box-shadow: var(--shadow-border-xs);
		text-align: center;
	}
	.badge {
		display: grid;
		place-items: center;
		width: 3rem;
		height: 3rem;
		margin-bottom: var(--space-2);
		border-radius: var(--radius-full);
		background: var(--container-inset);
		color: var(--text-secondary);
	}
	h2 {
		font-size: var(--text-base);
	}
	p {
		max-width: 28rem;
		margin: 0;
		color: var(--text-secondary);
	}
</style>

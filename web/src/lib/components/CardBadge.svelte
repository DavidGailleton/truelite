<script lang="ts">
	// Header pill saying why a card's numbers aren't moving: "Paused" while the link is down,
	// "Not live" after updates stopped. Renders nothing while live.
	import { badgeLabel, type CardView } from '#lib/card-state.js';

	let { view }: { view: CardView } = $props();

	const label = $derived(badgeLabel(view));
</script>

{#if label}<span class="badge">{label}</span>{/if}

<style>
	.badge {
		padding: 0 var(--space-2);
		border-radius: var(--radius-full);
		background: var(--surface-inset);
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
</style>

<script lang="ts">
	// The error side of a dashboard card: a missing role, a first load that failed, or updates
	// that stopped after data arrived (shown above the dimmed data). Renders nothing otherwise.
	import type { CardView } from '#lib/card-state.js';

	let {
		view,
		what,
		denied,
		onretry
	}: {
		view: CardView;
		/** What the card loads, capitalised: "Pools", "System details". */
		what: string;
		/** The sentence for a missing role. */
		denied: string;
		onretry: () => void;
	} = $props();

	const stale = $derived(
		(view.kind === 'ready' || view.kind === 'empty') && view.stale ? view.stale : null
	);
</script>

{#if view.kind === 'denied'}
	<p class="context">{denied}</p>
{:else if view.kind === 'error' || stale}
	<div class={['notice', { stale }]} role="status">
		<p class="context">
			{#if stale}Updates stopped: {stale.message}{:else if view.kind === 'error'}{what} didn't load:
				{view.error.message}{/if}
		</p>
		<div><button type="button" class="button-outline" onclick={onretry}>Try again</button></div>
	</div>
{/if}

<style>
	p {
		margin: 0;
	}
	.context {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.notice {
		display: grid;
		gap: var(--space-3);
	}
	.stale {
		padding: var(--space-3) var(--space-4);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-lg);
		background: var(--surface-inset);
	}
</style>

<script lang="ts">
	import Icon from '#lib/components/Icon.svelte';
	import { connection } from '#lib/nas.svelte.js';

	/** Short blips (a NAS reconnect that takes half a second) do not flash the banner. */
	const DELAY_MS = 1000;

	let shown = $state<typeof connection.status | null>(null);

	$effect(() => {
		const status = connection.status;
		// Only these two retry on their own; a logged-out socket sends the page to /login
		// (routes/(app)/+layout.svelte).
		if (status !== 'nas-reconnecting' && status !== 'offline') {
			shown = null;
			return;
		}
		const timer = setTimeout(() => (shown = status), DELAY_MS);
		return () => clearTimeout(timer);
	});
</script>

<div class="slot" role="status" aria-live="polite">
	{#if shown}
		<div class="banner">
			<span class="spin"><Icon name="refresh-cw" /></span>
			<span>
				{shown === 'nas-reconnecting'
					? 'Reconnecting to NAS…'
					: 'Can’t reach truelite. Retrying…'}
			</span>
		</div>
	{/if}
</div>

<style>
	.slot {
		position: sticky;
		top: 0;
		z-index: 10;
	}
	.banner {
		--tone: var(--warning);
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin: var(--space-2) var(--space-4) 0;
		margin-top: max(var(--space-2), env(safe-area-inset-top));
		padding: var(--space-3) var(--space-4);
		border: 1px solid color-mix(in srgb, var(--tone) 40%, transparent);
		border-radius: var(--radius-lg);
		background: color-mix(in srgb, var(--tone) 10%, var(--container));
		color: var(--text-primary);
		font-weight: var(--weight-medium);
	}
	.spin {
		display: flex;
		color: var(--tone);
		animation: spin 1.2s linear infinite;
	}
	@media (prefers-reduced-motion: reduce) {
		.spin {
			animation: none;
		}
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>

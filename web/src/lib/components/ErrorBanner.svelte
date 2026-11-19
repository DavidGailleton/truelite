<script lang="ts">
	// A failed action in plain words, with a button to dismiss it. Shared by the app actions and
	// the snapshots page.
	import Icon from './Icon.svelte';

	let {
		message,
		ondismiss,
		compact = false
	}: { message: string; ondismiss: () => void; compact?: boolean } = $props();
</script>

<div class={['error', { compact }]} role="alert">
	<Icon name="circle-alert" size={16} />
	<p>{message}</p>
	<button type="button" class="dismiss" aria-label="Dismiss" onclick={ondismiss}>×</button>
</div>

<style>
	.error {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-md);
		background: var(--destructive-tint);
		color: var(--destructive);
		font-size: var(--text-sm);
	}
	.compact {
		font-size: var(--text-xs);
	}
	.error > :global(svg) {
		flex: none;
		margin-top: 0.125rem;
	}
	p {
		flex: 1;
		margin: 0;
		color: var(--text-primary);
	}
	.dismiss {
		flex: none;
		width: 1.5rem;
		height: 1.5rem;
		margin: -0.125rem -0.25rem 0 0;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text-secondary);
		font: inherit;
		font-size: var(--text-base);
		cursor: pointer;
	}
	.dismiss:hover {
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.dismiss {
			width: var(--touch);
			height: var(--touch);
			margin: calc(-1 * var(--space-3)) calc(-1 * var(--space-3)) 0 0;
		}
	}
</style>

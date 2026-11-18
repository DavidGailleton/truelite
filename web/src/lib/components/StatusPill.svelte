<script lang="ts">
	// Status badge (docs/ui-style.md): words first, the tone only repeats them. The dot marks a
	// live state ("Running", "Starting…").
	import type { Tone } from '#lib/apps.js';

	let {
		label,
		tone = 'neutral',
		dot = false
	}: { label: string; tone?: Tone; dot?: boolean } = $props();
</script>

<span class="pill {tone}">
	{#if dot}<span class="dot" aria-hidden="true"></span>{/if}{label}
</span>

<style>
	.pill {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		padding: 0 var(--space-2);
		border: 1px solid currentColor;
		border-radius: var(--radius-full);
		font-size: var(--text-xs);
		white-space: nowrap;
	}
	.success {
		background: var(--success-tint);
		color: var(--success);
	}
	.warning {
		background: var(--warning-tint);
		color: var(--warning);
	}
	.destructive {
		background: var(--destructive-tint);
		color: var(--destructive);
	}
	.neutral {
		border-color: var(--border-secondary);
		background: var(--surface-inset);
		color: var(--text-secondary);
	}
	.dot {
		width: 0.375rem;
		height: 0.375rem;
		border-radius: var(--radius-full);
		background: currentColor;
	}
</style>

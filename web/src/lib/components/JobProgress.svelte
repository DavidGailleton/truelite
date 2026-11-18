<script lang="ts">
	// Progress of a running action (a TrueNAS job), as one line of text: what the job is doing,
	// and its percent when the NAS gives one. Announced politely to screen readers.
	import type { JobProgress } from '#lib/ws.js';

	let { progress, label }: { progress: JobProgress | null; label: string } = $props();

	const percent = $derived(progress?.percent ?? null);
	const text = $derived(progress?.description ?? label);
</script>

<p class="job" role="status" aria-live="polite">
	<span class="text">{text}</span>
	{#if percent !== null}<span class="percent">{Math.round(percent)} %</span>{/if}
</p>

<style>
	.job {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
		margin: 0;
		font-size: var(--text-sm);
	}
	.text {
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.percent {
		flex: none;
		color: var(--text-secondary);
		font-variant-numeric: tabular-nums;
	}
</style>

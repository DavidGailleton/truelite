<script lang="ts">
	// Progress of a running action (a TrueNAS job): a bar, indeterminate until the NAS gives a
	// percent, and what the job is doing. Announced politely to screen readers.
	import type { JobProgress } from '#lib/ws.js';

	let { progress, label }: { progress: JobProgress | null; label: string } = $props();

	const percent = $derived(progress?.percent ?? null);
	const text = $derived(progress?.description ?? label);
</script>

<div class="job" role="status" aria-live="polite">
	<div class="line">
		<span class="text">{text}</span>
		{#if percent !== null}<span class="percent">{Math.round(percent)} %</span>{/if}
	</div>
	<progress max="100" value={percent ?? undefined} aria-label={label}></progress>
</div>

<style>
	.job {
		display: grid;
		gap: var(--space-2);
	}
	.line {
		display: flex;
		justify-content: space-between;
		gap: var(--space-3);
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
	progress {
		width: 100%;
		height: 0.375rem;
		border: 0;
		border-radius: var(--radius-full);
		background: var(--chart-track);
		appearance: none;
		overflow: hidden;
	}
	progress::-webkit-progress-bar {
		background: var(--chart-track);
	}
	progress::-webkit-progress-value {
		background: var(--chart-primary);
	}
	progress::-moz-progress-bar {
		background: var(--chart-primary);
	}
</style>

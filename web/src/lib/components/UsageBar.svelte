<script lang="ts">
	// A pool's space: a bar coloured by how full it is, then "X used of Y · Z%". Shared by the
	// dashboard's Storage card and the pools page.
	import { FULL_CRITICAL, FULL_WARNING, type Usage } from '#lib/pools.js';
	import { formatBytes, formatPercent } from '#lib/realtime.js';

	let { usage, name }: { usage: Usage; name: string } = $props();

	const fill = $derived(
		usage.percent >= FULL_CRITICAL ? 'critical' : usage.percent >= FULL_WARNING ? 'warning' : 'ok'
	);
</script>

<div
	class="bar"
	role="meter"
	aria-label="Space used on {name}"
	aria-valuemin={0}
	aria-valuemax={100}
	aria-valuenow={Math.round(usage.percent)}
>
	<span class={fill} style:width="{usage.percent}%"></span>
</div>
<p class="context">
	{formatBytes(usage.used)} used of {formatBytes(usage.total)} · {formatPercent(
		usage.percent
	)}{#if usage.raw}<span title="Raw disk space, parity included"> (raw)</span>{/if}
</p>

<style>
	p {
		margin: 0;
	}
	.context {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.bar {
		display: flex;
		height: 0.5rem;
		overflow: hidden;
		border-radius: var(--radius-full);
		background: var(--chart-track);
	}
	.ok {
		background: var(--chart-primary);
	}
	.warning {
		background: var(--warning);
	}
	.critical {
		background: var(--destructive);
	}
</style>

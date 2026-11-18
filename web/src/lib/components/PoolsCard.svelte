<script lang="ts">
	// Storage card: one row per pool, worst first, with its status in words and colour, a usage
	// bar and the scrub/resilver line. The dashboard calls usePools(); this renders the state.
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import { FULL_CRITICAL, FULL_WARNING } from '#lib/pools.js';
	import { poolRowsNow, pools } from '#lib/pools.svelte.js';
	import { formatBytes, formatPercent } from '#lib/realtime.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';

	let { onretry }: { onretry: () => void } = $props();

	const list = $derived(pools.list);
	const rows = $derived(poolRowsNow() ?? []);
	const view = $derived(
		cardView({
			hasData: list !== null,
			isEmpty: rows.length === 0,
			error: pools.error,
			status: connection.status
		})
	);

	const fill = (percent: number) =>
		percent >= FULL_CRITICAL ? 'critical' : percent >= FULL_WARNING ? 'warning' : 'ok';
</script>

{#snippet badge()}<CardBadge {view} />{/snippet}

<Card title="Storage" icon="hard-drive" control={badge}>
	<CardNotice
		{view}
		what="Pools"
		denied="Your account can't see pools. It needs the Pool read role (POOL_READ) on the NAS."
		{onretry}
	/>
	{#if view.kind === 'empty'}
		<p class="context">No pools yet. Create one in the TrueNAS web interface.</p>
	{:else if view.kind === 'ready'}
		<ul class={['pools', { dim: view.paused || view.stale }]}>
			{#each rows as row (row.id)}
				<li>
					<div class="head">
						<span class="name">{row.name}</span>
						<span class="status {row.health.level}">
							<span class="dot" aria-hidden="true"></span>{row.health.label}
						</span>
					</div>
					{#if row.usage}
						<div
							class="bar"
							role="meter"
							aria-label="Space used on {row.name}"
							aria-valuemin={0}
							aria-valuemax={100}
							aria-valuenow={Math.round(row.usage.percent)}
						>
							<span class={fill(row.usage.percent)} style:width="{row.usage.percent}%"></span>
						</div>
						<p class="context">
							{formatBytes(row.usage.used)} used of {formatBytes(row.usage.total)} · {formatPercent(
								row.usage.percent
							)}{#if row.usage.raw}<span title="Raw disk space, parity included"> (raw)</span>{/if}
						</p>
					{/if}
					{#if row.health.detail}<p class="detail">{row.health.detail}</p>{/if}
					{#if row.scan}<p class="context">{row.scan}</p>{/if}
				</li>
			{/each}
		</ul>
	{:else if view.kind === 'loading'}
		<ul class="pools" aria-hidden="true">
			{#each [0, 1] as i (i)}
				<li>
					<span class="skeleton line" style:width="6rem"></span>
					<span class="skeleton bar"></span>
					<span class="skeleton line" style:width="12rem"></span>
				</li>
			{/each}
		</ul>
		<p class="visually-hidden">Loading pools…</p>
	{/if}
</Card>

<style>
	p {
		margin: 0;
	}
	.context {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.detail {
		font-size: var(--text-sm);
	}
	.dim {
		opacity: 0.5;
	}

	.pools {
		display: grid;
		gap: var(--space-5);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		display: grid;
		gap: var(--space-2);
		min-width: 0;
	}
	.head {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-1) var(--space-3);
	}
	.name {
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
	}

	.status {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		padding: 0 var(--space-2);
		border: 1px solid currentColor;
		border-radius: var(--radius-full);
		font-size: var(--text-xs);
		white-space: nowrap;
	}
	.dot {
		width: 0.375rem;
		height: 0.375rem;
		border-radius: var(--radius-full);
		background: currentColor;
	}
	.status.ok {
		background: var(--success-tint);
		color: var(--success);
	}
	.status.warning {
		background: var(--warning-tint);
		color: var(--warning);
	}
	.status.critical {
		background: var(--destructive-tint);
		color: var(--destructive);
	}

	.bar {
		display: flex;
		height: 0.5rem;
		overflow: hidden;
		border-radius: var(--radius-full);
		background: var(--chart-track);
	}
	.bar .ok {
		background: var(--chart-primary);
	}
	.bar .warning {
		background: var(--warning);
	}
	.bar .critical {
		background: var(--destructive);
	}

	.skeleton.bar {
		display: block;
	}
</style>

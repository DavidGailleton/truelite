<script lang="ts">
	// Storage: one card per pool, the ones in trouble first, each with its space, status, last
	// data check and layout (vdevs and disks). Live through `pool.query` / `pool.scan` events (the
	// store is shared with the dashboard card); layouts are read here only, again after each pool
	// change. Under them, the dataset tree with each dataset's space (its own card and store),
	// each linking to its snapshots (#37). Each pool card can start, pause and stop a data check
	// (#38). Disk health (#39) comes next.
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import { layouts, poolRowsNow, pools, usePoolLayouts, usePools } from '#lib/pools.svelte.js';
	import Card from '#lib/components/Card.svelte';
	import CardNotice from '#lib/components/CardNotice.svelte';
	import DatasetsCard from '#lib/components/DatasetsCard.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import PoolCard from '#lib/components/PoolCard.svelte';

	const topic = usePools();
	const layout = usePoolLayouts();

	const rows = $derived(poolRowsNow() ?? []);
	const view = $derived(
		cardView({
			hasData: pools.list !== null,
			isEmpty: rows.length === 0,
			error: pools.error,
			status: connection.status
		})
	);
	const subtitle = $derived.by(() => {
		if (pools.list === null) return undefined;
		const n = rows.length;
		const bad = rows.filter((r) => r.health.level !== 'ok').length;
		const pools_ = `${n} ${n === 1 ? 'pool' : 'pools'}`;
		return bad > 0 ? `${pools_} · ${bad} ${bad === 1 ? 'needs' : 'need'} attention` : pools_;
	});

	function retry(): void {
		topic.retry();
		layout.retry();
	}
</script>

<PageHeader title="Storage" {subtitle} loading={view.kind === 'loading'} />

{#if view.kind === 'empty'}
	<EmptyState
		icon="hard-drive"
		title="No pools yet"
		text="Create a pool in the TrueNAS web interface and it shows up here."
	/>
{:else if view.kind === 'ready'}
	<div class="page">
		{#if view.stale}
			<CardNotice {view} what="Pools" denied="" onretry={retry} />
		{/if}
		<div class="pools">
			{#each rows as row (row.id)}
				<PoolCard
					{row}
					layout={layouts.map ? (layouts.map.get(row.id) ?? null) : undefined}
					layoutError={layouts.map ? null : layouts.error}
					dim={view.paused || view.stale !== null}
				/>
			{/each}
		</div>
		<DatasetsCard />
	</div>
{:else}
	<Card title="Pools" icon="hard-drive">
		<CardNotice
			{view}
			what="Pools"
			denied="Your account can't see pools. It needs the Pool read role (POOL_READ) on the NAS."
			onretry={retry}
		/>
		{#if view.kind === 'loading'}
			<div class="placeholder" aria-hidden="true">
				<span class="skeleton line" style:width="6rem"></span>
				<span class="skeleton bar"></span>
				<span class="skeleton line" style:width="12rem"></span>
			</div>
			<p class="visually-hidden">Loading pools…</p>
		{/if}
	</Card>
{/if}

<style>
	p {
		margin: 0;
	}
	.page {
		display: grid;
		gap: var(--space-4);
	}
	.pools {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(min(100%, 24rem), 1fr));
		align-items: start;
		gap: var(--space-4);
	}
	.placeholder {
		display: grid;
		gap: var(--space-2);
	}
	.skeleton.bar {
		display: block;
		height: 0.5rem;
		border-radius: var(--radius-full);
	}
</style>

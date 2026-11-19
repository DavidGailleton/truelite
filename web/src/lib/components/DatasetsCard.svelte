<script lang="ts">
	// The Storage page's dataset tree: one tree per pool, each dataset with its space, a bar when
	// a quota limits it, and its state in words (locked, encrypted, volume). Branches holding
	// something that needs a look start opened. TrueNAS's own datasets are hidden and counted.
	// Each row links to the dataset's snapshots (#37).
	import { cardView } from '#lib/card-state.js';
	import { datasetUsage, openIds, type DatasetNode } from '#lib/datasets.js';
	import { datasets, useDatasets } from '#lib/datasets.svelte.js';
	import { connection } from '#lib/nas.svelte.js';
	import { formatBytes } from '#lib/realtime.js';
	import { snapshotsHref } from '#lib/snapshots.js';
	import Card from './Card.svelte';
	import CardNotice from './CardNotice.svelte';
	import Icon from './Icon.svelte';
	import StatusPill from './StatusPill.svelte';
	import UsageBar from './UsageBar.svelte';

	const topic = useDatasets();

	const tree = $derived(datasets.tree);
	const view = $derived(
		cardView({
			hasData: tree !== null,
			isEmpty: tree?.roots.length === 0,
			error: datasets.error,
			status: connection.status
		})
	);
	const open = $derived(tree ? openIds(tree.roots) : new Set<string>());
	const dim = $derived(
		(view.kind === 'ready' || view.kind === 'empty') && (view.paused || view.stale !== null)
	);
</script>

{#snippet row(d: DatasetNode, label: string)}
	{@const u = datasetUsage(d)}
	<span class="head">
		<span class="name">{label}</span>
		{#if d.locked}
			<StatusPill label="Locked" tone="warning" />
		{:else if d.encrypted}
			<span class="tag"><Icon name="lock" size={12} />Encrypted</span>
		{/if}
		{#if d.volume}<span class="tag">Volume</span>{/if}
	</span>
	{#if d.locked}
		<span class="space">Unlock it in TrueNAS to read its data.</span>
	{:else if d.quota !== null && u}
		<span class="quota">
			<UsageBar
				usage={{ used: d.used, total: d.used + d.available, percent: u.percent, raw: false }}
				name={d.id}
			/>
		</span>
	{:else}
		<span class="space">{formatBytes(d.used)} used · {formatBytes(d.available)} free</span>
	{/if}
	<!-- A link wins over its <summary>: following it doesn't open or close the branch. -->
	<a
		class="snapshots"
		href={snapshotsHref(d.id)}
		aria-label="Snapshots of {d.id}"
		title="Snapshots"
	>
		<Icon name="camera" size={16} />
	</a>
{/snippet}

{#snippet node(d: DatasetNode, label: string)}
	{#if d.children.length > 0}
		<li>
			<details open={open.has(d.id)}>
				<summary>{@render row(d, label)}</summary>
				<ul>
					{#each d.children as c (c.id)}{@render node(c, c.name)}{/each}
				</ul>
			</details>
		</li>
	{:else}
		<li class="leaf">{@render row(d, label)}</li>
	{/if}
{/snippet}

<Card title="Datasets" icon="folder">
	<CardNotice
		{view}
		what="Datasets"
		denied="Your account can't see datasets. It needs the Dataset read role (DATASET_READ) on the NAS."
		onretry={topic.retry}
	/>
	{#if view.kind === 'loading'}
		<div class="placeholder" aria-hidden="true">
			<span class="skeleton line" style:width="8rem"></span>
			<span class="skeleton line" style:width="12rem"></span>
			<span class="skeleton line" style:width="10rem"></span>
		</div>
		<p class="visually-hidden">Loading datasets…</p>
	{:else if view.kind === 'empty'}
		<p class="space">No datasets yet.</p>
	{:else if view.kind === 'ready' && tree}
		<div class={['body', { dim }]}>
			<ul class="tree">
				{#each tree.roots as r (r.id)}{@render node(r, r.id)}{/each}
			</ul>
			{#if tree.hidden > 0}
				<p class="space">
					{tree.hidden} system {tree.hidden === 1 ? 'dataset' : 'datasets'} hidden
				</p>
			{/if}
		</div>
	{/if}
</Card>

<style>
	p {
		margin: 0;
	}
	.body {
		display: grid;
		gap: var(--space-3);
		min-width: 0;
	}
	.dim {
		opacity: 0.5;
	}
	.placeholder {
		display: grid;
		gap: var(--space-2);
	}

	ul {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	/* Nested levels: a hairline and a small indent, so six levels still fit at 360 px. */
	details ul {
		margin-left: 0.4375rem;
		padding-left: var(--space-2);
		border-left: 1px solid var(--border-secondary);
	}

	summary,
	.leaf {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-1) var(--space-3);
		min-height: 2.5rem;
		padding: var(--space-1) 0 var(--space-1) 1.25rem;
		font-size: var(--text-sm);
	}
	@media (pointer: coarse) {
		summary,
		.leaf {
			min-height: 2.75rem;
		}
	}
	summary {
		position: relative;
		list-style: none;
		cursor: pointer;
	}
	summary::-webkit-details-marker {
		display: none;
	}
	/* A chevron that turns when the branch opens. */
	summary::before {
		content: '';
		position: absolute;
		top: 50%;
		left: 0.25rem;
		width: 0.4rem;
		height: 0.4rem;
		border-right: 1.5px solid var(--text-secondary);
		border-bottom: 1.5px solid var(--text-secondary);
		transform: translateY(-75%) rotate(45deg);
		transition: transform 0.15s;
	}
	details:not([open]) > summary::before {
		transform: translateY(-50%) rotate(-45deg);
	}
	@media (prefers-reduced-motion: reduce) {
		summary::before {
			transition: none;
		}
	}

	.head {
		margin-right: auto;
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
		min-width: 0;
	}
	.name {
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
	}
	.tag {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.space {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	/* After the space, at the row's end (`.head` takes the free room). */
	.snapshots {
		display: inline-grid;
		flex: none;
		place-items: center;
		width: 2.25rem;
		height: 2.25rem;
		margin: -0.25rem 0;
		border-radius: var(--radius-md);
		color: var(--text-secondary);
	}
	.snapshots:hover {
		color: var(--text-primary);
		background: var(--container-inset);
	}
	@media (pointer: coarse) {
		.snapshots {
			width: var(--touch);
			height: var(--touch);
		}
	}
	.quota {
		display: grid;
		gap: var(--space-1);
		flex: 1 1 12rem;
		max-width: 20rem;
	}
</style>

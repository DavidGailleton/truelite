<script lang="ts">
	// Apps card: how many apps run, are stopped or have an update, and which ones crashed. The
	// dashboard calls useApps(); this renders the state.
	import { appSummary, apps } from '#lib/apps.svelte.js';
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';
	import StatusPill from './StatusPill.svelte';

	let { onretry }: { onretry: () => void } = $props();

	const list = $derived(apps.list);
	const summary = $derived(appSummary());
	const view = $derived(
		cardView({
			hasData: list !== null,
			isEmpty: list?.length === 0,
			error: apps.error,
			status: connection.status
		})
	);
</script>

{#snippet badge()}
	<span class="header-end">
		<CardBadge {view} />
		{#if view.kind === 'ready'}<a class="all" href="/apps">See all</a>{/if}
	</span>
{/snippet}

<Card title="Apps" icon="box" control={badge}>
	<CardNotice
		{view}
		what="Apps"
		denied="Your account can't see apps. It needs the Apps read role (APPS_READ) on the NAS."
		{onretry}
	/>
	{#if view.kind === 'empty'}
		<p class="context">No apps installed yet. Install one in the TrueNAS web interface.</p>
	{:else if view.kind === 'ready' && summary}
		<div class={['body', { dim: view.paused || view.stale }]}>
			<dl class="stats">
				<div>
					<dt>Running</dt>
					<dd>{summary.running}</dd>
				</div>
				<div>
					<dt>Stopped</dt>
					<dd>{summary.stopped}</dd>
				</div>
				<div class={{ warning: summary.updates > 0 }}>
					<dt>{summary.updates === 1 ? 'Update' : 'Updates'}</dt>
					<dd>
						{#if summary.updates > 0}
							<a href="/apps?show=updates" aria-label="{summary.updates} apps with an update">
								{summary.updates}
							</a>
						{:else}0{/if}
					</dd>
				</div>
			</dl>
			{#if summary.crashed.length > 0}
				<ul class="crashed">
					{#each summary.crashed as app (app.id)}
						<li>
							<a class="name" href="/apps/{encodeURIComponent(app.id)}">{app.name}</a>
							<StatusPill label="Crashed" tone="destructive" />
						</li>
					{/each}
				</ul>
			{/if}
			{#if summary.starting > 0}
				<p class="context">
					{summary.starting}
					{summary.starting === 1 ? 'app is' : 'apps are'} starting or stopping…
				</p>
			{/if}
		</div>
	{:else if view.kind === 'loading'}
		<div class="stats" aria-hidden="true">
			{#each [0, 1, 2] as i (i)}
				<div>
					<span class="skeleton line" style:width="4rem"></span>
					<span class="skeleton number"></span>
				</div>
			{/each}
		</div>
		<p class="visually-hidden">Loading apps…</p>
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
	.body {
		display: grid;
		gap: var(--space-4);
	}
	.dim {
		opacity: 0.5;
	}

	.stats {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: var(--space-3);
		margin: 0;
	}
	.stats > div {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}
	dt {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	dd {
		margin: 0;
		font-size: var(--text-xl);
		font-weight: var(--weight-medium);
		font-variant-numeric: tabular-nums;
	}
	.warning dt,
	.warning dd {
		color: var(--warning);
	}

	.crashed {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.crashed li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-1) var(--space-3);
	}
	.name {
		color: inherit;
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
		text-decoration: underline;
		text-decoration-thickness: 1px;
		text-underline-offset: 0.2em;
	}
	.header-end {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.all {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.all:hover,
	dd a:hover {
		color: var(--text-primary);
	}
	dd a {
		color: inherit;
		text-decoration: underline;
		text-decoration-thickness: 1px;
		text-underline-offset: 0.2em;
	}

	.skeleton.number {
		display: block;
		width: 2rem;
		height: 1.75rem;
	}
</style>

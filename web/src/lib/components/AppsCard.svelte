<script lang="ts">
	// Apps card: how many apps run, are stopped or have an update, and which ones crashed. The
	// dashboard calls useApps(); this renders the state.
	import { summarize } from '#lib/apps.js';
	import { apps } from '#lib/apps.svelte.js';
	import { connection } from '#lib/nas.svelte.js';
	import Card from './Card.svelte';

	let { onretry }: { onretry: () => void } = $props();

	const list = $derived(apps.list);
	const summary = $derived(list ? summarize(list) : null);
	const failure = $derived(apps.error);
	const denied = $derived(failure?.code === 'nas' && failure.errname === 'EACCES');
	const paused = $derived(connection.status !== 'up');
</script>

{#snippet pausedBadge()}
	{#if paused && list}<span class="paused">Paused</span>{/if}
{/snippet}

<Card title="Apps" icon="box" control={pausedBadge}>
	{#if denied}
		<p class="context">Your account can't see apps. It needs read-only admin access on the NAS.</p>
	{:else if failure && !list}
		<p class="context">Apps didn't load: {failure.message}</p>
		<div><button type="button" class="outline" onclick={onretry}>Try again</button></div>
	{:else if list && list.length === 0}
		<p class="context">No apps installed yet.</p>
	{:else if summary}
		<div class={['body', { dim: paused }]}>
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
					<dd>{summary.updates}</dd>
				</div>
			</dl>
			{#if summary.crashed.length > 0}
				<ul class="crashed">
					{#each summary.crashed as app (app.id)}
						<li>
							<span class="name">{app.name}</span>
							<span class="status"><span class="dot" aria-hidden="true"></span>Crashed</span>
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
	{:else}
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
	.paused {
		padding: 0 var(--space-2);
		border-radius: var(--radius-full);
		background: var(--surface-inset);
		color: var(--text-secondary);
		font-size: var(--text-xs);
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
		background: var(--destructive-tint);
		color: var(--destructive);
		font-size: var(--text-xs);
		white-space: nowrap;
	}
	.dot {
		width: 0.375rem;
		height: 0.375rem;
		border-radius: var(--radius-full);
		background: currentColor;
	}

	.skeleton.line {
		display: block;
		max-width: 100%;
		height: 1rem;
	}
	.skeleton.number {
		display: block;
		width: 2rem;
		height: 1.75rem;
	}

	button.outline {
		height: 2.25rem;
		padding: 0 var(--space-3);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text-primary);
		font: inherit;
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		cursor: pointer;
	}
	button.outline:hover {
		background: var(--surface-hover);
	}
	@media (pointer: coarse) {
		button.outline {
			height: var(--touch);
		}
	}
</style>

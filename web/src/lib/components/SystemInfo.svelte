<script lang="ts">
	// System card: the hardware basics from `system.info`. The dashboard calls useSystemInfo();
	// this only renders the shared state.
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import { formatBytes } from '#lib/realtime.js';
	import { coresLabel } from '#lib/system.js';
	import { loadSystemInfo, system } from '#lib/system.svelte.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';

	const info = $derived(system.info);
	const view = $derived(
		cardView({ hasData: info !== null, error: system.error, status: connection.status })
	);

	const started = $derived(
		info?.bootTime?.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
	);
</script>

{#snippet badge()}<CardBadge {view} />{/snippet}

<Card title="System" icon="server" control={badge}>
	<CardNotice
		{view}
		what="System details"
		denied="Your account can't see system details. It needs read-only admin access on the NAS."
		onretry={loadSystemInfo}
	/>
	{#if view.kind === 'ready' && info}
		<dl class={{ dim: view.paused || view.stale }}>
			<div>
				<dt>Processor</dt>
				<dd>
					{info.cpu}<span class="context"> · {coresLabel(info.cores, info.physicalCores)}</span>
				</dd>
			</div>
			<div>
				<dt>Memory</dt>
				<dd>{formatBytes(info.memory)}{info.ecc ? ', error-correcting (ECC)' : ''}</dd>
			</div>
			{#if info.hardware}
				<div>
					<dt>Hardware</dt>
					<dd>{info.hardware}</dd>
				</div>
			{/if}
			{#if started}
				<div>
					<dt>Started</dt>
					<dd>{started}</dd>
				</div>
			{/if}
			{#if info.timezone}
				<div>
					<dt>Time zone</dt>
					<dd>{info.timezone}</dd>
				</div>
			{/if}
		</dl>
	{:else if view.kind === 'loading'}
		<dl aria-hidden="true">
			{#each [10, 6, 8] as width, i (i)}
				<div>
					<dt><span class="skeleton line" style:width="5rem"></span></dt>
					<dd><span class="skeleton line" style:width="{width}rem"></span></dd>
				</div>
			{/each}
		</dl>
		<p class="visually-hidden">Loading system details…</p>
	{/if}
</Card>

<style>
	p {
		margin: 0;
	}
	.context {
		color: var(--text-secondary);
	}
	.dim {
		opacity: 0.5;
	}

	dl {
		display: grid;
		gap: var(--space-3);
		margin: 0;
	}
	dl > div {
		display: grid;
		gap: var(--space-1);
	}
	dt {
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	dd {
		margin: 0;
		overflow-wrap: anywhere;
	}
	@media (min-width: 480px) {
		dl > div {
			grid-template-columns: 7rem minmax(0, 1fr);
			gap: var(--space-3);
		}
		dt {
			font-size: inherit;
		}
	}
</style>

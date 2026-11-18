<script lang="ts">
	// System card: the hardware basics from `system.info`. The dashboard calls useSystemInfo();
	// this only renders the shared state.
	import { connection } from '#lib/nas.svelte.js';
	import { formatBytes } from '#lib/realtime.js';
	import { coresLabel } from '#lib/system.js';
	import { loadSystemInfo, system } from '#lib/system.svelte.js';
	import Card from './Card.svelte';

	const info = $derived(system.info);
	const failure = $derived(system.error);
	const denied = $derived(failure?.code === 'nas' && failure.errname === 'EACCES');
	const paused = $derived(connection.status !== 'up');

	const started = $derived(
		info?.bootTime?.toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })
	);
</script>

{#snippet pausedBadge()}
	{#if paused && info}<span class="badge">Paused</span>{/if}
{/snippet}

<Card title="System" icon="server" control={pausedBadge}>
	{#if denied}
		<p class="context">
			Your account can't see system details. It needs read-only admin access on the NAS.
		</p>
	{:else if failure && !info}
		<p class="context">System details didn't load: {failure.message}</p>
		<div><button type="button" class="outline" onclick={loadSystemInfo}>Try again</button></div>
	{:else if info}
		<dl class:dim={paused}>
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
	{:else}
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
	.badge {
		padding: 0 var(--space-2);
		border-radius: var(--radius-full);
		background: var(--surface-inset);
		color: var(--text-secondary);
		font-size: var(--text-xs);
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

	.skeleton.line {
		display: inline-block;
		max-width: 100%;
		height: 1rem;
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

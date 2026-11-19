<script lang="ts">
	// Updates card on the System page (#40): installed version, update channel, and either "up to
	// date", the version on offer with its release notes, or why the check failed. Live from
	// `update.status` events; the page calls useUpdateStatus() and useSystemInfo().
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import { versionLabel } from '#lib/system.js';
	import { system } from '#lib/system.svelte.js';
	import { update } from '#lib/update.svelte.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';
	import Icon from './Icon.svelte';
	import StatusPill from './StatusPill.svelte';

	let { onretry }: { onretry: () => void } = $props();

	const status = $derived(update.status);
	const view = $derived(
		cardView({ hasData: status !== null, error: update.error, status: connection.status })
	);
	const installed = $derived(system.info ? versionLabel(system.info.version) : null);
</script>

{#snippet badge()}<CardBadge {view} />{/snippet}

<Card title="Updates" icon="circle-arrow-up" control={badge}>
	<CardNotice
		{view}
		what="Update status"
		denied="Your account can't see updates. It needs the update read role on the NAS."
		{onretry}
	/>
	{#if view.kind === 'ready' && status}
		<div class={['body', { dim: view.paused || view.stale }]}>
			<dl>
				{#if installed}
					<div>
						<dt>Installed</dt>
						<dd>{installed}</dd>
					</div>
				{/if}
				{#if status.train}
					<div>
						<dt>Update channel</dt>
						<dd>{status.train}</dd>
					</div>
				{/if}
				<div>
					<dt>Status</dt>
					<dd>
						{#if status.kind === 'available'}
							<StatusPill label="Update available" tone="warning" />
						{:else if status.kind === 'current'}
							<StatusPill label="Up to date" tone="success" />
						{:else}
							<StatusPill label="Check failed" tone="destructive" />
						{/if}
					</dd>
				</div>
				{#if status.kind === 'available'}
					<div>
						<dt>New version</dt>
						<dd>{versionLabel(status.newVersion ?? '')}</dd>
					</div>
				{/if}
			</dl>
			{#if status.kind === 'current'}
				<p class="context">TrueNAS is up to date.</p>
			{:else if status.kind === 'error'}
				<p class="context">{status.error}</p>
			{:else}
				{#if status.notes}
					<details>
						<summary>What's new</summary>
						<p class="notes">{status.notes}</p>
					</details>
				{/if}
				{#if status.notesUrl}
					<a class="notes-link" href={status.notesUrl} target="_blank" rel="noopener noreferrer">
						Full release notes <Icon name="external-link" size={16} />
					</a>
				{/if}
			{/if}
		</div>
	{:else if view.kind === 'loading'}
		<dl aria-hidden="true">
			{#each [10, 8, 6] as width, i (i)}
				<div>
					<dt><span class="skeleton line" style:width="5rem"></span></dt>
					<dd><span class="skeleton line" style:width="{width}rem"></span></dd>
				</div>
			{/each}
		</dl>
		<p class="visually-hidden">Checking for updates…</p>
	{/if}
</Card>

<style>
	p {
		margin: 0;
	}
	.body {
		display: grid;
		gap: var(--space-3);
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

	details summary {
		min-height: 2rem;
		align-content: center;
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		cursor: pointer;
	}
	.notes {
		max-height: 16rem;
		overflow-y: auto;
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-md);
		background: var(--container-inset);
		color: var(--text-secondary);
		font-size: var(--text-sm);
		white-space: pre-line;
	}
	.notes-link {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: 2rem;
		justify-self: start;
		font-size: var(--text-sm);
	}
	@media (pointer: coarse) {
		details summary,
		.notes-link {
			min-height: var(--touch);
		}
	}
</style>

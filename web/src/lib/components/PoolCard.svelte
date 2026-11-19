<script lang="ts">
	// One pool on the Storage page: status in words and colour, usable space, what is wrong, the
	// scrub line, then its layout (vdevs and disks, plain words), opened when something in it needs
	// a look. Under the scrub line, its buttons (Check now, Pause, Resume, Stop: #38) for accounts
	// with the Pool write role.
	import type { ErrorBody } from '#lib/ws.js';
	import {
		GROUP_LABELS,
		deviceProblem,
		deviceStatus,
		hasProblem,
		isDisk,
		layoutSummary,
		topologyNeedsLook,
		vdevLabel
	} from '#lib/pool-layout.js';
	import {
		TOPOLOGY_GROUPS,
		type Level,
		type PoolRow,
		type Topology,
		type TopologyGroup,
		type Vdev
	} from '#lib/pools.js';
	import type { Tone } from '#lib/apps.js';
	import { canScrub } from '#lib/scrub.js';
	import { session } from '#lib/session.svelte.js';
	import ScrubActions from './ScrubActions.svelte';
	import Card from './Card.svelte';
	import StatusPill from './StatusPill.svelte';
	import UsageBar from './UsageBar.svelte';

	let {
		row,
		layout,
		layoutError,
		dim = false
	}: {
		row: PoolRow;
		/** `undefined` while loading, `null` when the NAS has none for this pool. */
		layout: Topology | null | undefined;
		layoutError: ErrorBody | null;
		dim?: boolean;
	} = $props();

	const TONES: Record<Level, Tone> = { ok: 'success', warning: 'warning', critical: 'destructive' };
	/** States where the device is gone or unusable. */
	const GONE = new Set(['FAULTED', 'UNAVAIL', 'REMOVED']);

	// A dimmed card is paused (link down) or stale: no actions on what may be old.
	const canAct = $derived(!dim && canScrub(session.user?.roles));
	const groups = $derived(layout ? TOPOLOGY_GROUPS.filter((g) => layout[g].length > 0) : []);
	const needsLook = $derived(layout ? topologyNeedsLook(layout) : false);
</script>

{#snippet pill()}<StatusPill label={row.health.label} tone={TONES[row.health.level]} />{/snippet}

{#snippet device(v: Vdev, group: TopologyGroup, top: boolean)}
	{@const problem = deviceProblem(v)}
	{#if isDisk(v)}
		<!-- A lone top-level data disk has no redundancy; inside a vdev, a disk is a member. -->
		{@const label = top ? vdevLabel(v, group) : null}
		<li class={['disk', { problem }]}>
			<span class="name">{v.disk ?? v.name}</span>
			{#if problem}
				<StatusPill label={problem} tone={GONE.has(v.status) ? 'destructive' : 'warning'} />
			{:else}
				<span class="muted">{deviceStatus(v.status)}</span>
			{/if}
			{#if label?.risky}<span class="risk">{label.text}</span>{/if}
		</li>
	{:else}
		<li class="vdev">
			<div class={['vdev-head', { problem: hasProblem(v) }]}>
				<span>{vdevLabel(v, group).text}</span>
				{#if problem}<StatusPill label={problem} tone="warning" />{/if}
			</div>
			<ul>
				{#each v.children as child (child.name)}{@render device(child, group, false)}{/each}
			</ul>
		</li>
	{/if}
{/snippet}

<Card title={row.name} icon="hard-drive" control={pill}>
	<div class={['body', { dim }]}>
		{#if row.usage}<UsageBar usage={row.usage} name={row.name} />{/if}
		{#if row.health.detail}<p>{row.health.detail}</p>{/if}
		{#if row.scan}<p class="muted">{row.scan}</p>{/if}
		<ScrubActions name={row.name} status={row.status} scan={row.scanState} {canAct} />

		{#if layout}
			<details open={needsLook}>
				<summary>
					<span>Layout</span>
					<span class="muted">{layoutSummary(layout)}</span>
				</summary>
				{#each groups as g (g)}
					<section class="group">
						<h3>{GROUP_LABELS[g]}</h3>
						<ul>
							{#each layout[g] as v (v.name)}{@render device(v, g, true)}{/each}
						</ul>
					</section>
				{/each}
			</details>
		{:else if layoutError}
			<p class="muted">Layout didn't load: {layoutError.message}</p>
		{:else if layout === undefined}
			<span class="skeleton line" style:width="10rem" aria-hidden="true"></span>
		{/if}
	</div>
</Card>

<style>
	p {
		margin: 0;
	}
	.body {
		display: grid;
		gap: var(--space-2);
		min-width: 0;
	}
	.dim {
		opacity: 0.5;
	}
	.muted {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}

	details {
		margin-top: var(--space-2);
		border-top: 1px solid var(--border-secondary);
	}
	summary {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-3);
		min-height: 2.5rem;
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		cursor: pointer;
	}
	@media (pointer: coarse) {
		summary {
			min-height: 2.75rem;
		}
	}
	summary .muted {
		font-weight: var(--weight-normal);
	}

	.group {
		display: grid;
		gap: var(--space-1);
		padding-bottom: var(--space-3);
	}
	h3 {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--text-xs);
		font-weight: var(--weight-medium);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	ul {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.vdev ul {
		padding-left: var(--space-4);
		border-left: 1px solid var(--border-secondary);
		margin-left: var(--space-1);
	}
	.vdev-head,
	.disk {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-1) var(--space-2);
		min-height: 1.75rem;
		font-size: var(--text-sm);
	}
	.vdev-head.problem,
	.disk.problem .name {
		font-weight: var(--weight-medium);
	}
	.name {
		font-family: var(--font-mono);
		overflow-wrap: anywhere;
	}
	.risk {
		color: var(--warning);
		font-size: var(--text-sm);
	}
</style>

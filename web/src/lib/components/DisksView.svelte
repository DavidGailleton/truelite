<script lang="ts">
	// Every disk of the NAS (#39), grouped by pool, the ones needing a look first: model, size,
	// kind and serial, the temperature against its critical limit with the 7-day range, and the
	// disk's state and error counts from its pool layout. Read-only. SMART results are not in the
	// TrueNAS 25.10 API (#61). Reached from the Storage page and from each disk of a pool layout
	// (`#disk-<name>` scrolls to that disk once the list is in).
	import { tick } from 'svelte';
	import { page } from '$app/state';
	import { cardView, isDenied } from '#lib/card-state.js';
	import {
		diskAnchor,
		diskGroups,
		diskKind,
		disksByName,
		disksSummary,
		formatTemp,
		type DiskRow,
		type TempLevel
	} from '#lib/disks.js';
	import { useDisks } from '#lib/disks.svelte.js';
	import { connection } from '#lib/nas.svelte.js';
	import { layouts, usePoolLayouts, usePools } from '#lib/pools.svelte.js';
	import type { Tone } from '#lib/apps.js';
	import { formatBytes } from '#lib/realtime.js';
	import BackLink from './BackLink.svelte';
	import Card from './Card.svelte';
	import CardNotice from './CardNotice.svelte';
	import EmptyState from './EmptyState.svelte';
	import PageHeader from './PageHeader.svelte';
	import StatusPill from './StatusPill.svelte';

	const { store, retry } = useDisks();
	// Pool layouts give each pool disk its state and error counts.
	usePools();
	usePoolLayouts();

	const devices = $derived(layouts.map ? disksByName(layouts.map.values()) : null);
	const groups = $derived(
		store.list ? diskGroups(store.list, store.temps, store.ranges, devices) : []
	);
	const view = $derived(
		cardView({
			hasData: store.list !== null,
			isEmpty: store.list?.length === 0,
			error: store.error,
			status: connection.status
		})
	);
	const dim = $derived(view.kind === 'ready' && (view.paused || view.stale !== null));
	const tempNote = $derived.by(() => {
		if (!store.tempError) return null;
		if (isDenied(store.tempError)) {
			return "Your account can't see disk temperatures. It needs the Reporting read role (REPORTING_READ) on the NAS.";
		}
		return `Temperatures didn't load: ${store.tempError.message}`;
	});

	const LEVEL: Record<TempLevel, { label: string; tone: Tone } | null> = {
		hot: { label: 'Too hot', tone: 'destructive' },
		warm: { label: 'Warm', tone: 'warning' },
		ok: null
	};
	/** States where the disk is gone or unusable. */
	const GONE = new Set(['FAULTED', 'UNAVAIL', 'REMOVED']);

	// A link to one disk: scroll to it once the list is drawn (it isn't there on arrival).
	let scrolled = false;
	$effect(() => {
		const hash = page.url.hash.slice(1);
		if (scrolled || !hash || groups.length === 0) return;
		scrolled = true;
		void tick().then(() => document.getElementById(hash)?.scrollIntoView({ block: 'center' }));
	});

	function temperature(r: DiskRow): string {
		if (store.temps === null) return '';
		if (r.temp?.temp == null) return 'No reading';
		return formatTemp(r.temp.temp);
	}
</script>

<BackLink href="/storage" label="Storage" />

<PageHeader
	title="Disks"
	subtitle={store.list ? disksSummary(groups) : undefined}
	loading={view.kind === 'loading'}
/>

{#if view.kind === 'empty'}
	<EmptyState icon="hard-drive" title="No disks" text="The NAS reports no disks." />
{:else if view.kind === 'ready'}
	<div class="page">
		{#if view.stale}
			<CardNotice {view} what="Disks" denied="" onretry={retry} />
		{/if}
		{#if tempNote}<p class="note">{tempNote}</p>{/if}
		{#each groups as group (group.pool)}
			<Card title={group.title} icon="hard-drive">
				<ul class={['rows', { dim }]}>
					{#each group.rows as r (r.disk.name)}
						{@const level = LEVEL[r.level]}
						{@const kind = diskKind(r.disk)}
						<li id={diskAnchor(r.disk.name)}>
							<div class="text">
								<span class="name">
									<span class="dev">{r.disk.name}</span>{#if r.disk.model}<span class="model">
											{r.disk.model}
										</span>{/if}
								</span>
								<span class="meta">
									{[r.disk.size !== null ? formatBytes(r.disk.size) : null, kind]
										.filter(Boolean)
										.join(' · ')}{#if r.disk.serial}<span class="serial">
											Serial {r.disk.serial}
										</span>{/if}
								</span>
								{#if r.disk.description}<span class="meta">{r.disk.description}</span>{/if}
							</div>
							<div class="state">
								{#if store.temps !== null}
									<span class="temp">
										{#if level}<StatusPill label={level.label} tone={level.tone} />{/if}
										<span class={['reading', { muted: r.temp?.temp == null }]}>
											{temperature(r)}
										</span>
									</span>
									{#if r.temp?.crit != null || r.range}
										<span class="meta">
											{[
												r.temp?.crit != null ? `limit ${formatTemp(r.temp.crit)}` : null,
												r.range && r.range.min !== null && r.range.max !== null
													? `7 days ${Math.round(r.range.min)}–${formatTemp(r.range.max)}`
													: null
											]
												.filter(Boolean)
												.join(' · ')}
										</span>
									{/if}
								{:else if !store.tempError}
									<span class="skeleton line" style:width="4rem" aria-hidden="true"></span>
								{/if}
								{#if r.problem && r.device}
									<StatusPill
										label={r.problem}
										tone={GONE.has(r.device.status) ? 'destructive' : 'warning'}
									/>
								{/if}
							</div>
						</li>
					{/each}
				</ul>
			</Card>
		{/each}
		<p class="note">SMART test results aren't available through the TrueNAS 25.10 API.</p>
	</div>
{:else}
	<Card title="Disks" icon="hard-drive">
		<CardNotice
			{view}
			what="Disks"
			denied="Your account can't see disks. It needs the Disk read role (DISK_READ) on the NAS."
			onretry={retry}
		/>
		{#if view.kind === 'loading'}
			<div class="placeholder" aria-hidden="true">
				<span class="skeleton line" style:width="12rem"></span>
				<span class="skeleton line" style:width="10rem"></span>
				<span class="skeleton line" style:width="11rem"></span>
			</div>
			<p class="visually-hidden">Loading disks…</p>
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
	.note {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.placeholder {
		display: grid;
		gap: var(--space-2);
	}

	.rows {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.rows li {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-1) var(--space-3);
		min-height: 3rem;
		padding: var(--space-2) 0;
		border-top: 1px solid var(--border-secondary);
		scroll-margin-top: var(--space-8);
	}
	.rows li:first-child {
		border-top: 0;
	}
	.rows li:target .name {
		text-decoration: underline;
		text-underline-offset: 0.2em;
	}
	.dim {
		opacity: 0.5;
	}
	.text {
		display: grid;
		gap: 0.125rem;
		min-width: 0;
	}
	.name {
		display: flex;
		flex-wrap: wrap;
		align-items: baseline;
		gap: 0 var(--space-2);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
	}
	.dev {
		font-family: var(--font-mono);
	}
	.model {
		color: var(--text-secondary);
		font-weight: var(--weight-normal);
		overflow-wrap: anywhere;
	}
	.meta {
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.serial {
		margin-left: var(--space-2);
		overflow-wrap: anywhere;
	}
	.state {
		display: grid;
		justify-items: end;
		gap: 0.125rem;
		margin-left: auto;
		text-align: right;
	}
	.temp {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}
	.reading {
		font-size: var(--text-sm);
		font-variant-numeric: tabular-nums;
	}
	.muted {
		color: var(--text-secondary);
	}
</style>

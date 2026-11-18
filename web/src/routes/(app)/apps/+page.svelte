<script lang="ts">
	// Apps: every installed app with its state, version and pending update, attention first.
	// Live through `app.query` events (the store is shared with the dashboard card). Filter and
	// search live in the URL (`?show=updates&q=jelly`), so a reload or a link keeps the view.
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { apps, useApps } from '#lib/apps.svelte.js';
	import {
		APP_FILTERS,
		filterApps,
		filterCounts,
		parseFilter,
		sortApps,
		stateLabel,
		type AppFilter
	} from '#lib/apps.js';
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import Card from '#lib/components/Card.svelte';
	import CardBadge from '#lib/components/CardBadge.svelte';
	import CardNotice from '#lib/components/CardNotice.svelte';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import StatusPill from '#lib/components/StatusPill.svelte';

	const topic = useApps();

	let filter = $state<AppFilter>(parseFilter(page.url.searchParams.get('show')));
	let query = $state(page.url.searchParams.get('q') ?? '');

	// Mirror the view into the URL without a navigation (no reload, focus stays in the field).
	function syncUrl(): void {
		const params = new URLSearchParams();
		if (filter !== 'all') params.set('show', filter);
		if (query.trim() !== '') params.set('q', query);
		const search = params.size > 0 ? `?${params}` : '';
		if (search !== page.url.search) replaceState(`${page.url.pathname}${search}`, page.state);
	}

	const list = $derived(apps.list);
	const counts = $derived(list ? filterCounts(list) : null);
	const shown = $derived(list ? sortApps(filterApps(list, filter, query)) : []);
	const view = $derived(
		cardView({
			hasData: list !== null,
			isEmpty: list?.length === 0,
			error: apps.error,
			status: connection.status
		})
	);
	const subtitle = $derived(
		counts
			? `${counts.all} ${counts.all === 1 ? 'app' : 'apps'} · ${counts.running} running`
			: undefined
	);

	function showAll(): void {
		filter = 'all';
		query = '';
		syncUrl();
	}
</script>

<PageHeader title="Apps" {subtitle} loading={view.kind === 'loading'} />

{#if view.kind === 'empty'}
	<EmptyState
		icon="box"
		title="No apps yet"
		text="Install an app in the TrueNAS web interface and it shows up here."
	/>
{:else}
	<div class="page">
		{#if counts}
			<div class="toolbar">
				<label class="field">
					<span>Search apps</span>
					<input
						type="search"
						bind:value={query}
						oninput={syncUrl}
						placeholder="Name"
						autocomplete="off"
					/>
				</label>
				<fieldset class="filter">
					<legend class="visually-hidden">Show</legend>
					<div class="segments">
						{#each APP_FILTERS as option (option.value)}
							<label class="segment">
								<input
									class="visually-hidden"
									type="radio"
									name="show"
									value={option.value}
									bind:group={filter}
									onchange={syncUrl}
								/>
								{option.label}
								<span class="count">{counts[option.value]}</span>
							</label>
						{/each}
					</div>
				</fieldset>
			</div>
		{/if}

		{#snippet badge()}<CardBadge {view} />{/snippet}

		<Card title="Installed apps" icon="box" control={badge}>
			<CardNotice
				{view}
				what="Apps"
				denied="Your account can't see apps. It needs the Apps read role (APPS_READ) on the NAS."
				onretry={topic.retry}
			/>
			{#if view.kind === 'ready'}
				{#if shown.length === 0}
					<div class="no-match">
						<p>No apps match.</p>
						<button class="button-outline" type="button" onclick={showAll}>Show all</button>
					</div>
				{:else}
					<!-- #31 turns each row into a link to /apps/[id]. -->
					<ul class={['rows', { dim: view.paused || view.stale }]}>
						{#each shown as app (app.id)}
							{@const state = stateLabel(app.state)}
							<li>
								<span class="avatar" aria-hidden="true">{app.name.charAt(0).toUpperCase()}</span>
								<div class="text">
									<span class="name">{app.name}</span>
									{#if app.version || app.custom}
										<span class="meta">
											{#if app.version}<span class="version">{app.version}</span>{/if}
											{#if app.custom}<span class="marker">Custom</span>{/if}
										</span>
									{/if}
								</div>
								<div class="pills">
									{#if app.updateAvailable}<StatusPill label="Update" tone="warning" />{/if}
									<StatusPill {...state} />
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			{:else if view.kind === 'loading'}
				<ul class="rows" aria-hidden="true">
					{#each [0, 1, 2, 3, 4] as i (i)}
						<li>
							<span class="avatar skeleton"></span>
							<div class="text">
								<span class="skeleton line" style:width="{6 + (i % 3) * 2}rem"></span>
							</div>
							<span class="skeleton line" style:width="4rem"></span>
						</li>
					{/each}
				</ul>
				<p class="visually-hidden">Loading apps…</p>
			{/if}
		</Card>
	</div>
{/if}

<style>
	p {
		margin: 0;
	}
	.page {
		display: grid;
		gap: var(--space-4);
	}

	.toolbar {
		display: grid;
		gap: var(--space-3);
	}
	@media (min-width: 768px) {
		.toolbar {
			grid-template-columns: minmax(12rem, 20rem) auto;
			justify-content: space-between;
			align-items: end;
		}
	}
	.field {
		display: grid;
		gap: 0.125rem;
		padding: var(--space-1) var(--space-3);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-md);
		background: var(--container);
		box-shadow: var(--shadow-xs);
	}
	.field:focus-within {
		box-shadow: 0 0 0 4px var(--border-subdued);
	}
	.field span {
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.field input {
		min-height: 1.75rem;
		padding: 0;
		border: 0;
		background: transparent;
		color: var(--text-primary);
		font: inherit;
		font-size: var(--text-sm);
		outline: none;
	}
	.field input::placeholder {
		color: var(--text-secondary);
		opacity: 0.5;
	}

	/* At 360 px the four segments scroll sideways rather than wrap. */
	.filter {
		min-width: 0;
		margin: 0;
		padding: 0;
		border: 0;
		overflow-x: auto;
	}
	.filter .segments {
		width: max-content;
		min-width: 100%;
	}
	.filter .segment {
		flex: 1 0 auto;
		padding: 0 var(--space-3);
		white-space: nowrap;
	}
	.count {
		color: var(--text-secondary);
		font-variant-numeric: tabular-nums;
	}

	.rows {
		display: grid;
		margin: 0 calc(-1 * var(--space-5));
		padding: 0;
		list-style: none;
	}
	.dim {
		opacity: 0.5;
	}
	.rows li {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--touch);
		padding: var(--space-2) var(--space-5);
		border-top: 1px solid var(--border-subdued);
	}
	.avatar {
		display: grid;
		flex: none;
		place-items: center;
		width: 2rem;
		height: 2rem;
		border-radius: var(--radius-md);
		background: var(--surface-inset);
		color: var(--text-secondary);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
	}
	.text {
		display: grid;
		flex: 1;
		min-width: 0;
	}
	.name {
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
	}
	.meta {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.version {
		font-family: var(--font-mono);
		overflow-wrap: anywhere;
	}
	.marker {
		padding: 0 var(--space-1);
		border-radius: var(--radius-md);
		background: var(--surface-inset);
		font-size: 0.6875rem;
		letter-spacing: 0.04em;
		text-transform: uppercase;
	}
	.pills {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: var(--space-1);
	}

	.no-match {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
</style>

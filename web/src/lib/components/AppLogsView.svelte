<script lang="ts">
	// `/apps/<name>/logs`: one app's container logs, live. The container is picked by service
	// name (`?service=`), which survives restarts; the stream restarts with each new container id.
	import { replaceState } from '$app/navigation';
	import { page } from '$app/state';
	import { useAppDetail } from '#lib/app-detail.svelte.js';
	import { sortContainers } from '#lib/app-detail.js';
	import { cardView } from '#lib/card-state.js';
	import { pickContainer } from '#lib/logs.js';
	import { connection } from '#lib/nas.svelte.js';
	import Card from './Card.svelte';
	import CardNotice from './CardNotice.svelte';
	import EmptyState from './EmptyState.svelte';
	import BackLink from './BackLink.svelte';
	import Icon from './Icon.svelte';
	import LogStream from './LogStream.svelte';
	import PageHeader from './PageHeader.svelte';

	let { id }: { id: string } = $props();

	// The page keys this component by id, so `id` is fixed for its lifetime.
	// svelte-ignore state_referenced_locally
	const app = useAppDetail(id);

	let service = $state(page.url.searchParams.get('service'));

	const detail = $derived(app.detail);
	const view = $derived(
		cardView({ hasData: detail !== null, error: app.error, status: connection.status })
	);
	const containers = $derived(detail ? sortContainers(detail.containers) : []);
	const container = $derived(pickContainer(containers, service));

	function choose(next: string): void {
		service = next;
		const params = new URLSearchParams(page.url.search);
		params.set('service', next);
		replaceState(`${page.url.pathname}?${params}`, page.state);
	}
</script>

<BackLink href="/apps/{encodeURIComponent(id)}" label={detail?.app.name ?? id} />

<PageHeader
	title="Logs"
	subtitle={container ? `${detail?.app.name ?? id} · ${container.service}` : undefined}
	loading={view.kind === 'loading'}
/>

{#if app.missing}
	<EmptyState
		icon="box"
		title="App not found"
		text="There is no app named “{id}” on the NAS. It may have been deleted."
	>
		{#snippet action()}<a class="button-outline link-button" href="/apps">See all apps</a>{/snippet}
	</EmptyState>
{:else if detail && !container}
	<EmptyState
		icon="scroll-text"
		title={detail.app.state === 'stopped' ? 'The app is stopped' : 'No containers right now'}
		text={detail.app.state === 'stopped'
			? 'Start the app to see its logs.'
			: 'Logs show up once the app has a container.'}
	/>
{:else}
	<Card title="Log" icon="scroll-text">
		<CardNotice
			{view}
			what="The app"
			denied="Your account can't see apps. It needs the Apps read role (APPS_READ) on the NAS."
			onretry={app.retry}
		/>
		{#if containers.length > 1 && container}
			<fieldset class="pick">
				<legend class="visually-hidden">Container</legend>
				<div class="segments">
					{#each containers as option (option.id)}
						<label class="segment">
							<input
								class="visually-hidden"
								type="radio"
								name="container"
								value={option.service}
								checked={option.id === container.id}
								onchange={() => choose(option.service)}
							/>
							{option.service}
						</label>
					{/each}
				</div>
			</fieldset>
		{/if}
		{#if container}
			{#key container.id}
				<LogStream app={id} containerId={container.id} />
			{/key}
		{:else if view.kind === 'loading'}
			<div aria-hidden="true" class="skeletons">
				<span class="skeleton line" style:width="100%"></span>
				<span class="skeleton line" style:width="70%"></span>
			</div>
			<p class="visually-hidden">Loading the app…</p>
		{/if}
	</Card>
{/if}

<style>
	p {
		margin: 0;
	}
	.link-button {
		display: inline-flex;
		align-items: center;
		margin-top: var(--space-2);
		text-decoration: none;
	}
	/* Many containers scroll sideways at 360 px rather than wrap. */
	.pick {
		min-width: 0;
		margin: 0 0 var(--space-3);
		padding: 0;
		border: 0;
		overflow-x: auto;
	}
	.pick .segments {
		width: max-content;
		min-width: 100%;
	}
	.pick .segment {
		flex: 1 0 auto;
		padding: 0 var(--space-3);
		white-space: nowrap;
	}
	.skeletons {
		display: grid;
		gap: var(--space-2);
	}
</style>

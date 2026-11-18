<script lang="ts">
	// One app: state, version, web UI links, then its containers, ports and folders. Live through
	// `app.query` events (each one about this app queries it again). Each container links to its
	// live log (#32). Start, stop and restart, confirmed in a dialog, with the job's progress (#33).
	// Update, with the new version and its changelog in the dialog (#34).
	import { untrack } from 'svelte';
	import {
		ACTION_LABELS,
		actionCall,
		actionDialog,
		appActions,
		busyLine,
		canChangeApps,
		type AppAction
	} from '#lib/app-actions.js';
	import { actionRun, dismissAction, finishAction, runAction } from '#lib/app-actions.svelte.js';
	import { appMounts, containerStateLabel, portLabel } from '#lib/app-detail.js';
	import { useAppDetail } from '#lib/app-detail.svelte.js';
	import { parseUpgradeSummary, UPGRADE_SUMMARY, type UpgradeSummary } from '#lib/app-upgrade.js';
	import { stateLabel } from '#lib/apps.js';
	import { cardView } from '#lib/card-state.js';
	import { connection, nas } from '#lib/nas.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { NasError } from '#lib/ws.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';
	import ConfirmDialog from './ConfirmDialog.svelte';
	import EmptyState from './EmptyState.svelte';
	import Icon from './Icon.svelte';
	import JobProgress from './JobProgress.svelte';
	import PageHeader from './PageHeader.svelte';
	import StatusPill from './StatusPill.svelte';

	let { id }: { id: string } = $props();

	// The page keys this component by id, so `id` is fixed for its lifetime.
	// svelte-ignore state_referenced_locally
	const app = useAppDetail(id);

	const detail = $derived(app.detail);
	const view = $derived(
		cardView({ hasData: detail !== null, error: app.error, status: connection.status })
	);
	const mounts = $derived(detail ? appMounts(detail.containers) : []);
	const subtitle = $derived.by(() => {
		if (!detail) return undefined;
		const parts = [detail.app.version ?? 'Unknown version'];
		if (detail.app.custom) parts.push('Custom app');
		return parts.join(' · ');
	});
	const dim = $derived(view.kind === 'ready' && (view.paused || view.stale !== null));

	const ACTION_ICONS = {
		start: 'play',
		stop: 'square',
		restart: 'rotate-cw',
		update: 'circle-arrow-up'
	} as const;
	const run = $derived(actionRun(id));
	// The job, then the wait for the app's new state (or a start/stop from elsewhere).
	const busy = $derived(detail ? busyLine(run, detail.app.state) : null);
	// An action's job ended: its containers were recreated (new ids) and TrueNAS sends no event
	// after they start, so query the app again, once per run.
	$effect(() => {
		if (run?.done) untrack(() => app.reload());
	});
	// The app reached its new state: the done run has nothing more to show.
	$effect(() => {
		if (run?.done && detail && busy === null) finishAction(id);
	});
	// Web UI links only work while the app runs, and not in the middle of an action.
	const [primaryPortal, ...otherPortals] = $derived(
		detail && detail.app.state === 'running' && !busy ? detail.portals : []
	);
	// Actions need the NAS link and the Apps write role; none while the app is changing.
	const canAct = $derived(
		detail !== null && view.kind === 'ready' && !view.paused && canChangeApps(session.user?.roles)
	);
	const actions = $derived(canAct && detail ? appActions(detail.app.state, detail.app.update) : []);
	/** The action waiting for the user's confirmation. */
	let asking = $state<AppAction | null>(null);
	const dialog = $derived(
		asking && detail ? actionDialog(asking, detail.app.name, detail.app.update) : null
	);

	/** The Update dialog's `app.upgrade_summary`: versions and changelog of a new version. */
	type SummaryState =
		| { kind: 'loading' }
		| { kind: 'ready'; summary: UpgradeSummary }
		| { kind: 'error'; message: string };
	let summary = $state.raw<SummaryState | null>(null);
	/** Snapshot the app's host-path folders before an upgrade (TrueNAS default: no). */
	let snapshot = $state(false);
	let summaryRequest = 0;

	function loadSummary() {
		const request = ++summaryRequest;
		summary = { kind: 'loading' };
		nas()
			.call(UPGRADE_SUMMARY, [id])
			.then(
				(raw) => {
					if (request !== summaryRequest) return;
					const parsed = parseUpgradeSummary(raw);
					summary = parsed
						? { kind: 'ready', summary: parsed }
						: { kind: 'error', message: 'The NAS sent an answer truelite does not understand.' };
				},
				(e: unknown) => {
					if (request !== summaryRequest) return;
					summary = {
						kind: 'error',
						message: e instanceof NasError ? e.message : 'Unknown error.'
					};
				}
			);
	}

	function ask(action: AppAction) {
		dismissAction(id);
		asking = action;
		snapshot = false;
		summaryRequest++;
		summary = null;
		// Only a new version has a summary: TrueNAS refuses it for image-only updates.
		if (action === 'update' && detail?.app.update === 'version') loadSummary();
	}

	function confirmAction() {
		if (asking && detail) runAction(id, asking, actionCall(asking, detail.app, snapshot));
		asking = null;
	}
</script>

<a class="back" href="/apps"><Icon name="arrow-left" size={16} />Apps</a>

{#snippet headerActions()}
	{#if detail && detail.containers.length > 0}
		<a class="button-outline link-button" href="/apps/{encodeURIComponent(id)}/logs">
			<Icon name="scroll-text" size={16} />Logs
		</a>
	{/if}
	{#if primaryPortal}
		<a class="button-primary" href={primaryPortal.url} target="_blank" rel="noopener noreferrer">
			Open {primaryPortal.name}
			<Icon name="external-link" size={16} />
		</a>
	{/if}
{/snippet}

<PageHeader
	title={detail?.app.name ?? id}
	{subtitle}
	loading={view.kind === 'loading'}
	actions={primaryPortal || (detail && detail.containers.length > 0) ? headerActions : undefined}
/>

{#if app.missing}
	<EmptyState
		icon="box"
		title="App not found"
		text="There is no app named “{id}” on the NAS. It may have been deleted."
	>
		{#snippet action()}<a class="button-outline link-button" href="/apps">See all apps</a>{/snippet}
	</EmptyState>
{:else}
	{#snippet badge()}<CardBadge {view} />{/snippet}
	<div class={['page', { dim }]}>
		<Card title="Status" icon="info" control={badge}>
			<CardNotice
				{view}
				what="The app"
				denied="Your account can't see apps. It needs the Apps read role (APPS_READ) on the NAS."
				onretry={app.retry}
			/>
			{#if detail}
				<div class="pills">
					<StatusPill {...stateLabel(detail.app.state)} />
					{#if detail.app.update}<StatusPill label="Update available" tone="warning" />{/if}
				</div>
				{#if detail.description}<p class="description">{detail.description}</p>{/if}
				{#if canAct && detail.app.update && detail.app.state === 'stopped'}
					<p class="context">Start the app to update it.</p>
				{/if}
				{#if busy}
					<JobProgress progress={busy.progress} label={busy.label} />
				{:else if actions.length > 0}
					<div class="app-actions">
						{#each actions as action (action)}
							<button type="button" class="button-outline action" onclick={() => ask(action)}>
								<Icon name={ACTION_ICONS[action]} size={16} />{ACTION_LABELS[action]}
							</button>
						{/each}
					</div>
				{/if}
				{#if run?.error}
					<div class="action-error" role="alert">
						<Icon name="circle-alert" size={16} />
						<p>{run.error}</p>
						<button
							type="button"
							class="dismiss"
							aria-label="Dismiss"
							onclick={() => dismissAction(id)}
						>
							×
						</button>
					</div>
				{/if}
				{#if otherPortals.length > 0}
					<ul class="portals">
						{#each otherPortals as portal (portal.url)}
							<li>
								<a href={portal.url} target="_blank" rel="noopener noreferrer">
									{portal.name}<Icon name="external-link" size={12} />
								</a>
							</li>
						{/each}
					</ul>
				{/if}
			{:else if view.kind === 'loading'}
				<div aria-hidden="true" class="skeletons">
					<span class="skeleton line" style:width="5rem"></span>
					<span class="skeleton line" style:width="100%"></span>
					<span class="skeleton line" style:width="70%"></span>
				</div>
				<p class="visually-hidden">Loading the app…</p>
			{/if}
		</Card>

		{#if detail}
			<Card title="Containers" icon="box">
				{#if detail.containers.length === 0}
					<p class="context">
						{detail.app.state === 'stopped'
							? 'No containers while the app is stopped.'
							: 'No containers right now.'}
					</p>
				{:else}
					<ul class="rows">
						{#each detail.containers as container (container.id)}
							<li>
								<div class="text">
									<span class="name">{container.service}</span>
									{#if container.image}<span class="mono">{container.image}</span>{/if}
								</div>
								<StatusPill {...containerStateLabel(container.state)} />
								<a
									class="logs"
									href="/apps/{encodeURIComponent(id)}/logs?service={encodeURIComponent(
										container.service
									)}"
									aria-label="Logs of {container.service}"
								>
									<Icon name="scroll-text" size={16} />
								</a>
							</li>
						{/each}
					</ul>
				{/if}
			</Card>

			{#if detail.ports.length > 0}
				<Card title="Ports" icon="network">
					<ul class="rows">
						{#each detail.ports as port (`${port.containerPort}/${port.protocol}`)}
							<li>
								<span class="mono">{portLabel(port)}</span>
								{#if port.hostPorts.length === 0}<span class="context">Not published</span>{/if}
							</li>
						{/each}
					</ul>
					<p class="context">NAS port → container port</p>
				</Card>
			{/if}

			{#if mounts.length > 0}
				<Card title="Folders" icon="hard-drive">
					<ul class="rows">
						{#each mounts as mount (`${mount.source}\n${mount.destination}`)}
							<li>
								<div class="text">
									<span class="mono">{mount.source}</span>
									<span class="mono secondary">→ {mount.destination}</span>
								</div>
								{#if mount.readOnly}<StatusPill label="Read-only" />{/if}
							</li>
						{/each}
					</ul>
					<p class="context">Folder on the NAS → path inside the app</p>
				</Card>
			{/if}
		{/if}
	</div>
{/if}

{#if dialog}
	<ConfirmDialog
		{...dialog}
		busy={asking === 'update' && summary?.kind === 'loading'}
		onconfirm={confirmAction}
		oncancel={() => (asking = null)}
	>
		{#if asking === 'update' && detail?.app.update === 'version'}
			<div class="update">
				{#if summary?.kind === 'ready'}
					<p class="versions">
						<span class="mono">{detail.app.version ?? 'Current version'}</span>
						<Icon name="arrow-right" size={16} />
						<span class="mono">{summary.summary.to ?? 'Latest version'}</span>
					</p>
					{#if summary.summary.changelog}
						<details>
							<summary>What's new</summary>
							<p class="changelog">{summary.summary.changelog}</p>
						</details>
					{/if}
				{:else if summary?.kind === 'error'}
					<p class="context">
						Couldn't check the new version: {summary.message} You can still update.
					</p>
				{:else}
					<span aria-hidden="true" class="skeleton line" style:width="10rem"></span>
					<p class="visually-hidden">Checking the new version…</p>
				{/if}
				<label class="check">
					<input type="checkbox" bind:checked={snapshot} />
					Snapshot the app's folders first
				</label>
			</div>
		{/if}
	</ConfirmDialog>
{/if}

<style>
	p {
		margin: 0;
	}
	.back {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: 2rem;
		margin-bottom: var(--space-1);
		color: var(--text-secondary);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		text-decoration: none;
	}
	.back:hover {
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.back {
			min-height: var(--touch);
		}
	}
	.link-button {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		text-decoration: none;
	}
	:global(.empty) .link-button {
		margin-top: var(--space-2);
	}
	.logs {
		display: inline-flex;
		flex: none;
		align-items: center;
		justify-content: center;
		width: var(--touch);
		height: var(--touch);
		margin: calc(-1 * var(--space-2)) calc(-1 * var(--space-3)) calc(-1 * var(--space-2)) 0;
		border-radius: var(--radius-md);
		color: var(--text-secondary);
	}
	.logs:hover {
		color: var(--text-primary);
		background: var(--container-inset);
	}

	.page {
		display: grid;
		gap: var(--space-4);
	}
	@media (min-width: 1024px) {
		.page {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			align-items: start;
		}
	}
	/* Paused or not live: the numbers stay, dimmed; the notice in the Status card says why. */
	.dim > :global(:not(:first-child)) {
		opacity: 0.5;
	}

	.pills {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
	}
	.description {
		display: -webkit-box;
		overflow: hidden;
		-webkit-box-orient: vertical;
		-webkit-line-clamp: 3;
		line-clamp: 3;
		color: var(--text-secondary);
	}
	.portals {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-3);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.portals a {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: 2rem;
		color: var(--text-primary);
		font-weight: var(--weight-medium);
	}
	.app-actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.action {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
	.action-error {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-md);
		background: var(--destructive-tint);
		color: var(--destructive);
		font-size: var(--text-sm);
	}
	.action-error > :global(svg) {
		flex: none;
		margin-top: 0.125rem;
	}
	.action-error p {
		flex: 1;
		color: var(--text-primary);
	}
	.dismiss {
		flex: none;
		width: 1.5rem;
		height: 1.5rem;
		margin: -0.125rem -0.25rem 0 0;
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text-secondary);
		font: inherit;
		font-size: var(--text-base);
		cursor: pointer;
	}
	.dismiss:hover {
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.dismiss {
			width: var(--touch);
			height: var(--touch);
			margin: calc(-1 * var(--space-3)) calc(-1 * var(--space-3)) 0 0;
		}
	}
	.update {
		display: grid;
		gap: var(--space-2);
	}
	.versions {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
		color: var(--text-primary);
	}
	.versions .mono {
		font-size: var(--text-sm);
	}
	details summary {
		min-height: 2rem;
		align-content: center;
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		cursor: pointer;
	}
	.changelog {
		max-height: 12rem;
		overflow-y: auto;
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-md);
		background: var(--container-inset);
		color: var(--text-secondary);
		font-size: var(--text-sm);
		white-space: pre-line;
		overflow-wrap: anywhere;
	}
	.check {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		min-height: 2rem;
		font-size: var(--text-sm);
		cursor: pointer;
	}
	.check input {
		width: 1rem;
		height: 1rem;
		margin: 0;
		accent-color: var(--button-primary-bg);
	}
	@media (pointer: coarse) {
		details summary,
		.check {
			min-height: var(--touch);
		}
	}
	.skeletons {
		display: grid;
		gap: var(--space-2);
	}

	.rows {
		display: grid;
		margin: 0 calc(-1 * var(--space-5));
		padding: 0;
		list-style: none;
	}
	.rows li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: var(--touch);
		padding: var(--space-2) var(--space-5);
		border-top: 1px solid var(--border-subdued);
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
	.mono {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		overflow-wrap: anywhere;
	}
	.secondary,
	.context {
		color: var(--text-secondary);
	}
	.context {
		font-size: var(--text-sm);
	}
</style>

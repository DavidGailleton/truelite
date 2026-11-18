<script lang="ts">
	// Start, stop, restart (#33) and update (#34) one app, on the app page and on each row of the
	// apps list (`compact`: icon buttons). Each action is confirmed in a dialog; Update shows the
	// new version, its changelog and a snapshot option. While an action runs, its progress line
	// replaces the buttons. Runs are kept per app (app-actions.svelte.ts), so a run started on
	// one page shows on the other.
	import {
		ACTION_LABELS,
		actionCall,
		actionDialog,
		appActions,
		busyLine,
		type AppAction
	} from '#lib/app-actions.js';
	import {
		actionRun,
		dismissAction,
		finishAction,
		noteState,
		runAction
	} from '#lib/app-actions.svelte.js';
	import { parseUpgradeSummary, UPGRADE_SUMMARY, type UpgradeSummary } from '#lib/app-upgrade.js';
	import type { App, AppUpdate } from '#lib/apps.js';
	import { nas } from '#lib/nas.svelte.js';
	import { NasError } from '#lib/ws.js';
	import ConfirmDialog from './ConfirmDialog.svelte';
	import Icon from './Icon.svelte';
	import JobProgress from './JobProgress.svelte';

	let {
		app,
		canAct,
		compact = false
	}: {
		app: App;
		/** The NAS link is up and the user has the Apps write role. */
		canAct: boolean;
		/** Icon-only buttons and a smaller progress line, for list rows. */
		compact?: boolean;
	} = $props();

	const ACTION_ICONS = {
		start: 'play',
		stop: 'square',
		restart: 'rotate-cw',
		update: 'circle-arrow-up'
	} as const;

	const run = $derived(actionRun(app.id));
	// The job, then the wait for the app's new state (or a start/stop from elsewhere).
	const busy = $derived(busyLine(run, app.state));
	// The app reached its new state: the done run has nothing more to show.
	$effect(() => {
		if (run?.done && busy === null) finishAction(app.id);
	});
	// A run started on a crashed app ends only once the app has moved (busyLine).
	$effect(() => {
		if (run) noteState(app.id, app.state);
	});
	// None while the app is changing.
	const actions = $derived(canAct ? appActions(app.state, app.update) : []);

	/** The action waiting for the user's confirmation. */
	let asking = $state<AppAction | null>(null);
	/** What an update brought when its dialog opened: confirming sends that, nothing newer. */
	let askedUpdate = $state<AppUpdate | null>(null);
	const dialog = $derived(asking ? actionDialog(asking, app.name, askedUpdate) : null);
	// The dialog no longer matches the app (the link dropped, the app changed state, or another
	// update appeared): close it rather than confirm something the user did not see.
	$effect(() => {
		if (asking && (!actions.includes(asking) || app.update !== askedUpdate)) asking = null;
	});

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
			.call(UPGRADE_SUMMARY, [app.id])
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
		dismissAction(app.id);
		asking = action;
		askedUpdate = app.update;
		snapshot = false;
		summaryRequest++;
		summary = null;
		// Only a new version has a summary: TrueNAS refuses it for image-only updates.
		if (action === 'update' && askedUpdate === 'version') loadSummary();
	}

	function confirmAction() {
		if (asking) {
			const call = actionCall(asking, { id: app.id, update: askedUpdate }, snapshot);
			runAction(app.id, asking, call, app.state);
		}
		asking = null;
	}
</script>

{#if busy || actions.length > 0 || run?.error}
	<div class={['actions', { compact }]}>
		{#if busy}
			<JobProgress progress={busy.progress} label={busy.label} />
		{:else if actions.length > 0}
			<div class="buttons">
				{#each actions as action (action)}
					{#if compact}
						<button
							type="button"
							class="button-outline icon-button"
							aria-label="{ACTION_LABELS[action]} {app.name}"
							title={ACTION_LABELS[action]}
							onclick={() => ask(action)}
						>
							<Icon name={ACTION_ICONS[action]} size={16} />
						</button>
					{:else}
						<button type="button" class="button-outline action" onclick={() => ask(action)}>
							<Icon name={ACTION_ICONS[action]} size={16} />{ACTION_LABELS[action]}
						</button>
					{/if}
				{/each}
			</div>
		{/if}
		{#if run?.error}
			<div class="error" role="alert">
				<Icon name="circle-alert" size={16} />
				<p>{run.error}</p>
				<button
					type="button"
					class="dismiss"
					aria-label="Dismiss"
					onclick={() => dismissAction(app.id)}
				>
					×
				</button>
			</div>
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
		{#if asking === 'update' && askedUpdate === 'version'}
			<div class="update">
				{#if summary?.kind === 'ready'}
					<p class="versions">
						<span class="mono">{app.version ?? 'Current version'}</span>
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
	.actions {
		display: grid;
		gap: var(--space-2);
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}
	.action {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}

	/* List rows: small icon buttons, a short progress line, the error under them. */
	.compact {
		justify-items: end;
		gap: var(--space-1);
		max-width: 18rem;
	}
	.compact .buttons {
		flex-wrap: nowrap;
		gap: var(--space-1);
	}
	.icon-button {
		display: inline-grid;
		place-items: center;
		width: 2.25rem;
		height: 2.25rem;
		padding: 0;
	}
	@media (pointer: coarse) {
		.icon-button {
			width: var(--touch);
			height: var(--touch);
		}
	}
	.compact :global(.job) {
		font-size: var(--text-xs);
		color: var(--text-secondary);
	}

	.error {
		display: flex;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-2) var(--space-3);
		border-radius: var(--radius-md);
		background: var(--destructive-tint);
		color: var(--destructive);
		font-size: var(--text-sm);
	}
	.compact .error {
		font-size: var(--text-xs);
	}
	.error > :global(svg) {
		flex: none;
		margin-top: 0.125rem;
	}
	.error p {
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

	/* The Update dialog's content. */
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
	.mono {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		overflow-wrap: anywhere;
	}
	.context {
		color: var(--text-secondary);
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
</style>

<script lang="ts">
	// Updates card on the System page (#40): installed version, update channel, and either "up to
	// date", the version on offer with its release notes, "restart needed", or why the check
	// failed. Live from `update.status` events; the page calls useUpdateStatus() and
	// useSystemInfo(). Install update (#41): confirmed in a dialog (restarting asks for the NAS
	// name), then the job's progress; a pre-check warning asks again before going on.
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import { powerBusy } from '#lib/power.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { versionLabel } from '#lib/system.js';
	import { system } from '#lib/system.svelte.js';
	import { canInstallUpdates, INSTALL_PERCENT } from '#lib/update.js';
	import { update } from '#lib/update.svelte.js';
	import {
		dismissUpdateRun,
		installUpdate,
		updateBusy,
		updateRun
	} from '#lib/update-run.svelte.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';
	import ConfirmDialog from './ConfirmDialog.svelte';
	import ErrorBanner from './ErrorBanner.svelte';
	import Icon from './Icon.svelte';
	import JobProgress from './JobProgress.svelte';
	import StatusPill from './StatusPill.svelte';

	let { onretry }: { onretry: () => void } = $props();

	const status = $derived(update.status);
	const view = $derived(
		cardView({ hasData: status !== null, error: update.error, status: connection.status })
	);
	const installed = $derived(system.info ? versionLabel(system.info.version) : null);
	const hostname = $derived(system.info?.hostname ?? null);

	const run = $derived(updateRun.run);
	// The NAS name is needed: restarting asks for it to be typed. Nothing installs while a
	// restart or shutdown is under way.
	const canInstall = $derived(
		status?.kind === 'available' &&
			view.kind === 'ready' &&
			!view.paused &&
			hostname !== null &&
			!updateBusy() &&
			!powerBusy() &&
			canInstallUpdates(session.user?.roles)
	);
	const runLabel = $derived(
		run?.phase === 'running' && (run.progress?.percent ?? 0) >= INSTALL_PERCENT
			? 'Installing the update…'
			: 'Downloading the update…'
	);

	/** The version the open dialog offers; confirming installs that one, nothing newer. */
	let asking = $state<string | null>(null);
	/** Restart the NAS once installed (TrueNAS UI default: yes). */
	let reboot = $state(true);
	// The offer changed or the link dropped: don't confirm something the user did not see.
	$effect(() => {
		if (asking !== null && (!canInstall || status?.newVersion !== asking)) asking = null;
	});

	function ask() {
		dismissUpdateRun();
		reboot = true;
		asking = status?.newVersion ?? null;
	}

	function confirmInstall() {
		if (status && asking !== null) installUpdate(status, { reboot });
		asking = null;
	}

	function acceptWarning() {
		if (status && run?.phase === 'warning') {
			installUpdate(status, { reboot: run.reboot, resume: true });
		}
	}
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
						{:else if status.kind === 'restart'}
							<StatusPill label="Restart needed" tone="warning" />
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
			{:else if status.kind === 'restart'}
				<p class="context">
					An update is installed. Restart the NAS to finish, from the Power card.
				</p>
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
			{#if run?.phase === 'running'}
				<JobProgress progress={run.progress} label={runLabel} />
			{:else if run?.phase === 'restarting'}
				<p class="note" role="status">
					Installed. The NAS restarts in a few seconds and is away for a few minutes; log in again
					when it's back.
				</p>
			{:else if run?.phase === 'installed' && status.kind !== 'restart'}
				<p class="note" role="status">Installed. Restart the NAS to finish.</p>
			{:else if run?.phase === 'failed'}
				<ErrorBanner message={run.message} ondismiss={dismissUpdateRun} />
			{/if}
			{#if canInstall}
				<button type="button" class="button-primary install" onclick={ask}>
					<Icon name="circle-arrow-up" size={16} />Install update
				</button>
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

{#if asking !== null && hostname !== null}
	<ConfirmDialog
		title="Install {versionLabel(asking)}?"
		text="The NAS downloads the update, then installs it. Apps and shares keep running until the restart; the NAS is unreachable for a few minutes while it restarts."
		confirm={reboot ? 'Install and restart' : 'Install'}
		destructive={reboot}
		typeToConfirm={reboot && hostname !== null ? hostname : undefined}
		onconfirm={confirmInstall}
		oncancel={() => (asking = null)}
	>
		<label class="check">
			<input type="checkbox" bind:checked={reboot} />
			Restart the NAS when installed
		</label>
	</ConfirmDialog>
{/if}

{#if run?.phase === 'warning'}
	<ConfirmDialog
		title="TrueNAS has a warning about this update"
		text={run.message}
		confirm="Install anyway"
		destructive
		onconfirm={acceptWarning}
		oncancel={dismissUpdateRun}
	/>
{/if}

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
	.note {
		font-size: var(--text-sm);
	}
	.install {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		justify-self: start;
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
		.note {
			font-size: var(--text-sm);
		}
		.install {
			display: inline-flex;
			align-items: center;
			gap: var(--space-1);
			justify-self: start;
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
		}
		.notes-link {
			min-height: var(--touch);
		}
	}
</style>

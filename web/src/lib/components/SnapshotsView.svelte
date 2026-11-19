<script lang="ts">
	// One dataset's snapshots (#37): newest first, each with its age and the space only it
	// holds. "Take snapshot" (named like TrueNAS does, children optional) and Delete (after typing
	// the snapshot's name), each confirmed in a dialog and each needing its own TrueNAS role.
	// Live through `pool.snapshot.query` events; read again when the page comes back.
	import { cardView } from '#lib/card-state.js';
	import { connection } from '#lib/nas.svelte.js';
	import { formatBytes } from '#lib/realtime.js';
	import { session } from '#lib/session.svelte.js';
	import {
		canDeleteSnapshots,
		canTakeSnapshots,
		defaultSnapshotName,
		snapshotNameProblem,
		type Snapshot
	} from '#lib/snapshots.js';
	import { useSnapshots } from '#lib/snapshots.svelte.js';
	import { ago } from '#lib/time.js';
	import Card from './Card.svelte';
	import CardNotice from './CardNotice.svelte';
	import ConfirmDialog from './ConfirmDialog.svelte';
	import Icon from './Icon.svelte';
	import PageHeader from './PageHeader.svelte';

	let { dataset }: { dataset: string } = $props();

	// The page keys this component by dataset, so `dataset` is fixed for its lifetime.
	// svelte-ignore state_referenced_locally
	const { store, retry } = useSnapshots(dataset);

	const list = $derived(store.list);
	const view = $derived(
		cardView({
			hasData: list !== null,
			isEmpty: list?.length === 0,
			error: store.error,
			status: connection.status
		})
	);
	const live = $derived((view.kind === 'ready' || view.kind === 'empty') && !view.paused);
	const dim = $derived(
		(view.kind === 'ready' || view.kind === 'empty') && (view.paused || view.stale !== null)
	);
	const canTake = $derived(live && canTakeSnapshots(session.user?.roles));
	const canDelete = $derived(live && canDeleteSnapshots(session.user?.roles));
	const roleLine = $derived.by(() => {
		if (view.kind !== 'ready' && view.kind !== 'empty') return null;
		const roles = session.user?.roles;
		if (!canTakeSnapshots(roles) && !canDeleteSnapshots(roles)) {
			return 'Your account can only look at snapshots. Taking and deleting them needs the Snapshot write and delete roles on the NAS.';
		}
		if (!canDeleteSnapshots(roles)) {
			return "Your account can't delete snapshots. It needs the Snapshot delete role (SNAPSHOT_DELETE) on the NAS.";
		}
		if (!canTakeSnapshots(roles)) {
			return "Your account can't take snapshots. It needs the Snapshot write role (SNAPSHOT_WRITE) on the NAS.";
		}
		return null;
	});
	const subtitle = $derived(
		list === null ? undefined : `${list.length} ${list.length === 1 ? 'snapshot' : 'snapshots'}`
	);
	// Ages are worked out when the list changes; good enough for minutes-old snapshots.
	const now = $derived(list ? Date.now() : 0);
	const busyText = $derived(
		store.run
			? `${store.run.action === 'take' ? 'Taking' : 'Deleting'} snapshot ${store.run.name}…`
			: null
	);

	/** The Take snapshot dialog's fields; `null` while it is closed. */
	let taking = $state<{ name: string; recursive: boolean } | null>(null);
	const nameProblem = $derived(taking ? snapshotNameProblem(taking.name, list ?? []) : null);
	/** The snapshot waiting for its Delete confirmation. */
	let deleting = $state.raw<Snapshot | null>(null);

	// The link dropped or the role is gone: close the dialog rather than send it later.
	$effect(() => {
		if (!canTake) taking = null;
		if (!canDelete) deleting = null;
	});

	function askTake() {
		store.runError = null;
		taking = { name: defaultSnapshotName(new Date()), recursive: false };
	}

	function confirmTake() {
		if (taking && nameProblem === null) void store.take(taking.name, taking.recursive);
		taking = null;
	}

	function confirmDelete() {
		if (deleting) void store.delete(deleting);
		deleting = null;
	}
</script>

<a class="back" href="/storage"><Icon name="arrow-left" size={16} />Storage</a>

{#snippet headerActions()}
	{#if canTake}
		<button
			type="button"
			class="button-primary take"
			disabled={store.run !== null}
			onclick={askTake}
		>
			<Icon name="plus" size={16} />Take snapshot
		</button>
	{/if}
{/snippet}

<PageHeader
	title="Snapshots"
	{subtitle}
	loading={view.kind === 'loading'}
	actions={canTake ? headerActions : undefined}
/>

<Card title={dataset} icon="camera">
	<CardNotice
		{view}
		what="Snapshots"
		denied="Your account can't see snapshots. It needs the Snapshot read role (SNAPSHOT_READ) on the NAS."
		onretry={retry}
	/>
	{#if busyText}
		<p class="busy" role="status">{busyText}</p>
	{/if}
	{#if store.runError}
		<div class="error" role="alert">
			<Icon name="circle-alert" size={16} />
			<p>{store.runError}</p>
			<button
				type="button"
				class="dismiss"
				aria-label="Dismiss"
				onclick={() => (store.runError = null)}
			>
				×
			</button>
		</div>
	{/if}
	{#if roleLine}
		<p class="note">{roleLine}</p>
	{/if}
	{#if view.kind === 'loading'}
		<div class="placeholder" aria-hidden="true">
			<span class="skeleton line" style:width="12rem"></span>
			<span class="skeleton line" style:width="10rem"></span>
			<span class="skeleton line" style:width="11rem"></span>
		</div>
		<p class="visually-hidden">Loading snapshots…</p>
	{:else if view.kind === 'empty'}
		<p class="note">No snapshots of this dataset yet.</p>
	{:else if view.kind === 'ready' && list}
		<ul class={['rows', { dim }]}>
			{#each list as snap (snap.id)}
				<li>
					<div class="text">
						<span class="name">{snap.name}</span>
						<span class="meta">
							{#if snap.created}
								<time datetime={snap.created.toISOString()} title={snap.created.toLocaleString()}>
									{ago(snap.created, now)}
								</time>
							{:else}
								Unknown date
							{/if}
							{#if snap.used !== null}· uses {formatBytes(snap.used)}{/if}
						</span>
					</div>
					{#if canDelete}
						<button
							type="button"
							class="button-outline icon-button"
							aria-label="Delete {snap.name}"
							title="Delete"
							disabled={store.run !== null}
							onclick={() => {
								store.runError = null;
								deleting = snap;
							}}
						>
							<Icon name="trash-2" size={16} />
						</button>
					{/if}
				</li>
			{/each}
		</ul>
	{/if}
</Card>

{#if taking}
	<ConfirmDialog
		title="Take a snapshot"
		text="A snapshot keeps {dataset} as it is now. It takes no space at first and grows as the data changes."
		confirm="Take snapshot"
		busy={nameProblem !== null}
		onconfirm={confirmTake}
		oncancel={() => (taking = null)}
	>
		<label class="field">
			Name
			<input
				bind:value={taking.name}
				autocomplete="off"
				autocapitalize="off"
				spellcheck="false"
				maxlength={250}
				aria-invalid={nameProblem !== null}
				aria-describedby="snapshot-name-problem"
			/>
			{#if nameProblem}<span class="problem" id="snapshot-name-problem">{nameProblem}</span>{/if}
		</label>
		<label class="check">
			<input type="checkbox" bind:checked={taking.recursive} />
			Include child datasets
		</label>
	</ConfirmDialog>
{/if}

{#if deleting}
	<ConfirmDialog
		title="Delete snapshot {deleting.name}?"
		text={deleting.used !== null && deleting.used > 0
			? `This frees about ${formatBytes(deleting.used)}. The data it kept can't be brought back.`
			: "The data it kept can't be brought back."}
		confirm="Delete"
		destructive
		typeToConfirm={deleting.name}
		onconfirm={confirmDelete}
		oncancel={() => (deleting = null)}
	/>
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
	.take {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
	.take:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.placeholder {
		display: grid;
		gap: var(--space-2);
	}
	.note,
	.busy {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}

	.rows {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.rows li {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: 3rem;
		padding: var(--space-2) 0;
		border-top: 1px solid var(--border-secondary);
	}
	.rows li:first-child {
		border-top: 0;
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
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
	}
	.meta {
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.icon-button {
		display: inline-grid;
		flex: none;
		place-items: center;
		width: 2.25rem;
		height: 2.25rem;
		padding: 0;
	}
	.icon-button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
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

	/* The Take snapshot dialog. */
	.field {
		display: grid;
		gap: var(--space-1);
		font-size: var(--text-sm);
	}
	.field input {
		height: 2.25rem;
		padding: 0 var(--space-3);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-md);
		background: var(--surface-inset);
		color: inherit;
		font: inherit;
	}
	.problem {
		color: var(--destructive);
		font-size: var(--text-xs);
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
		.back,
		.check {
			min-height: var(--touch);
		}
		.icon-button {
			width: var(--touch);
			height: var(--touch);
		}
		.field input {
			height: var(--touch);
		}
	}
</style>

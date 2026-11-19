<script lang="ts">
	// Check now, Pause, Resume and Stop a pool's data check (#38), on its Storage page card. Each
	// is confirmed in a dialog (Stop is destructive). While an action waits for the scan line to
	// show it, a progress line replaces the buttons. Runs are kept per pool (scrub.svelte.ts).
	import type { Scan } from '#lib/pools.js';
	import {
		SCRUB_LABELS,
		SCRUB_RUNNING,
		scrubActions,
		scrubDialog,
		type ScrubAction
	} from '#lib/scrub.js';
	import { dismissScrub, finishScrub, runScrub, scrubRun } from '#lib/scrub.svelte.js';
	import ConfirmDialog from './ConfirmDialog.svelte';
	import ErrorBanner from './ErrorBanner.svelte';
	import Icon from './Icon.svelte';
	import JobProgress from './JobProgress.svelte';

	let {
		name,
		status,
		scan,
		canAct
	}: {
		name: string;
		status: string;
		scan: Scan | null;
		/** The NAS link is up and the user has the Pool write role. */
		canAct: boolean;
	} = $props();

	const ICONS = { start: 'play', resume: 'play', pause: 'pause', stop: 'square' } as const;

	const run = $derived(scrubRun(name));
	const busy = $derived(run && run.error === null ? run : null);
	// The scan line shows what was asked: the run has nothing more to say.
	$effect(() => {
		if (busy) finishScrub(name, scan);
	});
	const actions = $derived(canAct && !busy ? scrubActions(status, scan) : []);

	/** The action waiting for the user's confirmation. */
	let asking = $state<ScrubAction | null>(null);
	const dialog = $derived(asking ? scrubDialog(asking, name) : null);
	// The scan moved on (or the link dropped): don't confirm something the user did not see.
	$effect(() => {
		if (asking && !actions.includes(asking)) asking = null;
	});

	function ask(action: ScrubAction) {
		dismissScrub(name);
		asking = action;
	}

	function confirmAction() {
		if (asking) runScrub(name, asking);
		asking = null;
	}
</script>

{#if busy || actions.length > 0 || run?.error}
	<div class="actions">
		{#if busy}
			<JobProgress progress={busy.progress} label={SCRUB_RUNNING[busy.action]} />
		{:else if actions.length > 0}
			<div class="buttons">
				{#each actions as action (action)}
					<button type="button" class="button-outline action" onclick={() => ask(action)}>
						<Icon name={ICONS[action]} size={16} />{SCRUB_LABELS[action]}
					</button>
				{/each}
			</div>
		{/if}
		{#if run?.error}
			<ErrorBanner message={run.error} ondismiss={() => dismissScrub(name)} />
		{/if}
	</div>
{/if}

{#if dialog}
	<ConfirmDialog {...dialog} onconfirm={confirmAction} oncancel={() => (asking = null)} />
{/if}

<style>
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
</style>

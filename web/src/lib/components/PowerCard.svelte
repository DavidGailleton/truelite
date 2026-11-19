<script lang="ts">
	// Power card on the System page (#42): Restart and Shut down the NAS. Each is confirmed in a
	// dialog where the NAS name must be typed. Full admin only. The NAS answers, or the link just
	// drops: either way the card then says it is going down. The page calls useSystemInfo().
	import { connection } from '#lib/nas.svelte.js';
	import { canPower, POWER_TEXT, type PowerAction } from '#lib/power.js';
	import { dismissPower, power, powerBusy, startPower } from '#lib/power.svelte.js';
	import { session } from '#lib/session.svelte.js';
	import { system } from '#lib/system.svelte.js';
	import { updateBusy } from '#lib/update-run.svelte.js';
	import ConfirmDialog from './ConfirmDialog.svelte';
	import Card from './Card.svelte';
	import ErrorBanner from './ErrorBanner.svelte';
	import Icon from './Icon.svelte';

	const hostname = $derived(system.info?.hostname ?? null);
	const run = $derived(power.run);
	const allowed = $derived(canPower(session.user?.roles));
	const disabled = $derived(
		hostname === null || connection.status !== 'up' || powerBusy() || updateBusy()
	);

	/** The action the open dialog asks about. */
	let asking = $state<PowerAction | null>(null);
	// The link dropped or something else started: don't confirm what the user can't do now.
	$effect(() => {
		if (asking !== null && disabled) asking = null;
	});

	function ask(action: PowerAction) {
		dismissPower();
		asking = action;
	}

	function confirm() {
		if (asking !== null) startPower(asking);
		asking = null;
	}
</script>

<Card title="Power" icon="power">
	<div class="body">
		{#if run?.phase === 'running'}
			<p class="note" role="status">Sending to the NAS…</p>
		{:else if run?.phase === 'going-down'}
			<p class="note" role="status">{POWER_TEXT[run.action].goingDown}</p>
		{:else}
			<p class="context">Restart or shut down the NAS. Apps, shares and VMs stop meanwhile.</p>
			{#if run?.phase === 'failed'}
				<ErrorBanner message={run.message} ondismiss={dismissPower} />
			{/if}
			{#if allowed}
				<div class="buttons">
					<button
						type="button"
						class="button-outline action"
						{disabled}
						onclick={() => ask('reboot')}
					>
						<Icon name="rotate-cw" size={16} />{POWER_TEXT.reboot.button}
					</button>
					<button
						type="button"
						class="button-outline action danger"
						{disabled}
						onclick={() => ask('shutdown')}
					>
						<Icon name="power" size={16} />{POWER_TEXT.shutdown.button}
					</button>
				</div>
			{:else}
				<p class="context">Restarting or shutting down needs a full admin account.</p>
			{/if}
		{/if}
	</div>
</Card>

{#if asking !== null && hostname !== null}
	<ConfirmDialog
		title="{POWER_TEXT[asking].title} {hostname}?"
		text={POWER_TEXT[asking].text}
		confirm={POWER_TEXT[asking].button}
		destructive
		typeToConfirm={hostname}
		onconfirm={confirm}
		oncancel={() => (asking = null)}
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
	.note {
		font-size: var(--text-sm);
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
	.action:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	.danger {
		color: var(--destructive);
	}
</style>

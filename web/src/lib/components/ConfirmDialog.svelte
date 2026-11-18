<script lang="ts">
	// Asks before an action (ui-style: every action is confirmed). A native modal `<dialog>`:
	// focus is trapped, Esc cancels, the first control (Cancel, or the name field) gets focus.
	// `typeToConfirm` makes the user type a name first (deletes, reboot: phases 5–6).
	// `children` adds content under the text (the Update dialog's versions and options); `busy`
	// holds the confirm button while that content loads.
	import type { Snippet } from 'svelte';

	let {
		title,
		text,
		confirm,
		destructive = false,
		typeToConfirm,
		busy = false,
		children,
		onconfirm,
		oncancel
	}: {
		title: string;
		text: string;
		confirm: string;
		destructive?: boolean;
		typeToConfirm?: string;
		busy?: boolean;
		children?: Snippet;
		onconfirm: () => void;
		oncancel: () => void;
	} = $props();

	let typed = $state('');
	const ready = $derived(!busy && (typeToConfirm === undefined || typed === typeToConfirm));
	let confirmed = false;

	function open(dialog: HTMLDialogElement) {
		dialog.showModal();
	}

	function onclose() {
		if (!confirmed) oncancel();
	}

	function onsubmit(event: SubmitEvent) {
		if (!ready) {
			event.preventDefault();
			return;
		}
		// `method="dialog"` closes it; `close` follows.
		confirmed = true;
		onconfirm();
	}
</script>

<dialog {@attach open} {onclose} aria-labelledby="confirm-title" aria-describedby="confirm-text">
	<form method="dialog" {onsubmit}>
		<h2 id="confirm-title">{title}</h2>
		<p id="confirm-text">{text}</p>
		{@render children?.()}
		{#if typeToConfirm !== undefined}
			<label>
				Type <strong>{typeToConfirm}</strong> to confirm
				<input bind:value={typed} autocomplete="off" autocapitalize="off" spellcheck="false" />
			</label>
		{/if}
		<div class="buttons">
			<button
				type="button"
				class="button-outline"
				onclick={(e) => e.currentTarget.closest('dialog')?.close()}
			>
				Cancel
			</button>
			<button
				type="submit"
				class={[destructive ? 'button-destructive' : 'button-primary']}
				disabled={!ready}
			>
				{confirm}
			</button>
		</div>
	</form>
</dialog>

<style>
	dialog {
		width: min(26rem, calc(100vw - 2 * var(--gutter)));
		padding: var(--space-5);
		border: 0;
		border-radius: var(--radius-lg);
		background: var(--container);
		color: var(--text-primary);
		box-shadow: var(--shadow-ring), var(--shadow-lg);
	}
	dialog::backdrop {
		background: var(--overlay);
	}
	form {
		display: grid;
		gap: var(--space-3);
	}
	h2 {
		margin: 0;
		font-size: var(--text-base);
		font-weight: var(--weight-semibold);
		overflow-wrap: anywhere;
	}
	p {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	label {
		display: grid;
		gap: var(--space-1);
		font-size: var(--text-sm);
	}
	input {
		height: 2.25rem;
		padding: 0 var(--space-3);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-md);
		background: var(--surface-inset);
		color: inherit;
		font: inherit;
	}
	.buttons {
		display: flex;
		flex-wrap: wrap;
		justify-content: flex-end;
		gap: var(--space-2);
		margin-top: var(--space-2);
	}
	button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>

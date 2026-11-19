<script lang="ts">
	// Full-screen wait while the NAS restarts or is off (#43). Shown by the (app) layout while
	// the session lasts, then by the login page, which asks whether the NAS is back.
	import { elapsedText, restartText, restartTitle, type RestartWait } from '#lib/restart.js';
	import Icon from './Icon.svelte';
	import Logo from './Logo.svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	let { wait, onstop }: { wait: RestartWait; onstop: () => void } = $props();

	let now = $state(Date.now());
	$effect(() => {
		const timer = setInterval(() => (now = Date.now()), 30_000);
		return () => clearInterval(timer);
	});

	const title = $derived(restartTitle(wait));
	const elapsed = $derived(elapsedText(now - wait.since));
</script>

<svelte:head>
	<title>{title} · truelite</title>
</svelte:head>

<main class="page">
	<ThemeToggle />
	<div class="body" role="status" aria-live="polite">
		<Logo size={40} />
		<span class={['icon', wait.kind === 'reboot' && 'spin']}>
			<Icon name={wait.kind === 'reboot' ? 'refresh-cw' : 'power'} size={24} />
		</span>
		<h1>{title}</h1>
		<p class="text">{restartText(wait.kind)}</p>
		<p class="since">Started {elapsed} ago.</p>
	</div>
	<button type="button" class="stop" onclick={onstop}>Stop waiting</button>
</main>

<style>
	.page {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: var(--space-6);
		min-height: 100dvh;
		padding: max(var(--space-6), env(safe-area-inset-top))
			max(var(--space-4), env(safe-area-inset-right))
			max(var(--space-6), env(safe-area-inset-bottom))
			max(var(--space-4), env(safe-area-inset-left));
		background: var(--surface);
		text-align: center;
	}
	.body {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--space-3);
		max-width: 24rem;
	}
	.icon {
		display: flex;
		margin-top: var(--space-3);
		color: var(--text-secondary);
	}
	.spin {
		color: var(--warning);
		animation: spin 1.2s linear infinite;
	}
	@media (prefers-reduced-motion: reduce) {
		.spin {
			animation: none;
		}
	}
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
	h1 {
		margin: 0;
		font-size: var(--text-xl);
		font-weight: var(--weight-medium);
		overflow-wrap: anywhere;
	}
	p {
		margin: 0;
	}
	.text {
		color: var(--text-secondary);
	}
	.since {
		color: var(--text-subdued);
		font-size: var(--text-sm);
	}
	.stop {
		min-height: 2.25rem;
		padding: 0 var(--space-3);
		border: 0;
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--text-secondary);
		font-weight: var(--weight-medium);
		cursor: pointer;
	}
	.stop:hover {
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.stop {
			min-height: var(--touch);
		}
	}
</style>

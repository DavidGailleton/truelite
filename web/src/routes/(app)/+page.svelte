<script lang="ts">
	// Temporary live check of the WebSocket client; the dashboard (phase 3) replaces it.
	import { nas, useTopic } from '#lib/nas.svelte.js';

	type Health = { status: string; version: string };
	type SystemInfo = { hostname: string; version: string };
	type Alert = { dismissed: boolean };

	let health = $state<Health | null>(null);
	let healthError = $state<string | null>(null);
	let info = $state<SystemInfo | null>(null);
	let alerts = $state<number | null>(null);
	let nasError = $state<string | null>(null);

	$effect(() => {
		fetch('/api/health')
			.then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
			.then((h: Health) => (health = h))
			.catch((e: Error) => (healthError = e.message));
	});

	async function load() {
		try {
			const [i, list] = await Promise.all([
				nas().call<SystemInfo>('system.info'),
				nas().call<Alert[]>('alert.list')
			]);
			info = i;
			alerts = list.filter((a) => !a.dismissed).length;
			nasError = null;
		} catch (e) {
			nasError = (e as Error).message;
		}
	}

	// Query on subscribe, after every resume or gap, and (alerts being rare) on every change.
	useTopic('alert.list', (message) => {
		if (!('status' in message) || message.status !== 'ended') load();
	});
</script>

<main>
	<h1>{info?.hostname ?? 'truelite'}</h1>
	<p class="muted">A light web interface for TrueNAS.</p>
	{#if health}
		<p class="ok">Server {health.status} · v{health.version}</p>
	{:else if healthError}
		<p class="danger">Server unreachable: {healthError}</p>
	{:else}
		<p class="muted">Checking server…</p>
	{/if}
	{#if nasError}
		<p class="danger">{nasError}</p>
	{:else if info}
		<p>{info.version} · {alerts === 1 ? '1 alert' : `${alerts} alerts`}</p>
	{:else}
		<p class="muted">Connecting to NAS…</p>
	{/if}
</main>

<style>
	main {
		max-width: 40rem;
		margin: 0 auto;
		padding: var(--space-8) var(--space-4);
	}
	h1 {
		margin: 0 0 var(--space-1);
	}
	.muted {
		color: var(--text-muted);
	}
	.ok {
		color: var(--ok);
	}
	.danger {
		color: var(--danger);
	}
</style>

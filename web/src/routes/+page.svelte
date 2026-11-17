<script lang="ts">
	type Health = { status: string; version: string };

	let health = $state<Health | null>(null);
	let error = $state<string | null>(null);

	$effect(() => {
		fetch('/api/health')
			.then((r) => (r.ok ? r.json() : Promise.reject(new Error(`HTTP ${r.status}`))))
			.then((h: Health) => (health = h))
			.catch((e: Error) => (error = e.message));
	});
</script>

<main>
	<h1>truelite</h1>
	<p class="muted">A light web interface for TrueNAS.</p>
	{#if health}
		<p class="ok">Server {health.status} · v{health.version}</p>
	{:else if error}
		<p class="danger">Server unreachable: {error}</p>
	{:else}
		<p class="muted">Checking server…</p>
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

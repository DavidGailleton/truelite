<script lang="ts">
	// Dashboard: what needs attention first (alerts, #27), then system, live stats and storage.
	import LiveStats from '#lib/components/LiveStats.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
</script>

<PageHeader title="Dashboard" />

<div class="grid">
	<LiveStats />
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: var(--space-4);
	}
	@media (min-width: 768px) {
		.grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: var(--space-6);
		}
	}
</style>

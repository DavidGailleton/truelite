<script lang="ts">
	// Dashboard: what needs attention first (alerts, #27), then system, live stats and storage.
	import LiveStats from '#lib/components/LiveStats.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import SystemInfo from '#lib/components/SystemInfo.svelte';
	import { formatUptime, versionLabel } from '#lib/system.js';
	import { system, uptimeNow, useSystemInfo } from '#lib/system.svelte.js';

	useSystemInfo();

	const info = $derived(system.info);
	const uptime = $derived(uptimeNow());
	const subtitle = $derived(
		info && uptime !== null ? `${versionLabel(info.version)} · ${formatUptime(uptime)}` : undefined
	);
</script>

<PageHeader title={info?.hostname ?? 'Dashboard'} {subtitle} />

<div class="grid">
	<LiveStats />
	<SystemInfo />
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

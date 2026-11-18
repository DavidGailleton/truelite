<script lang="ts">
	// Dashboard: what needs attention first (alerts, #27; a pool in trouble), then live stats,
	// storage and system.
	import LiveStats from '#lib/components/LiveStats.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import PoolsCard from '#lib/components/PoolsCard.svelte';
	import SystemInfo from '#lib/components/SystemInfo.svelte';
	import { formatUptime, versionLabel } from '#lib/system.js';
	import { poolRows } from '#lib/pools.js';
	import { pools, usePools } from '#lib/pools.svelte.js';
	import { system, uptimeNow, useSystemInfo } from '#lib/system.svelte.js';

	useSystemInfo();
	const poolTopics = usePools();

	// A pool in trouble goes to the top; otherwise storage follows the live stats.
	const poolTrouble = $derived(
		pools.list !== null &&
			poolRows(pools.list, pools.usage, Date.now()).some((r) => r.health.level !== 'ok')
	);

	const info = $derived(system.info);
	const uptime = $derived(uptimeNow());
	const subtitle = $derived(
		info && uptime !== null ? `${versionLabel(info.version)} · ${formatUptime(uptime)}` : undefined
	);
</script>

<PageHeader title={info?.hostname ?? 'Dashboard'} {subtitle} />

<div class="grid">
	{#if poolTrouble}<PoolsCard onretry={poolTopics.retry} />{/if}
	<LiveStats />
	{#if !poolTrouble}<PoolsCard onretry={poolTopics.retry} />{/if}
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

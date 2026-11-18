<script lang="ts">
	// Dashboard: what needs attention first (alerts, #27; a pool in trouble; a crashed app), then
	// live stats, storage, apps and system.
	import AppsCard from '#lib/components/AppsCard.svelte';
	import LiveStats from '#lib/components/LiveStats.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import PoolsCard from '#lib/components/PoolsCard.svelte';
	import SystemInfo from '#lib/components/SystemInfo.svelte';
	import { summarize } from '#lib/apps.js';
	import { apps, useApps } from '#lib/apps.svelte.js';
	import { formatUptime, versionLabel } from '#lib/system.js';
	import { poolRows } from '#lib/pools.js';
	import { pools, usePools } from '#lib/pools.svelte.js';
	import { system, uptimeNow, useSystemInfo } from '#lib/system.svelte.js';

	useSystemInfo();
	const poolTopics = usePools();
	const appTopic = useApps();

	// A pool in trouble goes to the top; otherwise storage follows the live stats.
	const poolTrouble = $derived(
		pools.list !== null &&
			poolRows(pools.list, pools.usage, Date.now()).some((r) => r.health.level !== 'ok')
	);

	// A crashed app goes to the top too.
	const appTrouble = $derived(apps.list !== null && summarize(apps.list).crashed.length > 0);

	const info = $derived(system.info);
	const uptime = $derived(uptimeNow());
	const subtitle = $derived(
		info && uptime !== null ? `${versionLabel(info.version)} · ${formatUptime(uptime)}` : undefined
	);
</script>

<PageHeader title={info?.hostname ?? 'Dashboard'} {subtitle} />

<div class="grid">
	{#if poolTrouble}<PoolsCard onretry={poolTopics.retry} />{/if}
	{#if appTrouble}<AppsCard onretry={appTopic.retry} />{/if}
	<LiveStats />
	{#if !poolTrouble}<PoolsCard onretry={poolTopics.retry} />{/if}
	{#if !appTrouble}<AppsCard onretry={appTopic.retry} />{/if}
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

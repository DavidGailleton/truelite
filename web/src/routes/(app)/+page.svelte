<script lang="ts">
	// Dashboard: what needs attention first (warning or critical alerts, a pool in trouble, a
	// crashed app), then live stats, storage, apps, system and a quiet alerts card.
	// Each card is rendered once and moved with CSS `order`, so moving keeps its state.
	import AlertsCard from '#lib/components/AlertsCard.svelte';
	import AppsCard from '#lib/components/AppsCard.svelte';
	import LiveStats from '#lib/components/LiveStats.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import PoolsCard from '#lib/components/PoolsCard.svelte';
	import SystemInfo from '#lib/components/SystemInfo.svelte';
	import { alertSummary, useAlerts } from '#lib/alerts.svelte.js';
	import { appSummary, useApps } from '#lib/apps.svelte.js';
	import { formatUptime, versionLabel } from '#lib/system.js';
	import { poolRowsNow, usePools } from '#lib/pools.svelte.js';
	import { system, uptimeNow, useSystemInfo } from '#lib/system.svelte.js';

	useSystemInfo();
	const poolTopics = usePools();
	const appTopic = useApps();
	const alertTopic = useAlerts();

	// A warning or critical alert goes first: it often explains the pool or app below it.
	const worstAlert = $derived(alertSummary()?.worst ?? null);
	const alertTrouble = $derived(worstAlert === 'critical' || worstAlert === 'warning');

	// A pool in trouble goes to the top; otherwise storage follows the live stats.
	const poolTrouble = $derived(poolRowsNow()?.some((r) => r.health.level !== 'ok') ?? false);

	// A crashed app goes to the top too.
	const appTrouble = $derived((appSummary()?.crashed.length ?? 0) > 0);

	const info = $derived(system.info);
	const uptime = $derived(uptimeNow());
	const subtitle = $derived(
		info && uptime !== null ? `${versionLabel(info.version)} · ${formatUptime(uptime)}` : undefined
	);
</script>

<PageHeader title={info?.hostname ?? 'Dashboard'} {subtitle} loading={!info && !system.error} />

<div class="grid">
	<div class={['slot', { wide: alertTrouble }]} style:order={alertTrouble ? 0 : 7}>
		<AlertsCard onretry={alertTopic.retry} />
	</div>
	<div class="slot" style:order={poolTrouble ? 1 : 4}>
		<PoolsCard onretry={poolTopics.retry} />
	</div>
	<div class="slot" style:order={appTrouble ? 2 : 5}>
		<AppsCard onretry={appTopic.retry} />
	</div>
	<div class="slot" style:order={3}><LiveStats /></div>
	<div class="slot" style:order={6}><SystemInfo /></div>
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
		.wide {
			grid-column: 1 / -1;
		}
	}
	.slot {
		display: grid;
		min-width: 0;
	}
</style>

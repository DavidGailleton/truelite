<script lang="ts">
	// Live CPU, memory and network cards, fed by the `reporting.realtime` topic (one sample every
	// 2 s, reshaped by the server). Renders three cards side by side in the parent's grid.
	import { cardView } from '#lib/card-state.js';
	import { connection, useTopic } from '#lib/nas.svelte.js';
	import {
		REALTIME_TOPIC,
		appendSample,
		formatBytes,
		formatPercent,
		formatRate,
		memorySplit,
		parseSample,
		pickInterface,
		type Sample
	} from '#lib/realtime.js';
	import type { ErrorBody, TopicMessage } from '#lib/ws.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';
	import Sparkline from './Sparkline.svelte';

	const IFACE_KEY = 'truelite.net-iface';
	const WINDOW = 'last 2 minutes';

	let history = $state.raw<Sample[]>([]);
	let failure = $state<ErrorBody | null>(null);
	let chosen = $state<string | null>(load());

	function load(): string | null {
		try {
			return localStorage.getItem(IFACE_KEY);
		} catch {
			return null;
		}
	}

	function choose(name: string) {
		chosen = name;
		try {
			localStorage.setItem(IFACE_KEY, name);
		} catch {
			// Private mode or blocked storage: the choice lasts for this page only.
		}
	}

	function onMessage(m: TopicMessage) {
		if ('data' in m) {
			const s = parseSample(m.data.fields);
			if (s) history = appendSample(history, s);
		} else if (m.status === 'subscribed' || m.status === 'resumed') {
			// A new stream (first, after a hidden tab, after a reconnect): don't draw over the gap.
			history = [];
			failure = null;
		} else if (m.status === 'ended') {
			failure = m.error;
		}
		// `missed`: a few samples lost; the next ones keep coming, nothing to do.
	}

	const topic = useTopic(REALTIME_TOPIC, onMessage, { whenVisible: true });

	const latest = $derived(history.at(-1));
	// After the stream ends the last samples stay up, dimmed, with the notice on the CPU card.
	const view = $derived(
		cardView({ hasData: latest !== undefined, error: failure, status: connection.status })
	);
	const dim = $derived(view.kind === 'ready' && (view.paused || view.stale !== null));

	const cpu = $derived(history.map((s) => s.cpu));

	const mem = $derived(latest && memorySplit(latest.mem));
	const servicesPct = $derived(
		history.map((s) => {
			const m = memorySplit(s.mem);
			return m.total > 0 ? (m.services / m.total) * 100 : null;
		})
	);

	const ifaces = $derived(latest ? Object.keys(latest.net).sort() : []);
	const iface = $derived(latest ? pickInterface(latest.net, chosen) : null);
	const net = $derived(iface && latest ? latest.net[iface] : undefined);
	const rx = $derived(history.map((s) => (iface ? (s.net[iface]?.rx ?? null) : null)));
	const tx = $derived(history.map((s) => (iface ? (s.net[iface]?.tx ?? null) : null)));
	const netMax = $derived(Math.max(1, ...rx.map((v) => v ?? 0), ...tx.map((v) => v ?? 0)));

	/** "4 to 38%" over the window, for the charts' labels. */
	function range(values: readonly (number | null)[], fmt: (v: number) => string): string {
		const nums = values.filter((v): v is number => v !== null);
		if (nums.length === 0) return 'no data yet';
		return `${fmt(Math.min(...nums))} to ${fmt(Math.max(...nums))}`;
	}

	function speed(mbps: number | null): string {
		if (mbps === null) return '';
		return mbps >= 1000 ? ` · ${mbps / 1000} Gb/s` : ` · ${mbps} Mb/s`;
	}
</script>

{#snippet badge()}<CardBadge {view} />{/snippet}

{#snippet skeleton()}
	<div class="skeleton big" aria-hidden="true"></div>
	<div class="skeleton line" aria-hidden="true"></div>
	<div class="skeleton chart" aria-hidden="true"></div>
	<p class="visually-hidden">Loading live statistics…</p>
{/snippet}

{#snippet notice()}
	<CardNotice
		{view}
		what="Live statistics"
		denied="Your account can't see live statistics. It needs the Reporting read role on the NAS."
		onretry={topic.retry}
	/>
{/snippet}

{#if view.kind === 'denied' || view.kind === 'error'}
	<Card title="Live statistics" icon="cpu">{@render notice()}</Card>
{:else}
	<Card title="CPU" icon="cpu" control={badge}>
		{@render notice()}
		{#if latest}
			<div class={{ dim }}>
				<p class="big">{formatPercent(latest.cpu)}</p>
				<p class="context">
					{latest.temp === null ? 'Usage' : `Usage · ${Math.round(latest.temp)} °C`}
				</p>
			</div>
			<Sparkline
				series={[{ values: cpu, tone: 'primary' }]}
				max={100}
				label="CPU usage, {WINDOW}, {range(cpu, formatPercent)}"
			/>
		{:else}
			{@render skeleton()}
		{/if}
	</Card>

	<Card title="Memory" icon="memory-stick" control={badge}>
		{#if latest && mem && mem.total > 0}
			<div class={{ dim }}>
				<p class="big">{formatBytes(mem.services)}</p>
				<p class="context">used by services, of {formatBytes(mem.total)}</p>
			</div>
			<div class="bar" aria-hidden="true">
				<span class="services" style:width="{(mem.services / mem.total) * 100}%"></span>
				<span class="arc" style:width="{(mem.arc / mem.total) * 100}%"></span>
			</div>
			<ul class="legend">
				<li><span class="swatch services"></span>Services {formatBytes(mem.services)}</li>
				<li><span class="swatch arc"></span>ZFS cache {formatBytes(mem.arc)}</li>
				<li><span class="swatch free"></span>Free {formatBytes(mem.free)}</li>
			</ul>
			<Sparkline
				series={[{ values: servicesPct, tone: 'primary' }]}
				max={100}
				label="Memory used by services, {WINDOW}, {range(servicesPct, formatPercent)}"
			/>
		{:else}
			{@render skeleton()}
		{/if}
	</Card>

	<Card title="Network" icon="network">
		{#snippet control()}
			{@render badge()}
			{#if ifaces.length > 1 && iface}
				<label class="visually-hidden" for="net-iface">Interface</label>
				<select id="net-iface" value={iface} onchange={(e) => choose(e.currentTarget.value)}>
					{#each ifaces as name (name)}<option value={name}>{name}</option>{/each}
				</select>
			{/if}
		{/snippet}
		{#if latest && iface && net}
			<div class={{ dim }}>
				<p class="big">
					<span><span class="arrow" aria-hidden="true">↓</span>{formatRate(net.rx)}</span>
					<span><span class="arrow" aria-hidden="true">↑</span>{formatRate(net.tx)}</span>
				</p>
				<p class="context">
					{iface}{net.up ? speed(net.speed) : ' · link down'}
					<span class="visually-hidden">: in {formatRate(net.rx)}, out {formatRate(net.tx)}</span>
				</p>
			</div>
			<Sparkline
				series={[
					{ values: rx, tone: 'primary' },
					{ values: tx, tone: 'secondary' }
				]}
				max={netMax}
				label="Traffic on {iface}, {WINDOW}: in {range(rx, formatRate)}, out {range(
					tx,
					formatRate
				)}"
			/>
			<ul class="legend" aria-hidden="true">
				<li><span class="key solid"></span>In</li>
				<li><span class="key dashed"></span>Out</li>
			</ul>
		{:else if latest}
			<p class="context">No network interface reported.</p>
		{:else}
			{@render skeleton()}
		{/if}
	</Card>
{/if}

<style>
	p {
		margin: 0;
	}
	.big {
		display: flex;
		flex-wrap: wrap;
		gap: 0 var(--space-4);
		font-size: var(--text-3xl);
		font-weight: var(--weight-medium);
		line-height: 1.2;
		font-variant-numeric: tabular-nums;
	}
	.arrow {
		margin-right: var(--space-1);
		color: var(--text-secondary);
	}
	.context {
		color: var(--text-secondary);
	}
	.dim {
		opacity: 0.5;
	}

	.skeleton.big {
		width: 6rem;
		height: 2.25rem;
	}
	.skeleton.line {
		width: 10rem;
	}
	.skeleton.chart {
		height: 3rem;
	}

	.bar {
		display: flex;
		height: 0.5rem;
		overflow: hidden;
		border-radius: var(--radius-full);
		background: var(--chart-track);
	}
	.services {
		background: var(--chart-primary);
	}
	.arc {
		background: var(--chart-secondary);
	}
	.legend {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1) var(--space-4);
		margin: 0;
		padding: 0;
		list-style: none;
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}
	.legend li {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}
	.swatch {
		width: 0.5rem;
		height: 0.5rem;
		border-radius: var(--radius-full);
	}
	.swatch.free {
		background: var(--chart-track);
		box-shadow: var(--shadow-ring);
	}
	.key {
		width: 1rem;
		border-top: 2px solid var(--chart-primary);
	}
	.key.dashed {
		border-top: 2px dashed var(--chart-secondary);
	}

	select {
		max-width: 9rem;
		height: 1.75rem;
		padding: 0 var(--space-2);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-md);
		background: var(--container);
		color: var(--text-primary);
		font: inherit;
		font-size: var(--text-xs);
	}
	@media (pointer: coarse) {
		select {
			height: var(--touch);
		}
	}
</style>

<script lang="ts">
	// Alerts card: the NAS's active alerts, loudest and newest first. The dashboard calls
	// useAlerts(); this renders the state. Messages are plain text, never HTML.
	import { TONE_LABEL, type Tone } from '#lib/alerts.js';
	import { alertSummary, alerts } from '#lib/alerts.svelte.js';
	import { badgeLabel, cardView } from '#lib/card-state.js';
	import type { IconName } from '#lib/icons.js';
	import { connection, pageVisible } from '#lib/nas.svelte.js';
	import { ago } from '#lib/time.js';
	import Card from './Card.svelte';
	import CardBadge from './CardBadge.svelte';
	import CardNotice from './CardNotice.svelte';
	import Icon from './Icon.svelte';

	let { onretry }: { onretry: () => void } = $props();

	/** Rows shown before "Show all". */
	const FIRST = 5;
	const TICK_MS = 60_000;

	const ICON: Record<Tone, IconName> = {
		critical: 'circle-alert',
		warning: 'triangle-alert',
		info: 'info'
	};

	const summary = $derived(alertSummary());
	const view = $derived(
		cardView({
			hasData: summary !== null,
			isEmpty: summary?.active.length === 0,
			error: alerts.error,
			status: connection.status
		})
	);

	let showAll = $state(false);
	const shown = $derived(
		summary ? (showAll ? summary.active : summary.active.slice(0, FIRST)) : []
	);

	const countLabel = $derived.by(() => {
		if (!summary) return '';
		const n = summary.active.length;
		return n === 0 ? 'No alerts' : `${n} ${n === 1 ? 'alert' : 'alerts'}`;
	});

	// "3 h ago" moves on while the page is visible.
	let now = $state(Date.now());
	$effect(() => {
		if (!pageVisible()) return;
		now = Date.now();
		const timer = setInterval(() => (now = Date.now()), TICK_MS);
		return () => clearInterval(timer);
	});
</script>

{#snippet headerControl()}
	{#if badgeLabel(view)}
		<CardBadge {view} />
	{:else if summary}
		<span class="count" role="status">{countLabel}</span>
	{/if}
{/snippet}

<Card title="Alerts" icon="bell" control={headerControl}>
	<CardNotice
		{view}
		what="Alerts"
		denied="Your account can't see alerts. It needs the Alert list read role (ALERT_LIST_READ) on the NAS."
		{onretry}
	/>
	{#if view.kind === 'empty' && summary}
		<div class={['body', { dim: view.paused || view.stale }]}>
			<p class="all-clear">
				<span class="ok-icon"><Icon name="circle-check" size={16} /></span>
				No alerts. Everything looks fine.
			</p>
			{#if summary.dismissed > 0}
				<p class="context">
					{summary.dismissed} dismissed {summary.dismissed === 1 ? 'alert' : 'alerts'} hidden.
				</p>
			{/if}
		</div>
	{:else if view.kind === 'ready' && summary}
		<div class={['body', { dim: view.paused || view.stale }]}>
			<ul class="alerts">
				{#each shown as alert (alert.uuid)}
					<li class={['alert', alert.tone]}>
						<span class="tone-icon"><Icon name={ICON[alert.tone]} size={16} /></span>
						<div class="text">
							<p class="meta">
								<span class="level">{TONE_LABEL[alert.tone]}</span>
								{#if alert.last}
									<span class="when">
										· <time datetime={alert.last.toISOString()}>{ago(alert.last, now)}</time>
									</span>
								{/if}
							</p>
							<p class="message">{alert.message}</p>
						</div>
					</li>
				{/each}
			</ul>
			{#if summary.active.length > FIRST}
				<div>
					<button type="button" class="button-outline" onclick={() => (showAll = !showAll)}>
						{showAll ? 'Show fewer' : `Show all ${summary.active.length}`}
					</button>
				</div>
			{/if}
			{#if summary.dismissed > 0}
				<p class="context">
					{summary.dismissed} dismissed {summary.dismissed === 1 ? 'alert' : 'alerts'} hidden.
				</p>
			{/if}
		</div>
	{:else if view.kind === 'loading'}
		<div class="alerts" aria-hidden="true">
			{#each [0, 1] as i (i)}
				<span class="skeleton row"></span>
			{/each}
		</div>
		<p class="visually-hidden">Loading alerts…</p>
	{/if}
</Card>

<style>
	p {
		margin: 0;
	}
	.context {
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.body {
		display: grid;
		gap: var(--space-3);
	}
	.dim {
		opacity: 0.5;
	}
	.count {
		padding: 0 var(--space-2);
		border-radius: var(--radius-full);
		background: var(--surface-inset);
		color: var(--text-secondary);
		font-size: var(--text-xs);
	}

	.all-clear {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-sm);
	}
	.ok-icon {
		display: inline-flex;
		color: var(--success);
	}

	.alerts {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.alert {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border: 1px solid var(--border-secondary);
		border-radius: var(--radius-lg);
		background: var(--surface-inset);
		color: var(--text-secondary);
	}
	.alert.critical {
		border-color: var(--destructive);
		background: var(--destructive-tint);
		color: var(--destructive);
	}
	.alert.warning {
		border-color: var(--warning);
		background: var(--warning-tint);
		color: var(--warning);
	}
	.tone-icon {
		display: inline-flex;
		padding-top: 0.125rem;
	}
	.text {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}
	.meta {
		font-size: var(--text-xs);
	}
	.level {
		font-weight: var(--weight-medium);
	}
	.when {
		color: var(--text-secondary);
	}
	.message {
		color: var(--text-primary);
		font-size: var(--text-sm);
		white-space: pre-line;
		overflow-wrap: anywhere;
	}

	.skeleton.row {
		display: block;
		height: 3.5rem;
	}
</style>

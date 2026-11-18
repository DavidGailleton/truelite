<script lang="ts">
	// One container's live log: the tail, then new lines. Sticks to the bottom while the reader
	// is there; scrolled up, it stays put and offers "Jump to latest". Keep it keyed by container
	// id: a new container (app restarted) is a new stream.
	import { logTime } from '#lib/logs.js';
	import { useLogs } from '#lib/logs.svelte.js';
	import Icon from './Icon.svelte';

	let { app, containerId }: { app: string; containerId: string } = $props();

	// Keyed by the caller, so both are fixed for this component's lifetime.
	// svelte-ignore state_referenced_locally
	const logs = useLogs(app, containerId);

	let box = $state<HTMLElement>();
	let following = $state(true);

	const denied = $derived(logs.error?.errname === 'EACCES');

	// Follow new lines: after the DOM has them, scroll to the end (one write per batch).
	$effect(() => {
		void logs.entries;
		if (following && box) box.scrollTop = box.scrollHeight;
	});

	function onscroll(): void {
		if (!box) return;
		following = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
	}

	function jump(): void {
		following = true;
		box?.scrollTo({ top: box.scrollHeight });
	}
</script>

{#if logs.state === 'ended'}
	<div class="notice" role="status">
		<p>
			{#if denied}Your account can't read app logs. It needs the Apps read role (APPS_READ) on the
				NAS.{:else}The log stopped: {logs.error?.message ?? 'the NAS ended it'}{/if}
		</p>
		{#if !denied}
			<div>
				<button type="button" class="button-outline" onclick={logs.retry}>Follow again</button>
			</div>
		{/if}
	</div>
{/if}

<div class="frame">
	<!-- Focusable so keyboard users can scroll a log that has no links. -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<div
		class="box"
		bind:this={box}
		{onscroll}
		tabindex="0"
		role="region"
		aria-label="Log of this container"
	>
		{#if logs.state === 'waiting' && logs.entries.length === 0}
			<div aria-hidden="true" class="skeletons">
				<span class="skeleton line" style:width="80%"></span>
				<span class="skeleton line" style:width="60%"></span>
				<span class="skeleton line" style:width="70%"></span>
			</div>
			<p class="visually-hidden">Loading the log…</p>
		{:else if logs.entries.length === 0}
			<p class="empty">{logs.state === 'live' ? 'No log lines yet.' : 'No log lines.'}</p>
		{:else}
			<ol>
				{#each logs.entries as entry (entry.key)}
					{#if entry.skipped}
						<li class="gap">
							Some lines were skipped ({entry.skipped}): the page fell behind.
						</li>
					{:else}
						{@const time = logTime(entry.time)}
						<li>
							{#if time}<time datetime={entry.time}>{time}</time>{/if}<span>{entry.text}</span>
						</li>
					{/if}
				{/each}
			</ol>
		{/if}
	</div>
	{#if !following}
		<button type="button" class="jump button-primary" onclick={jump}>
			<Icon name="arrow-down" size={16} />Jump to latest
		</button>
	{/if}
</div>

<style>
	p {
		margin: 0;
	}
	.notice {
		display: grid;
		gap: var(--space-2);
		margin-bottom: var(--space-3);
		color: var(--text-secondary);
		font-size: var(--text-sm);
	}
	.frame {
		position: relative;
	}
	.box {
		height: max(18rem, calc(100dvh - 20rem));
		overflow: auto;
		overscroll-behavior: contain;
		padding: var(--space-3);
		border-radius: var(--radius-lg);
		background: var(--container-inset);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		line-height: 1.5;
	}
	ol {
		margin: 0;
		padding: 0;
		list-style: none;
	}
	li {
		white-space: pre-wrap;
		overflow-wrap: anywhere;
	}
	time {
		margin-right: var(--space-2);
		color: var(--text-secondary);
		user-select: none;
	}
	.gap {
		margin: var(--space-1) 0;
		color: var(--text-secondary);
		font-family: var(--font-sans);
		font-style: italic;
	}
	.empty {
		color: var(--text-secondary);
	}
	.skeletons {
		display: grid;
		gap: var(--space-2);
	}
	.jump {
		position: absolute;
		right: var(--space-3);
		bottom: var(--space-3);
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
	}
</style>

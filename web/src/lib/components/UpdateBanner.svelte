<script lang="ts">
	// Dashboard banner when a TrueNAS update is available (#40): one line and a link to the
	// System page, where the release notes are. Renders nothing otherwise.
	import { versionLabel } from '#lib/system.js';
	import { availableVersion } from '#lib/update.svelte.js';
	import Icon from './Icon.svelte';

	const version = $derived(availableVersion());
</script>

{#if version}
	<section class="banner" aria-label="TrueNAS update">
		<Icon name="circle-arrow-up" size={20} />
		<p>{versionLabel(version)} is available.</p>
		<a href="/system">See what's new <Icon name="arrow-right" size={16} /></a>
	</section>
{/if}

<style>
	.banner {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2) var(--space-3);
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-xl);
		background: var(--container);
		box-shadow: var(--shadow-border-xs);
	}
	.banner > :global(svg) {
		flex: none;
		color: var(--warning);
	}
	p {
		flex: 1 1 12rem;
		margin: 0;
		font-weight: var(--weight-medium);
	}
	a {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: 2rem;
		font-size: var(--text-sm);
	}
	@media (pointer: coarse) {
		a {
			min-height: var(--touch);
		}
	}
</style>

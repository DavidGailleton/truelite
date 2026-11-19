<script lang="ts">
	// The "← Parent" link above a detail page's header.
	import Icon from './Icon.svelte';

	let { href, label }: { href: string; label: string } = $props();
</script>

<a class="back" {href}><Icon name="arrow-left" size={16} />{label}</a>

<style>
	.back {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		min-height: 2rem;
		margin-bottom: var(--space-1);
		color: var(--text-secondary);
		font-size: var(--text-sm);
		font-weight: var(--weight-medium);
		text-decoration: none;
	}
	.back:hover {
		color: var(--text-primary);
	}
	@media (pointer: coarse) {
		.back {
			min-height: var(--touch);
		}
	}
</style>

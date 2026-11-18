<script lang="ts">
	import { ICONS, type IconName } from '#lib/icons.js';

	let { name, size = 20 }: { name: IconName; size?: 12 | 16 | 20 | 24 } = $props();
</script>

<!-- Decorative: the button or link around it carries the text or the aria-label. -->
<svg
	width={size}
	height={size}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width="2"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden="true"
	focusable="false"
>
	{#each ICONS[name] as d (d)}
		<path {d} />
	{/each}
</svg>

<style>
	svg {
		flex: none;
		display: block;
	}
</style>

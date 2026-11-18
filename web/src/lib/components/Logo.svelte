<script lang="ts">
	let { size = 32 }: { size?: number } = $props();
</script>

<!-- truelite mark: a rounded tile with three drive bays. Same drawing as favicon.svg. -->
<svg class="logo" width={size} height={size} viewBox="0 0 32 32" aria-hidden="true">
	<rect class="tile" width="32" height="32" rx="8" />
	<rect class="bay" x="8" y="8.5" width="16" height="3.5" rx="1.25" />
	<rect class="bay" x="8" y="14.25" width="16" height="3.5" rx="1.25" />
	<rect class="bay" x="8" y="20" width="16" height="3.5" rx="1.25" />
	<circle class="led" cx="21.5" cy="21.75" r="0.9" />
</svg>

<style>
	.logo {
		flex: none;
		display: block;
	}
	.tile {
		fill: var(--button-primary-bg);
	}
	.bay {
		fill: var(--button-primary-text);
	}
	.led {
		fill: var(--success);
	}
</style>

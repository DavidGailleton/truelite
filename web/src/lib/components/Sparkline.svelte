<script lang="ts" module>
	export interface Series {
		values: readonly (number | null)[];
		/** `primary`: solid green line with a fill. `secondary`: dashed gray line, no fill. */
		tone: 'primary' | 'secondary';
	}
</script>

<script lang="ts">
	import { HISTORY_SIZE } from '#lib/realtime.js';
	import { sparkPath } from '#lib/spark.js';

	let {
		series,
		max,
		label,
		capacity = HISTORY_SIZE
	}: {
		series: readonly Series[];
		/** Value at the top edge. */
		max: number;
		/** What the chart shows, in words, for screen readers. */
		label: string;
		capacity?: number;
	} = $props();

	// Drawing units; the SVG stretches to the box, strokes keep their width.
	const W = 100;
	const H = 32;

	const paths = $derived(
		series.map((s) => ({
			tone: s.tone,
			...sparkPath(s.values, { max, width: W, height: H, capacity })
		}))
	);
</script>

<svg viewBox="0 0 {W} {H}" preserveAspectRatio="none" role="img" aria-label={label}>
	{#each paths as p, i (i)}
		{#if p.tone === 'primary' && p.area}
			<path class="area" d={p.area} />
		{/if}
		<path class="line {p.tone}" d={p.line} />
	{/each}
</svg>

<style>
	svg {
		display: block;
		width: 100%;
		height: 3rem;
		overflow: visible;
	}
	.area {
		fill: var(--chart-primary);
		fill-opacity: var(--chart-fill-opacity);
	}
	.line {
		fill: none;
		stroke-width: 1.5;
		stroke-linecap: round;
		stroke-linejoin: round;
		vector-effect: non-scaling-stroke;
	}
	.primary {
		stroke: var(--chart-primary);
	}
	.secondary {
		stroke: var(--chart-secondary);
		stroke-dasharray: 4 3;
	}
</style>

// SVG path data for the sparklines: a line and the area under it. Pure, so it can be unit-tested.

export interface SparkOptions {
	/** Value at the top edge; values above are clipped. 0 or less draws everything at the bottom. */
	max: number;
	width: number;
	height: number;
	/** Points across the full width. The newest value sits at the right edge. */
	capacity: number;
}

const r = (n: number) => Math.round(n * 100) / 100;

/**
 * `line` and `area` path data for `values`, oldest first. `null` or a non-finite value is a
 * gap: the line stops and starts again after it. A lone point is drawn as a dot (needs a
 * round line cap).
 */
export function sparkPath(
	values: readonly (number | null)[],
	{ max, width, height, capacity }: SparkOptions
): { line: string; area: string } {
	const step = capacity > 1 ? width / (capacity - 1) : 0;
	const offset = width - (values.length - 1) * step;
	const y = (v: number) => r(height - (max > 0 ? Math.min(1, Math.max(0, v / max)) : 0) * height);

	const runs: [number, number][][] = [];
	let run: [number, number][] = [];
	values.forEach((v, i) => {
		if (v === null || !Number.isFinite(v)) {
			if (run.length) runs.push(run);
			run = [];
		} else {
			run.push([r(offset + i * step), y(v)]);
		}
	});
	if (run.length) runs.push(run);

	let line = '';
	let area = '';
	for (const pts of runs) {
		const [x0, y0] = pts[0];
		const rest = pts
			.slice(1)
			.map(([x, yy]) => `L${x} ${yy}`)
			.join('');
		line += pts.length === 1 ? `M${x0} ${y0}h0` : `M${x0} ${y0}${rest}`;
		if (pts.length > 1) {
			const xn = pts[pts.length - 1][0];
			area += `M${x0} ${height}L${x0} ${y0}${rest}L${xn} ${height}Z`;
		}
	}
	return { line, area };
}

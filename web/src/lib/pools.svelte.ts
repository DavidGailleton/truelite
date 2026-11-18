// Svelte binding of the pools: the list follows `pool.query` events, scrub and resilver progress
// follows `pool.scan`, and usable space comes from the root datasets, read again when a pool
// changes or the page comes back after a while. No timer polling.
import { untrack } from 'svelte';
import { nas, onNasClose, pageVisible, useTopic } from './nas.svelte';
import {
	DATASET_QUERY,
	POOL_QUERY,
	POOL_SCAN,
	POOL_SELECT,
	applyPoolEvent,
	applyScan,
	parsePools,
	parseRootUsage,
	poolRows,
	rootUsageParams,
	type Pool,
	type PoolRow,
	type RootUsage
} from './pools';
import { NasError, needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

/** Re-read usable space on a pool change at most this often. */
const USAGE_DEBOUNCE_MS = 1_000;
/** Re-read it when the page comes back after this long. */
const USAGE_STALE_MS = 60_000;

export const pools = $state<{
	/** `null` until the first answer. */
	list: Pool[] | null;
	/** Usable space by pool name; a missing pool falls back to raw space. */
	usage: ReadonlyMap<string, RootUsage>;
	/** Why the pool list failed; `null` after a success. */
	error: ErrorBody | null;
	/** The account can't read datasets: raw space is all there is. */
	usageDenied: boolean;
}>({ list: null, usage: new Map(), error: null, usageDenied: false });

let listRequest = 0;
let usageRequest = 0;
let usageAt = 0;
let usageTimer: ReturnType<typeof setTimeout> | undefined;

const rows = $derived(pools.list ? poolRows(pools.list, pools.usage, Date.now()) : null);

/** The pools as the card shows them, worst first; `null` until the first answer. Reactive. */
export function poolRowsNow(): PoolRow[] | null {
	return rows;
}

onNasClose(() => {
	listRequest++;
	usageRequest++;
	clearTimeout(usageTimer);
	usageAt = 0;
	pools.list = null;
	pools.usage = new Map();
	pools.error = null;
	pools.usageDenied = false;
});

/** Queries the pool list, then their usable space. */
export function loadPools(): void {
	const mine = ++listRequest;
	nas()
		.call(POOL_QUERY, [[], { select: [...POOL_SELECT] }])
		.then((raw) => {
			if (mine !== listRequest) return;
			const list = parsePools(raw);
			if (!list) {
				pools.error = { code: 'bad_request', message: "The NAS sent pools truelite can't read." };
				return;
			}
			pools.list = list;
			pools.error = null;
			loadUsage();
		})
		.catch((e: unknown) => {
			if (mine !== listRequest) return;
			const error = shownError(e);
			if (error) pools.error = error;
		});
}

/** Queries the usable space of the known pools' root datasets. */
function loadUsage(): void {
	clearTimeout(usageTimer);
	const names = (pools.list ?? []).map((p) => p.name);
	if (names.length === 0 || pools.usageDenied) return;
	const mine = ++usageRequest;
	nas()
		.call(DATASET_QUERY, rootUsageParams(names))
		.then((raw) => {
			if (mine !== usageRequest) return;
			pools.usage = parseRootUsage(raw);
			usageAt = performance.now();
		})
		.catch((e: unknown) => {
			if (mine !== usageRequest) return;
			// Missing DATASET_READ: show raw space and stop asking. Other failures keep the last
			// numbers; the next pool change or visit asks again.
			if (e instanceof NasError && e.errname === 'EACCES') pools.usageDenied = true;
		});
}

function loadUsageSoon(): void {
	clearTimeout(usageTimer);
	usageTimer = setTimeout(loadUsage, USAGE_DEBOUNCE_MS);
}

function onPoolMessage(message: TopicMessage): void {
	if ('data' in message) {
		// After a failed reload the list may lack events: read it again rather than patch it.
		const next = !pools.error && pools.list && applyPoolEvent(pools.list, message.data);
		if (!next) {
			loadPools();
			return;
		}
		pools.list = next;
		if (message.data.kind !== 'removed') loadUsageSoon();
	} else if (message.status === 'ended') {
		pools.error = message.error;
	} else if (needsRefetch(message)) {
		loadPools();
	}
}

function onScanMessage(message: TopicMessage): void {
	// An `ended` scan topic (no POOL_SCRUB_READ) only loses live progress; the pool list still
	// carries the last scan state.
	if (!('data' in message) || !pools.list) return;
	const next = applyScan(pools.list, message.data.fields);
	if (next !== pools.list) pools.list = [...next];
}

/**
 * Follows the pools while the calling component is mounted. Call from component setup.
 * `retry()` subscribes again after the pool topic ended.
 */
export function usePools(): { retry(): void } {
	const list = useTopic(POOL_QUERY, onPoolMessage);
	const scan = useTopic(POOL_SCAN, onScanMessage);
	$effect(() => {
		if (!pageVisible()) return;
		untrack(() => {
			if (pools.list && performance.now() - usageAt > USAGE_STALE_MS) loadUsage();
		});
	});
	$effect(() => () => clearTimeout(usageTimer));
	return {
		retry: () => {
			pools.error = null;
			list.retry();
			scan.retry();
		}
	};
}

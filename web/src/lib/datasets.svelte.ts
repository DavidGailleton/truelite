// Svelte binding of the dataset tree (Storage page only). The tree is read while the page is
// mounted: again after a `pool.dataset.query` event (created, changed, deleted), after a pool
// change (space moved; TrueNAS sends no event for space alone) and when the page comes back
// after a while. Events are not patched in: the whole list is small once the server trims it.
import { untrack } from 'svelte';
import { DATASET_QUERY, DATASET_TREE_PARAMS, buildTree, type DatasetTree } from './datasets';
import { nas, onNasClose, pageVisible, useTopic } from './nas.svelte';
import { onPoolChange } from './pools.svelte';
import { needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

/** Wait this long after a change before reading again, so a burst costs one read. */
const DEBOUNCE_MS = 1_000;
/** Read again when the page comes back after this long. */
const STALE_MS = 60_000;

class DatasetStore {
	/** `null` until the first answer. Replaced whole on each read, never mutated. */
	tree = $state.raw<DatasetTree | null>(null);
	/** Why the last read failed; `null` after a success. */
	error = $state.raw<ErrorBody | null>(null);
}

export const datasets = new DatasetStore();

/** Mounted pages that show the tree; nothing is read without one. */
let wanted = 0;
let request = 0;
let readAt = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let stopPoolWatch: (() => void) | undefined;

onNasClose(() => {
	request++;
	clearTimeout(timer);
	readAt = 0;
	datasets.tree = null;
	datasets.error = null;
});

/** Reads the whole tree now. */
export function loadDatasets(): void {
	clearTimeout(timer);
	const mine = ++request;
	nas()
		.call(DATASET_QUERY, [...DATASET_TREE_PARAMS])
		.then((raw) => {
			if (mine !== request) return;
			const tree = buildTree(raw);
			if (!tree) {
				datasets.error = {
					code: 'bad_request',
					message: "The NAS sent datasets truelite can't read."
				};
				return;
			}
			datasets.tree = tree;
			datasets.error = null;
			readAt = performance.now();
		})
		.catch((e: unknown) => {
			if (mine !== request) return;
			const error = shownError(e);
			if (error) datasets.error = error;
		});
}

function loadSoon(): void {
	// A missing role stays missing: no read on every pool change.
	if (wanted === 0 || datasets.error?.errname === 'EACCES') return;
	clearTimeout(timer);
	timer = setTimeout(loadDatasets, DEBOUNCE_MS);
}

function onDatasetMessage(message: TopicMessage): void {
	if ('data' in message) loadSoon();
	else if (message.status === 'ended') datasets.error = message.error;
	else if (needsRefetch(message) && wanted > 0) loadDatasets();
}

/**
 * Lets the tree be read until the returned release is called, and reads it again after each
 * pool change. The first read comes with the `pool.dataset.query` subscription (`subscribed`,
 * sent on every mount and after a reconnect). Plain function so tests can drive it without a
 * component.
 */
export function wantDatasets(): () => void {
	if (wanted++ === 0) stopPoolWatch = onPoolChange(loadSoon);
	let released = false;
	return () => {
		if (released) return;
		released = true;
		if (--wanted === 0) {
			clearTimeout(timer);
			stopPoolWatch?.();
			stopPoolWatch = undefined;
		}
	};
}

/**
 * Follows the dataset tree while the calling component is mounted. Call from component setup.
 * `retry()` subscribes and reads again after an error.
 */
export function useDatasets(): { retry(): void } {
	const topic = useTopic(DATASET_QUERY, onDatasetMessage);
	$effect(() => untrack(wantDatasets));
	$effect(() => {
		if (!pageVisible()) return;
		untrack(() => {
			if (datasets.tree && performance.now() - readAt > STALE_MS) loadDatasets();
		});
	});
	return {
		retry: () => {
			datasets.error = null;
			topic.retry();
			loadDatasets();
		}
	};
}

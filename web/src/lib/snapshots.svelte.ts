// Svelte binding of one dataset's snapshots (the snapshots page). The list is read when its
// subscription starts and when the page comes back after a while: periodic snapshot tasks send
// no event. Snapshots taken or deleted (here or in the TrueNAS UI) come as `pool.snapshot.query`
// events and are patched in. One store per page, not shared: each page shows one dataset.
import { untrack } from 'svelte';
import { nas, pageVisible, useTopic } from './nas.svelte';
import {
	SNAPSHOT_CREATE,
	SNAPSHOT_DELETE,
	SNAPSHOT_QUERY,
	applySnapshotEvent,
	parseSnapshot,
	parseSnapshots,
	snapshotError,
	snapshotQueryParams,
	withSnapshot,
	type Snapshot,
	type SnapshotAction
} from './snapshots';
import { NasError, needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

/** Read again when the page comes back after this long. */
const STALE_MS = 60_000;

/** What the user asked for and is waiting on. */
export interface SnapshotRun {
	action: SnapshotAction;
	/** The snapshot's name. */
	name: string;
}

export class SnapshotStore {
	readonly dataset: string;
	/** Newest first; `null` until the first answer. Replaced whole, never mutated. */
	list = $state.raw<Snapshot[] | null>(null);
	/** Why the last read failed; `null` after a success. */
	error = $state.raw<ErrorBody | null>(null);
	/** The take or delete in flight. */
	run = $state.raw<SnapshotRun | null>(null);
	/** Why the last take or delete failed, in plain words. */
	runError = $state<string | null>(null);

	#request = 0;
	#readAt = 0;

	constructor(dataset: string) {
		this.dataset = dataset;
	}

	/** Reads the whole list now. */
	load(): void {
		const mine = ++this.#request;
		nas()
			.call(SNAPSHOT_QUERY, snapshotQueryParams(this.dataset))
			.then((raw) => {
				if (mine !== this.#request) return;
				const list = parseSnapshots(raw, this.dataset);
				if (!list) {
					this.error = {
						code: 'bad_request',
						message: "The NAS sent snapshots truelite can't read."
					};
					return;
				}
				this.list = list;
				this.error = null;
				this.#readAt = performance.now();
			})
			.catch((e: unknown) => {
				if (mine !== this.#request) return;
				const error = shownError(e);
				if (error) this.error = error;
			});
	}

	/** Reads again if the list is older than {@link STALE_MS}. */
	loadIfStale(now = performance.now()): void {
		if (this.list && now - this.#readAt > STALE_MS) this.load();
	}

	onMessage = (message: TopicMessage): void => {
		if ('data' in message) {
			if (!this.list) return;
			const next = applySnapshotEvent(this.list, message.data, this.dataset);
			if (next) this.list = next;
		} else if (message.status === 'ended') this.error = message.error;
		else if (needsRefetch(message)) this.load();
	};

	/** Takes a snapshot named `name` (of the children too with `recursive`). */
	take(name: string, recursive: boolean): Promise<void> {
		return this.#act({ action: 'take', name }, async () => {
			const raw = await nas().call(SNAPSHOT_CREATE, [{ dataset: this.dataset, name, recursive }]);
			const snap = parseSnapshot(raw);
			if (snap && this.list) this.list = withSnapshot(this.list, snap);
		});
	}

	/** Deletes `snap` (this one only, never its children's). */
	delete(snap: Snapshot): Promise<void> {
		return this.#act({ action: 'delete', name: snap.name }, async () => {
			await nas().call(SNAPSHOT_DELETE, [snap.id]);
			if (this.list) this.list = this.list.filter((s) => s.id !== snap.id);
		});
	}

	async #act(run: SnapshotRun, work: () => Promise<void>): Promise<void> {
		if (this.run) return;
		this.run = run;
		this.runError = null;
		try {
			await work();
		} catch (e) {
			const error: ErrorBody =
				e instanceof NasError
					? { code: e.code, message: e.message, errname: e.errname }
					: { code: 'bad_request', message: 'Something went wrong.' };
			this.runError = snapshotError(run.action, error);
		} finally {
			this.run = null;
		}
	}
}

/**
 * Follows `dataset`'s snapshots while the calling component is mounted. Call from component
 * setup; the page keys its view by dataset, so the store is the dataset's for its lifetime.
 */
export function useSnapshots(dataset: string): { store: SnapshotStore; retry(): void } {
	const store = new SnapshotStore(dataset);
	const topic = useTopic(SNAPSHOT_QUERY, store.onMessage);
	$effect(() => {
		if (pageVisible()) untrack(() => store.loadIfStale());
	});
	return {
		store,
		retry: () => {
			store.error = null;
			topic.retry();
			store.load();
		}
	};
}

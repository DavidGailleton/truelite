// Svelte binding of the disks page. TrueNAS sends no disk or temperature events, so the page
// reads the list and the temperatures when it opens, after a NAS reconnect and when it comes back
// after a while; temperatures are read again every 5 minutes while the page is on screen (TrueNAS
// caches them that long). One store per page, not shared.
import { untrack } from 'svelte';
import { connection, nas, pageVisible } from './nas.svelte';
import {
	DISK_QUERY,
	DISK_TEMPS,
	DISK_TEMP_AGG,
	TEMPS_PARAMS,
	aggParams,
	diskQueryParams,
	parseDisks,
	parseTempAgg,
	parseTemps,
	type Disk,
	type DiskTemp,
	type TempRange
} from './disks';
import { shownError, type ErrorBody, type Status } from './ws';

/** Read again when the page comes back after this long. */
const STALE_MS = 60_000;
/** Temperatures are read again this often while the page is on screen. */
export const TEMPS_EVERY_MS = 5 * 60_000;

const unreadable = (what: string): ErrorBody => ({
	code: 'bad_request',
	message: `The NAS sent ${what} truelite can't read.`
});

export class DiskStore {
	/** `null` until the first answer. Replaced whole, never mutated. */
	list = $state.raw<Disk[] | null>(null);
	/** Why the last list read failed; `null` after a success. */
	error = $state.raw<ErrorBody | null>(null);
	/** Current temperatures by disk name; `null` until the first answer. */
	temps = $state.raw<ReadonlyMap<string, DiskTemp> | null>(null);
	/** Why the last temperature read failed (`REPORTING_READ` missing, …). */
	tempError = $state.raw<ErrorBody | null>(null);
	/** 7-day lowest and highest temperature by disk name. */
	ranges = $state.raw<ReadonlyMap<string, TempRange> | null>(null);

	#listRequest = 0;
	#tempRequest = 0;
	#aggRequest = 0;
	#readAt = 0;
	#tempsAt = 0;
	#status: Status | null = null;

	/** Reads the list (then the 7-day ranges) and the temperatures now. */
	load(): void {
		this.loadList();
		this.loadTemps();
	}

	loadList(): void {
		const mine = ++this.#listRequest;
		nas()
			.call(DISK_QUERY, diskQueryParams())
			.then((raw) => {
				if (mine !== this.#listRequest) return;
				const list = parseDisks(raw);
				if (!list) {
					this.error = unreadable('disks');
					return;
				}
				this.list = list;
				this.error = null;
				this.#readAt = performance.now();
				if (list.length > 0) this.#loadRanges(list.map((d) => d.name));
			})
			.catch((e: unknown) => {
				if (mine !== this.#listRequest) return;
				const error = shownError(e);
				if (error) this.error = error;
			});
	}

	loadTemps(): void {
		const mine = ++this.#tempRequest;
		nas()
			.call(DISK_TEMPS, [...TEMPS_PARAMS])
			.then((raw) => {
				if (mine !== this.#tempRequest) return;
				const temps = parseTemps(raw);
				if (!temps) {
					this.tempError = unreadable('temperatures');
					return;
				}
				this.temps = temps;
				this.tempError = null;
				this.#tempsAt = performance.now();
			})
			.catch((e: unknown) => {
				if (mine !== this.#tempRequest) return;
				const error = shownError(e);
				if (error) this.tempError = error;
			});
	}

	/** The 7-day ranges are extra: a failure only leaves them out. */
	#loadRanges(names: string[]): void {
		const mine = ++this.#aggRequest;
		nas()
			.call(DISK_TEMP_AGG, aggParams(names))
			.then((raw) => {
				if (mine === this.#aggRequest) this.ranges = parseTempAgg(raw) ?? this.ranges;
			})
			.catch(() => {});
	}

	/** On return to the page: reads again whatever is older than {@link STALE_MS}. */
	loadIfStale(now = performance.now()): void {
		if (this.list && now - this.#readAt > STALE_MS) this.loadList();
		if (this.temps && now - this.#tempsAt > STALE_MS) this.loadTemps();
	}

	/** Reads everything again when the NAS link comes back up after a drop. */
	onStatus(status: Status): void {
		const was = this.#status;
		this.#status = status;
		if (status === 'up' && was !== null && was !== 'up' && this.list) this.load();
	}

	/** Reads the temperatures every {@link TEMPS_EVERY_MS} until the returned stop is called. */
	refreshTemps(): () => void {
		const timer = setInterval(() => this.loadTemps(), TEMPS_EVERY_MS);
		return () => clearInterval(timer);
	}
}

/** Follows the disks while the calling component is mounted. Call from component setup. */
export function useDisks(): { store: DiskStore; retry(): void } {
	const store = new DiskStore();
	store.load();
	$effect(() => {
		const status = connection.status;
		untrack(() => store.onStatus(status));
	});
	$effect(() => {
		if (!pageVisible()) return;
		untrack(() => store.loadIfStale());
		return store.refreshTemps();
	});
	return {
		store,
		retry: () => {
			store.error = null;
			store.tempError = null;
			store.load();
		}
	};
}

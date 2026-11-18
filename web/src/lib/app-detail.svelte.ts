// Svelte binding of one app in detail. The `app.query` event reaches the browser compacted (no
// workloads, `server/src/truenas/apps.rs`), so an event about this app queries it again rather
// than patching it. No timer polling.
import { nas, useTopic } from './nas.svelte';
import { APP_QUERY } from './apps';
import { detailQuery, parseAppDetail, type AppDetail } from './app-detail';
import { needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

class AppDetailStore {
	/** `null` until the first answer, and when the app is missing. */
	detail = $state.raw<AppDetail | null>(null);
	/** The NAS has no app by this name (never installed, or deleted). */
	missing = $state(false);
	/** Why the app failed to load or to follow; `null` after a success. */
	error = $state.raw<ErrorBody | null>(null);

	#request = 0;
	readonly #id: string;

	constructor(id: string) {
		this.#id = id;
	}

	load(): void {
		const mine = ++this.#request;
		nas()
			.call(APP_QUERY, detailQuery(this.#id))
			.then((raw) => {
				if (mine !== this.#request) return;
				if (Array.isArray(raw) && raw.length === 0) {
					this.detail = null;
					this.missing = true;
					this.error = null;
					return;
				}
				const detail = Array.isArray(raw) ? parseAppDetail(raw[0], location.hostname) : null;
				if (!detail) {
					this.error = { code: 'bad_request', message: "The NAS sent an app truelite can't read." };
					return;
				}
				this.detail = detail;
				this.missing = false;
				this.error = null;
			})
			.catch((e: unknown) => {
				if (mine !== this.#request) return;
				const error = shownError(e);
				if (error) this.error = error;
			});
	}

	/** Drops answers still on their way (the page is gone). */
	stop(): void {
		this.#request++;
	}

	onMessage(message: TopicMessage): void {
		if ('data' in message) {
			const { id, kind } = message.data;
			// Another app changed: nothing to do. An unreadable id may be this app: read again.
			if (typeof id === 'string' && id !== this.#id) return;
			if (kind === 'removed' && id === this.#id) {
				this.#request++;
				this.detail = null;
				this.missing = true;
				this.error = null;
			} else {
				this.load();
			}
		} else if (message.status === 'ended') {
			this.error = message.error;
		} else if (needsRefetch(message)) {
			this.load();
		}
	}
}

export type AppDetailView = Pick<AppDetailStore, 'detail' | 'missing' | 'error'> & {
	retry(): void;
};

/**
 * Follows the app named `id` while the calling component is mounted. Call from component setup;
 * a component per app (the page keys it by id) keeps one app's answers out of another's page.
 */
export function useAppDetail(id: string): AppDetailView {
	const store = new AppDetailStore(id);
	const topic = useTopic(APP_QUERY, (message) => store.onMessage(message));
	$effect(() => () => store.stop());
	return {
		get detail() {
			return store.detail;
		},
		get missing() {
			return store.missing;
		},
		get error() {
			return store.error;
		},
		retry: () => {
			store.error = null;
			topic.retry();
		}
	};
}

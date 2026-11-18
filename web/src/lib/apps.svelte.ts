// Svelte binding of the apps summary: the list follows `app.query` events, and is queried again
// on (re)subscription. No timer polling.
import { nas, useTopic } from './nas.svelte';
import { APP_QUERY, APP_SELECT, applyAppEvent, parseApps, type App } from './apps';
import { needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

export const apps = $state<{
	/** `null` until the first answer. */
	list: App[] | null;
	/** Why the app list failed; `null` after a success. */
	error: ErrorBody | null;
}>({ list: null, error: null });

let request = 0;

/** Queries the app list. */
export function loadApps(): void {
	const mine = ++request;
	nas()
		.call(APP_QUERY, [[], { select: [...APP_SELECT] }])
		.then((raw) => {
			if (mine !== request) return;
			const list = parseApps(raw);
			if (!list) {
				apps.error = { code: 'bad_request', message: "The NAS sent apps truelite can't read." };
				return;
			}
			apps.list = list;
			apps.error = null;
		})
		.catch((e: unknown) => {
			if (mine !== request) return;
			const error = shownError(e);
			if (error) apps.error = error;
		});
}

function onAppMessage(message: TopicMessage): void {
	if ('data' in message) {
		const next = apps.list && applyAppEvent(apps.list, message.data);
		if (next) apps.list = next;
		else loadApps();
	} else if (message.status === 'ended') {
		apps.error = message.error;
	} else if (needsRefetch(message)) {
		loadApps();
	}
}

/**
 * Follows the apps while the calling component is mounted. Call from component setup.
 * `retry()` subscribes again after the topic ended.
 */
export function useApps(): { retry(): void } {
	const topic = useTopic(APP_QUERY, onAppMessage);
	return {
		retry: () => {
			apps.error = null;
			topic.retry();
		}
	};
}

// Svelte binding of the apps summary: the list follows `app.query` events, and is queried again
// on (re)subscription and when an app action's job ends (#57: an earlier app of several updates
// may get no event, or a stale one, after its job). No timer polling.
//
// At most one `app.query` is in flight. An event or a reload asked while it is in flight makes
// its answer stale (the NAS may have built it before the change): that answer is dropped and the
// list is queried again, so an older answer never overwrites a newer event.
import { onActionDone } from './app-actions.svelte';
import { nas, onNasClose, useTopic } from './nas.svelte';
import {
	APP_QUERY,
	APP_SELECT,
	applyAppEvent,
	parseApps,
	summarize,
	type App,
	type AppSummary
} from './apps';
import { needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

export const apps = $state<{
	/** `null` until the first answer. */
	list: App[] | null;
	/** Why the app list failed; `null` after a success. */
	error: ErrorBody | null;
}>({ list: null, error: null });

/** Bumped when the NAS link closes: answers of an older link are dropped. */
let request = 0;
/** An `app.query` is on its way. */
let inFlight = false;
/** The answer on its way is stale: drop it and query again. */
let again = false;
/** Mounted components following the list. */
let followers = 0;

const summary = $derived(apps.list ? summarize(apps.list) : null);

/** The apps counted by state, `null` until the first answer. Reactive. */
export function appSummary(): AppSummary | null {
	return summary;
}

onNasClose(() => {
	request++;
	inFlight = false;
	again = false;
	apps.list = null;
	apps.error = null;
});

// A job ended: the app changed, and TrueNAS may not say so (#57).
onActionDone(() => {
	if (followers > 0 && apps.list) loadApps();
});

/** Queries the app list, or marks the query on its way stale. */
export function loadApps(): void {
	if (inFlight) {
		again = true;
		return;
	}
	inFlight = true;
	const mine = request;
	const settle = (apply: () => void) => {
		if (mine !== request) return;
		inFlight = false;
		if (again) {
			again = false;
			loadApps();
		} else {
			apply();
		}
	};
	nas()
		.call(APP_QUERY, [[], { select: [...APP_SELECT] }])
		.then(
			(raw) =>
				settle(() => {
					const list = parseApps(raw);
					if (!list) {
						apps.error = { code: 'bad_request', message: "The NAS sent apps truelite can't read." };
						return;
					}
					apps.list = list;
					apps.error = null;
				}),
			(e: unknown) =>
				settle(() => {
					const error = shownError(e);
					if (error) apps.error = error;
				})
		);
}

/** Handles an `app.query` topic message. Exported for tests. */
export function onAppMessage(message: TopicMessage): void {
	if ('data' in message) {
		// After a failed reload the list may lack events: read it again rather than patch it.
		const next = !apps.error && apps.list && applyAppEvent(apps.list, message.data);
		if (next) {
			apps.list = next;
			// The answer on its way may predate this event.
			if (inFlight) again = true;
		} else {
			loadApps();
		}
	} else if (message.status === 'ended') {
		apps.error = message.error;
	} else if (needsRefetch(message)) {
		loadApps();
	}
}

/** Counts a follower of the list; returns the function that stops it. Exported for tests. */
export function followApps(): () => void {
	followers++;
	let followed = true;
	return () => {
		if (followed) followers--;
		followed = false;
	};
}

/**
 * Follows the apps while the calling component is mounted. Call from component setup.
 * `retry()` subscribes again after the topic ended.
 */
export function useApps(): { retry(): void } {
	$effect(() => followApps());
	const topic = useTopic(APP_QUERY, onAppMessage);
	return {
		retry: () => {
			apps.error = null;
			topic.retry();
		}
	};
}

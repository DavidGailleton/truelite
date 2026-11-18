// Svelte binding of the alerts: the list follows `alert.list` events, and is queried again on
// (re)subscription. No timer polling.
import { nas, onNasClose, useTopic } from './nas.svelte';
import {
	ALERT_LIST,
	applyAlertEvent,
	parseAlerts,
	summarizeAlerts,
	type Alert,
	type AlertSummary
} from './alerts';
import { needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

export const alerts = $state<{
	/** `null` until the first answer. Replaced, never mutated. */
	list: Alert[] | null;
	/** Why the alert list failed; `null` after a success. */
	error: ErrorBody | null;
}>({ list: null, error: null });

let request = 0;

const summary = $derived(alerts.list ? summarizeAlerts(alerts.list) : null);

/** The alerts sorted and counted, `null` until the first answer. Reactive. */
export function alertSummary(): AlertSummary | null {
	return summary;
}

onNasClose(() => {
	request++;
	alerts.list = null;
	alerts.error = null;
});

/** Queries the alert list. */
export function loadAlerts(): void {
	const mine = ++request;
	nas()
		.call(ALERT_LIST, [])
		.then((raw) => {
			if (mine !== request) return;
			const list = parseAlerts(raw);
			if (!list) {
				alerts.error = { code: 'bad_request', message: "The NAS sent alerts truelite can't read." };
				return;
			}
			alerts.list = list;
			alerts.error = null;
		})
		.catch((e: unknown) => {
			if (mine !== request) return;
			const error = shownError(e);
			if (error) alerts.error = error;
		});
}

function onAlertMessage(message: TopicMessage): void {
	if ('data' in message) {
		// After a failed reload the list may lack events: read it again rather than patch it.
		const next = !alerts.error && alerts.list && applyAlertEvent(alerts.list, message.data);
		if (next) alerts.list = next;
		else loadAlerts();
	} else if (message.status === 'ended') {
		alerts.error = message.error;
	} else if (needsRefetch(message)) {
		loadAlerts();
	}
}

/**
 * Follows the alerts while the calling component is mounted. Call from component setup. Not
 * paused while the tab is hidden: alerts are rare and must be current when it comes back.
 * `retry()` subscribes again after the topic ended.
 */
export function useAlerts(): { retry(): void } {
	const topic = useTopic(ALERT_LIST, onAlertMessage);
	return {
		retry: () => {
			alerts.error = null;
			topic.retry();
		}
	};
}

// Svelte binding of `update.status`: read on (re)subscription, then replaced by each event.
// Shared by the dashboard banner and the System page. No timer polling.
import { nas, onNasClose, useTopic } from './nas.svelte';
import { UPDATE_STATUS, parseUpdateStatus, type UpdateStatus } from './update';
import { needsRefetch, shownError, type ErrorBody, type TopicMessage } from './ws';

const UNREADABLE: ErrorBody = {
	code: 'bad_request',
	message: "The NAS sent an update status truelite can't read."
};

export const update = $state<{
	/** `null` until the first answer. */
	status: UpdateStatus | null;
	/** Why the last read failed; `null` after a success. */
	error: ErrorBody | null;
}>({ status: null, error: null });

let request = 0;

onNasClose(() => {
	request++;
	update.status = null;
	update.error = null;
});

/** The version on offer when an update is available, else `null`. Reactive. */
export function availableVersion(): string | null {
	return update.status?.kind === 'available' ? update.status.newVersion : null;
}

/**
 * Whether the dashboard shows the update banner: an update on offer, or one installed and
 * waiting for a restart. Reactive.
 */
export function updateBannerShown(): boolean {
	return availableVersion() !== null || update.status?.kind === 'restart';
}

/** Asks the NAS for the update status. */
export function loadUpdateStatus(): void {
	const mine = ++request;
	nas()
		.call(UPDATE_STATUS, [])
		.then((raw) => {
			if (mine !== request) return;
			const status = parseUpdateStatus(raw);
			update.error = status ? null : UNREADABLE;
			if (status) update.status = status;
		})
		.catch((e: unknown) => {
			if (mine !== request) return;
			const error = shownError(e);
			if (error) update.error = error;
		});
}

function onUpdateMessage(message: TopicMessage): void {
	if ('data' in message) {
		const status = parseUpdateStatus(message.data.fields);
		if (status) {
			request++; // An older call answering late must not undo the event.
			update.status = status;
			update.error = null;
		} else loadUpdateStatus();
	} else if (message.status === 'ended') {
		update.error = message.error;
	} else if (needsRefetch(message)) {
		loadUpdateStatus();
	}
}

/**
 * Follows the update status while the calling component is mounted. Call from component setup.
 * `retry()` subscribes again after the topic ended.
 */
export function useUpdateStatus(): { retry(): void } {
	const topic = useTopic(UPDATE_STATUS, onUpdateMessage);
	return {
		retry: () => {
			update.error = null;
			topic.retry();
		}
	};
}

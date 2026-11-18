// Svelte binding of the WebSocket client: one connection for the whole app.
import { NasSocket, type Status, type TopicHandler } from './ws';

let socket: NasSocket | undefined;

/** Reactive connection state, for banners and guards. */
export const connection = $state<{ status: Status }>({ status: 'connecting' });

/** The app's connection to truelite, opened on first use. */
export function nas(): NasSocket {
	if (!socket) {
		socket = new NasSocket();
		socket.onStatus((status) => (connection.status = status));
	}
	return socket;
}

const resets: (() => void)[] = [];

/**
 * Registers a reset run by {@link closeNas}, so data of the account that logged out is never
 * shown to the next one. Stores call it once, at module load.
 */
export function onNasClose(reset: () => void): void {
	resets.push(reset);
}

/** Stops the connection after a logout; the next {@link nas} call opens a fresh one. */
export function closeNas(): void {
	socket?.close();
	socket = undefined;
	connection.status = 'connecting';
	for (const reset of resets) reset();
}

/** Whether the page is on screen (not a background tab, not a locked phone). */
const page = $state({ visible: true });
let tracking = false;

function trackVisibility(): void {
	if (tracking || typeof document === 'undefined') return;
	tracking = true;
	const update = () => (page.visible = document.visibilityState !== 'hidden');
	update();
	document.addEventListener('visibilitychange', update);
}

/** Whether the page is on screen; reactive. */
export function pageVisible(): boolean {
	trackVisibility();
	return page.visible;
}

export interface TopicOptions {
	/**
	 * Unsubscribe while the page is hidden and subscribe again when it comes back (the handler
	 * then gets `subscribed` again). For streams like live stats that cost the NAS, the network
	 * and the phone's battery for nobody.
	 */
	whenVisible?: boolean;
}

/**
 * Follows a topic while the calling component is mounted. Call from component setup. The handler
 * should query the state again whenever `needsRefetch(message)` is true. After `ended`, call
 * `retry()` to subscribe again.
 */
export function useTopic(
	topic: string,
	handler: TopicHandler,
	options: TopicOptions = {}
): { retry(): void } {
	const { whenVisible = false } = options;
	if (whenVisible) trackVisibility();
	let attempt = $state(0);
	$effect(() => {
		void attempt; // re-run on retry()
		if (whenVisible && !page.visible) return;
		// A wrapper of our own: handlers live in a Set, and two callers sharing one function would
		// share one entry, so the first to unmount would unsubscribe the other.
		return nas().subscribe(topic, (message) => handler(message));
	});
	return { retry: () => attempt++ };
}

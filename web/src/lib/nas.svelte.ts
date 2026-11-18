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

/**
 * Follows a topic while the calling component is mounted. Call from component setup. The handler
 * should query the state again whenever `needsRefetch(message)` is true.
 */
export function useTopic(topic: string, handler: TopicHandler): void {
	$effect(() => nas().subscribe(topic, handler));
}

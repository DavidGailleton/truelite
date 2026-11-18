// Svelte binding of one container's log (#32). The server sends the tail, then new lines, in
// batches; a new or resumed subscription sends the tail again, so the page starts over then.
import { useTopic } from './nas.svelte';
import { LogBuffer, logTopic, parseLogBatch, type LogEntry } from './logs';
import type { ErrorBody, TopicMessage } from './ws';

export type LogState = 'waiting' | 'live' | 'ended';

class LogStore {
	entries = $state.raw<LogEntry[]>([]);
	state = $state<LogState>('waiting');
	/** Why the stream ended; `null` while live. */
	error = $state.raw<ErrorBody | null>(null);

	readonly #buffer = new LogBuffer();

	onMessage(message: TopicMessage): void {
		if ('data' in message) {
			const lines = parseLogBatch(message.data.fields);
			if (lines && lines.length > 0) this.entries = this.#buffer.append(this.entries, lines);
			return;
		}
		switch (message.status) {
			case 'subscribed':
			case 'resumed':
				// The tail follows: start over rather than show it twice.
				this.entries = [];
				this.state = 'live';
				this.error = null;
				break;
			case 'missed':
				this.entries = this.#buffer.gap(this.entries, message.count);
				break;
			case 'ended':
				this.state = 'ended';
				this.error = message.error;
				break;
		}
	}
}

export interface LogView {
	readonly entries: LogEntry[];
	readonly state: LogState;
	readonly error: ErrorBody | null;
	retry(): void;
}

/**
 * Follows one container's log while the calling component is mounted (call from component
 * setup; key the component by container id). Stops while the page is hidden: the NAS keeps a
 * Docker log stream open for each follower.
 */
export function useLogs(app: string, containerId: string): LogView {
	const store = new LogStore();
	const topic = useTopic(logTopic(app, containerId), (message) => store.onMessage(message), {
		whenVisible: true
	});
	return {
		get entries() {
			return store.entries;
		},
		get state() {
			return store.state;
		},
		get error() {
			return store.error;
		},
		retry: () => {
			store.state = 'waiting';
			store.error = null;
			topic.retry();
		}
	};
}

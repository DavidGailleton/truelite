// Container logs (#32): the topic string, the server's line batches and a bounded buffer. The
// wire format is in `docs/ws-protocol.md` (`app.container_log_follow`).
import { sortContainers, type Container } from './app-detail';

export const LOG_EVENT = 'app.container_log_follow';
/** Lines of history asked for on each (re)subscription; the server allows 1 to 200. */
export const LOG_TAIL = 200;
/** Lines kept on the page; older ones are dropped. */
export const LOG_KEEP = 1000;

/** The topic following one container's log. */
export function logTopic(app: string, containerId: string, tail = LOG_TAIL): string {
	return `${LOG_EVENT}:${JSON.stringify({ app_name: app, container_id: containerId, tail_lines: tail })}`;
}

export interface LogLine {
	/** Docker timestamp as TrueNAS formats it (`2026-11-18 10:00:01.123456+00:00`), or `null`. */
	time: string | null;
	text: string;
}

/** A line on the page, or a marker where lines were skipped (`skipped` > 0). */
export interface LogEntry extends LogLine {
	key: number;
	skipped?: number;
}

/** The lines of one `data` message, or `null` when it is not a batch of lines. */
export function parseLogBatch(fields: unknown): LogLine[] | null {
	if (typeof fields !== 'object' || fields === null) return null;
	const lines = (fields as { lines?: unknown }).lines;
	if (!Array.isArray(lines)) return null;
	const out: LogLine[] = [];
	for (const raw of lines) {
		if (typeof raw !== 'object' || raw === null) continue;
		const { time, text } = raw as { time?: unknown; text?: unknown };
		if (typeof text !== 'string') continue;
		out.push({ time: typeof time === 'string' ? time : null, text });
	}
	return out;
}

/**
 * The last {@link LOG_KEEP} entries, each with a key that never repeats (keyed `#each`). Every
 * change returns a new array, for `$state.raw`.
 */
export class LogBuffer {
	#next = 0;
	readonly #keep: number;

	constructor(keep = LOG_KEEP) {
		this.#keep = keep;
	}

	append(entries: readonly LogEntry[], lines: readonly LogLine[]): LogEntry[] {
		const added = lines.map((line) => ({ ...line, key: this.#next++ }));
		return this.#trim([...entries, ...added]);
	}

	/** A marker: the server skipped `count` events (the page read too slowly). */
	gap(entries: readonly LogEntry[], count: number): LogEntry[] {
		const marker: LogEntry = {
			key: this.#next++,
			time: null,
			text: '',
			skipped: Math.max(1, count)
		};
		return this.#trim([...entries, marker]);
	}

	#trim(entries: LogEntry[]): LogEntry[] {
		return entries.length > this.#keep ? entries.slice(entries.length - this.#keep) : entries;
	}
}

/** `10:00:01` in local time, or `null` when the timestamp can't be read. */
export function logTime(time: string | null): string | null {
	if (!time) return null;
	// Python's `str(datetime)`: a space before the time and up to 9 fraction digits.
	const iso = time
		.trim()
		.replace(' ', 'T')
		.replace(/(\.\d{3})\d+/, '$1');
	const ms = Date.parse(iso);
	if (Number.isNaN(ms)) return null;
	const d = new Date(ms);
	const two = (n: number) => String(n).padStart(2, '0');
	return `${two(d.getHours())}:${two(d.getMinutes())}:${two(d.getSeconds())}`;
}

/**
 * The container whose log to show: the one of `service` (stable across restarts, unlike the
 * id), else the first running one, else the first. `null` without containers.
 */
export function pickContainer(
	containers: readonly Container[],
	service: string | null
): Container | null {
	const asked = service ? containers.find((c) => c.service === service) : undefined;
	if (asked) return asked;
	const sorted = sortContainers(containers);
	return sorted.find((c) => c.state === 'running') ?? sorted[0] ?? null;
}

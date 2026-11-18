// Browser side of our WebSocket protocol (docs/ws-protocol.md). Framework-free so it can be
// unit-tested: the Svelte binding lives in nas.svelte.ts.

/** What the page should show about the connection. */
export type Status =
	/** Opening the socket, or waiting for the server's first `link` message. */
	| 'connecting'
	/** Connected to truelite, and truelite to the NAS. */
	| 'up'
	/** truelite is up but reconnecting to the NAS; calls wait for it. */
	| 'nas-reconnecting'
	/** truelite cannot be reached; retrying with backoff. */
	| 'offline'
	/** The session ended: log in again. Final. */
	| 'logged-out';

/** Protocol error codes, plus two from this client (`disconnected`, and `timeout` from its own timer). */
export type ErrorCode =
	| 'not_allowed'
	| 'bad_request'
	| 'nas'
	| 'timeout'
	| 'reconnecting'
	| 'busy'
	| 'logged_out'
	| 'disconnected';

export interface ErrorBody {
	code: ErrorCode;
	message: string;
	errname?: string;
}

/** A call or subscription that failed; `message` is in plain words, safe to show. */
export class NasError extends Error {
	readonly code: ErrorCode;
	readonly errname?: string;

	constructor({ code, message, errname }: ErrorBody) {
		super(message);
		this.name = 'NasError';
		this.code = code;
		this.errname = errname;
	}
}

/**
 * A failed call as an error worth showing, or `null` for one from a dropped link: the topic's
 * next `resumed` queries again.
 */
export function shownError(e: unknown): ErrorBody | null {
	if (!(e instanceof NasError)) return { code: 'bad_request', message: 'Something went wrong.' };
	if (['disconnected', 'reconnecting', 'logged_out'].includes(e.code)) return null;
	return { code: e.code, message: e.message, errname: e.errname };
}

export interface EventData {
	kind: 'added' | 'changed' | 'removed' | 'other';
	id?: unknown;
	fields?: Record<string, unknown>;
}

/** What a topic handler receives. */
export type TopicMessage =
	/** First subscription: query the current state now. */
	| { status: 'subscribed' }
	/** Subscribed again after a reconnect: events of the gap are lost, query again. */
	| { status: 'resumed' }
	/** The page read too slowly and `count` events were dropped: query again. */
	| { status: 'missed'; count: number }
	/** Refused or ended by the NAS. Nothing follows; subscribe again to retry. */
	| { status: 'ended'; error: ErrorBody }
	| { data: EventData };

export type TopicHandler = (message: TopicMessage) => void;

/** True when the handler should query the topic's state again. */
export function needsRefetch(message: TopicMessage): boolean {
	return 'status' in message && message.status !== 'ended';
}

/** The part of the browser `WebSocket` this client uses. */
export interface SocketLike {
	readonly readyState: number;
	onopen: ((ev: unknown) => void) | null;
	onmessage: ((ev: { data: unknown }) => void) | null;
	onclose: ((ev: { code: number }) => void) | null;
	onerror: ((ev: unknown) => void) | null;
	send(data: string): void;
	close(code?: number, reason?: string): void;
}

export interface Options {
	/** Default: `/ws` on the page's own host. */
	url?: string;
	WebSocket?: new (url: string) => SocketLike;
	fetch?: (input: string, init?: RequestInit) => Promise<{ status: number }>;
	/** Jitter source, `[0, 1)`. */
	random?: () => number;
	/**
	 * Registers a callback for "now is a good time to retry" (back online, tab visible again)
	 * and returns its removal. Default: window `online` + document `visibilitychange`.
	 */
	onWake?: (wake: () => void) => () => void;
	/** First call id; tests use it to reach the wrap-around. */
	firstId?: number;
}

/** Same as the server's `CLOSE_LOGGED_OUT` (server/src/ws.rs). */
export const CLOSE_LOGGED_OUT = 4401;
/** Longer than the server's 30 s call timeout, so its `timeout` reply normally comes first. */
export const CALL_TIMEOUT_MS = 35_000;
const BACKOFF_BASE_MS = 1_000;
const BACKOFF_MAX_MS = 30_000;
/** A socket that stayed up this long resets the backoff. */
const STABLE_MS = 10_000;
const OPEN = 1;

interface Pending {
	method: string;
	params: unknown[];
	resolve: (value: unknown) => void;
	reject: (error: NasError) => void;
	timer: ReturnType<typeof setTimeout>;
	/** Written to a socket: may have run, so it is never sent again. */
	sent: boolean;
}

interface Topic {
	handlers: Set<TopicHandler>;
	/** Acknowledged at least once: later `subscribed` acks (new socket) are reported as `resumed`. */
	acked: boolean;
}

/** Backoff before reconnect attempt `attempt` (0-based): 1 s·2ⁿ, at most 30 s, ±20 % jitter. */
export function backoffMs(attempt: number, random: () => number): number {
	const base = Math.min(BACKOFF_BASE_MS * 2 ** Math.min(attempt, 16), BACKOFF_MAX_MS);
	return Math.round(base * (0.8 + 0.4 * random()));
}

function defaultUrl(): string {
	const { protocol, host } = globalThis.location;
	return `${protocol === 'https:' ? 'wss' : 'ws'}://${host}/ws`;
}

function defaultWake(wake: () => void): () => void {
	const onVisible = () => {
		if (document.visibilityState === 'visible') wake();
	};
	window.addEventListener('online', wake);
	document.addEventListener('visibilitychange', onVisible);
	return () => {
		window.removeEventListener('online', wake);
		document.removeEventListener('visibilitychange', onVisible);
	};
}

function errorBody(value: unknown): ErrorBody {
	const e = (value ?? {}) as Partial<ErrorBody>;
	return {
		code: e.code ?? 'bad_request',
		message: e.message ?? 'Something went wrong.',
		...(e.errname ? { errname: e.errname } : {})
	};
}

/**
 * One connection to truelite's `/ws`: calls with promises, ref-counted subscriptions and
 * reconnect with backoff. Calls are never sent twice; subscriptions are renewed on every new
 * socket.
 */
export class NasSocket {
	#status: Status = 'connecting';
	#statusListeners = new Set<(status: Status) => void>();
	#socket: SocketLike | null = null;
	/** The current socket received its first `link up`. */
	#reachedUp = false;
	#pending = new Map<number, Pending>();
	#nextId: number;
	#topics = new Map<string, Topic>();
	#attempt = 0;
	#retryTimer: ReturnType<typeof setTimeout> | null = null;
	#stableTimer: ReturnType<typeof setTimeout> | null = null;
	#closed = false;
	#removeWake: () => void;

	readonly #url: string;
	readonly #WebSocket: new (url: string) => SocketLike;
	readonly #fetch: NonNullable<Options['fetch']>;
	readonly #random: () => number;

	constructor(options: Options = {}) {
		this.#url = options.url ?? defaultUrl();
		this.#WebSocket = options.WebSocket ?? (globalThis.WebSocket as never);
		this.#fetch = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
		this.#random = options.random ?? Math.random;
		this.#nextId = (options.firstId ?? 0) >>> 0;
		this.#removeWake = (options.onWake ?? defaultWake)(() => this.#wake());
		this.#open();
	}

	get status(): Status {
		return this.#status;
	}

	/** Calls `listener` with the current status now and on every change; returns its removal. */
	onStatus(listener: (status: Status) => void): () => void {
		this.#statusListeners.add(listener);
		listener(this.#status);
		return () => this.#statusListeners.delete(listener);
	}

	/**
	 * Calls an allowlisted TrueNAS method. Rejects with {@link NasError}. While truelite is out of
	 * reach the call waits (until its timeout) and is sent once connected.
	 */
	call<T = unknown>(
		method: string,
		params: unknown[] = [],
		{ timeout = CALL_TIMEOUT_MS }: { timeout?: number } = {}
	): Promise<T> {
		if (this.#status === 'logged-out' || this.#closed) {
			return Promise.reject(loggedOut());
		}
		const id = this.#takeId();
		return new Promise<T>((resolve, reject) => {
			const timer = setTimeout(() => {
				this.#pending.delete(id);
				reject(new NasError({ code: 'timeout', message: 'The NAS did not answer in time.' }));
			}, timeout);
			const pending: Pending = {
				method,
				params,
				resolve: resolve as (value: unknown) => void,
				reject,
				timer,
				sent: false
			};
			this.#pending.set(id, pending);
			if (this.#reachedUp) this.#sendCall(id, pending);
		});
	}

	/**
	 * Follows a topic (an allowlisted TrueNAS event). Handlers of one topic share a single
	 * subscription. Returns the removal; the last one unsubscribes.
	 */
	subscribe(topic: string, handler: TopicHandler): () => void {
		if (this.#status === 'logged-out' || this.#closed) {
			handler({ status: 'ended', error: { ...loggedOutBody() } });
			return () => {};
		}
		let entry = this.#topics.get(topic);
		if (!entry) {
			entry = { handlers: new Set(), acked: false };
			this.#topics.set(topic, entry);
			if (this.#reachedUp) this.#send({ op: 'sub', topic });
		}
		entry.handlers.add(handler);
		const mine = entry;
		return () => {
			if (!mine.handlers.delete(handler) || mine.handlers.size > 0) return;
			if (this.#topics.get(topic) !== mine) return;
			this.#topics.delete(topic);
			if (this.#reachedUp) this.#send({ op: 'unsub', topic });
		};
	}

	/** Stops for good: closes the socket, rejects pending calls, stops retrying. */
	close(): void {
		if (this.#closed) return;
		this.#closed = true;
		this.#removeWake();
		this.#clearTimers();
		const socket = this.#socket;
		this.#detach();
		socket?.close(1000);
		this.#rejectAll(() => loggedOut());
	}

	#open(): void {
		this.#retryTimer = null;
		if (this.#closed) return;
		this.#setStatus(this.#status === 'offline' ? 'offline' : 'connecting');
		this.#reachedUp = false;
		let socket: SocketLike;
		try {
			socket = new this.#WebSocket(this.#url);
		} catch {
			this.#scheduleRetry();
			return;
		}
		this.#socket = socket;
		socket.onmessage = (ev) => this.#onMessage(ev.data);
		socket.onclose = (ev) => this.#onClose(ev.code);
	}

	#onMessage(raw: unknown): void {
		let msg: Record<string, unknown>;
		try {
			msg = JSON.parse(String(raw));
		} catch {
			console.warn('truelite: malformed message', raw);
			return;
		}
		if (msg.op === 'link') {
			this.#onLink(msg.state);
		} else if (typeof msg.topic === 'string') {
			this.#onTopic(msg.topic, msg);
		} else if ('id' in msg) {
			this.#onReply(msg);
		} else {
			console.warn('truelite: unknown message', msg);
		}
	}

	#onLink(state: unknown): void {
		if (state === 'reconnecting') {
			this.#setStatus('nas-reconnecting');
			return;
		}
		if (state !== 'up') return;
		this.#setStatus('up');
		if (this.#reachedUp) return; // the NAS link came back: the server resumes topics itself
		this.#reachedUp = true;
		this.#stableTimer = setTimeout(() => {
			this.#stableTimer = null;
			this.#attempt = 0;
		}, STABLE_MS);
		for (const topic of this.#topics.keys()) this.#send({ op: 'sub', topic });
		for (const [id, pending] of this.#pending) this.#sendCall(id, pending);
	}

	#onTopic(topic: string, msg: Record<string, unknown>): void {
		const entry = this.#topics.get(topic);
		if (!entry) return; // unsubscribed meanwhile
		let message: TopicMessage;
		switch (msg.status) {
			case undefined:
				message = { data: msg.data as EventData };
				break;
			case 'subscribed':
				message = { status: entry.acked ? 'resumed' : 'subscribed' };
				entry.acked = true;
				break;
			case 'resumed':
				message = { status: 'resumed' };
				break;
			case 'missed':
				message = { status: 'missed', count: Number(msg.count) || 0 };
				break;
			case 'ended':
				this.#topics.delete(topic);
				message = { status: 'ended', error: errorBody(msg.error) };
				break;
			default:
				console.warn('truelite: unknown topic status', msg);
				return;
		}
		for (const handler of [...entry.handlers]) {
			try {
				handler(message);
			} catch (e) {
				console.error(`truelite: handler of ${topic} failed`, e);
			}
		}
	}

	#onReply(msg: Record<string, unknown>): void {
		if (typeof msg.id !== 'number') {
			console.warn('truelite: server rejected a message', msg.error);
			return;
		}
		const pending = this.#pending.get(msg.id);
		if (!pending) return; // timed out already
		this.#pending.delete(msg.id);
		clearTimeout(pending.timer);
		if ('error' in msg) pending.reject(new NasError(errorBody(msg.error)));
		else pending.resolve(msg.result);
	}

	#onClose(code: number): void {
		const reachedUp = this.#reachedUp;
		this.#detach();
		if (this.#stableTimer !== null) {
			clearTimeout(this.#stableTimer);
			this.#stableTimer = null;
		}
		// Calls written to the dead socket may have run: reject, never resend.
		this.#rejectAll(
			() =>
				new NasError({
					code: 'disconnected',
					message: 'The connection to truelite dropped. The action may or may not have run.'
				}),
			true
		);
		if (code === CLOSE_LOGGED_OUT) {
			this.#loggedOut();
			return;
		}
		this.#setStatus('offline');
		if (reachedUp) {
			this.#scheduleRetry();
			return;
		}
		// A refused upgrade (no session → 401) shows up as a plain 1006 close: ask the server.
		this.#fetch('/api/session', { credentials: 'same-origin', cache: 'no-store' }).then(
			(res) => {
				if (res.status === 401) this.#loggedOut();
				else this.#scheduleRetry();
			},
			() => this.#scheduleRetry()
		);
	}

	#scheduleRetry(): void {
		if (this.#closed || this.#status === 'logged-out' || this.#retryTimer !== null) return;
		const delay = backoffMs(this.#attempt, this.#random);
		this.#attempt += 1;
		this.#retryTimer = setTimeout(() => this.#open(), delay);
	}

	/** Back online or visible again: skip the remaining backoff. */
	#wake(): void {
		if (this.#retryTimer === null) return;
		clearTimeout(this.#retryTimer);
		this.#attempt = 0;
		this.#open();
	}

	#loggedOut(): void {
		this.#setStatus('logged-out');
		this.#clearTimers();
		this.#removeWake();
		this.#rejectAll(() => loggedOut());
		const ended: TopicMessage = { status: 'ended', error: loggedOutBody() };
		const topics = [...this.#topics.values()];
		this.#topics.clear();
		for (const { handlers } of topics) for (const handler of handlers) handler(ended);
	}

	#sendCall(id: number, pending: Pending): void {
		if (pending.sent) return;
		pending.sent = true;
		this.#send({ op: 'call', id, method: pending.method, params: pending.params });
	}

	#send(message: object): void {
		if (this.#socket?.readyState === OPEN) this.#socket.send(JSON.stringify(message));
	}

	#takeId(): number {
		let id = this.#nextId;
		while (this.#pending.has(id)) id = (id + 1) >>> 0;
		this.#nextId = (id + 1) >>> 0;
		return id;
	}

	/** Rejects pending calls: all of them, or only those already sent. */
	#rejectAll(error: () => NasError, sentOnly = false): void {
		for (const [id, pending] of this.#pending) {
			if (sentOnly && !pending.sent) continue;
			this.#pending.delete(id);
			clearTimeout(pending.timer);
			pending.reject(error());
		}
	}

	#detach(): void {
		if (this.#socket) {
			this.#socket.onmessage = null;
			this.#socket.onclose = null;
		}
		this.#socket = null;
		this.#reachedUp = false;
	}

	#clearTimers(): void {
		if (this.#retryTimer !== null) clearTimeout(this.#retryTimer);
		if (this.#stableTimer !== null) clearTimeout(this.#stableTimer);
		this.#retryTimer = null;
		this.#stableTimer = null;
	}

	#setStatus(status: Status): void {
		if (status === this.#status) return;
		this.#status = status;
		for (const listener of [...this.#statusListeners]) listener(status);
	}
}

function loggedOutBody(): ErrorBody {
	return { code: 'logged_out', message: 'Your session ended. Log in again.' };
}

function loggedOut(): NasError {
	return new NasError(loggedOutBody());
}

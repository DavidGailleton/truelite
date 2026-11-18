import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
	CALL_TIMEOUT_MS,
	CLOSE_LOGGED_OUT,
	NasError,
	NasSocket,
	backoffMs,
	needsRefetch,
	type Options,
	type SocketLike,
	type Status,
	type TopicMessage
} from './ws';

/** A scripted socket: records what the client sends, the test plays the server. */
class FakeSocket implements SocketLike {
	static all: FakeSocket[] = [];
	readyState = 0;
	sent: Record<string, unknown>[] = [];
	onopen: SocketLike['onopen'] = null;
	onmessage: SocketLike['onmessage'] = null;
	onclose: SocketLike['onclose'] = null;
	onerror: SocketLike['onerror'] = null;

	constructor(readonly url: string) {
		FakeSocket.all.push(this);
	}

	send(data: string) {
		this.sent.push(JSON.parse(data));
	}

	close() {
		this.readyState = 3;
	}

	/** Opens and sends the server's first message. */
	up() {
		this.readyState = 1;
		this.onopen?.({});
		this.serverSend({ op: 'link', state: 'up' });
	}

	serverSend(message: object) {
		this.onmessage?.({ data: JSON.stringify(message) });
	}

	serverClose(code = 1006) {
		this.readyState = 3;
		this.onclose?.({ code });
	}

	ops(op: string) {
		return this.sent.filter((m) => m.op === op);
	}
}

const last = () => FakeSocket.all[FakeSocket.all.length - 1];

let sessionStatus: number;
let fetchCalls: string[];
let wake: () => void;

function connect(extra: Options = {}) {
	const socket = new NasSocket({
		url: 'ws://truelite/ws',
		WebSocket: FakeSocket,
		fetch: async (input) => {
			fetchCalls.push(input);
			return { status: sessionStatus };
		},
		random: () => 0.5, // no jitter
		onWake: (fn) => {
			wake = fn;
			return () => {};
		},
		...extra
	});
	const statuses: Status[] = [];
	socket.onStatus((s) => statuses.push(s));
	return { socket, statuses };
}

/** Lets promise callbacks (the session check) run. */
const flush = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
	vi.useFakeTimers();
	FakeSocket.all = [];
	sessionStatus = 200;
	fetchCalls = [];
});

afterEach(() => {
	vi.useRealTimers();
});

describe('status', () => {
	it('is up only after the link message, and queued calls go out then', async () => {
		const { socket, statuses } = connect();
		const reply = socket.call('system.info');
		const ws = last();
		ws.readyState = 1;
		expect(ws.sent).toEqual([]);
		expect(socket.status).toBe('connecting');

		ws.serverSend({ op: 'link', state: 'up' });
		expect(statuses).toEqual(['connecting', 'up']);
		expect(ws.ops('call')).toEqual([{ op: 'call', id: 0, method: 'system.info', params: [] }]);

		ws.serverSend({ id: 0, result: { hostname: 'nas' } });
		await expect(reply).resolves.toEqual({ hostname: 'nas' });
	});

	it('follows the NAS link without a new socket', () => {
		const { statuses } = connect();
		const ws = last();
		ws.up();
		ws.serverSend({ op: 'link', state: 'reconnecting' });
		ws.serverSend({ op: 'link', state: 'up' });
		expect(statuses).toEqual(['connecting', 'up', 'nas-reconnecting', 'up']);
		expect(FakeSocket.all).toHaveLength(1);
	});
});

describe('calls', () => {
	it('match replies by id, in any order', async () => {
		const { socket } = connect();
		last().up();
		const a = socket.call('a', [1]);
		const b = socket.call('b');
		const [ca, cb] = last().ops('call');
		expect(ca).toMatchObject({ method: 'a', params: [1] });
		last().serverSend({ id: cb.id, result: 'B' });
		last().serverSend({ id: ca.id, result: 'A' });
		await expect(a).resolves.toBe('A');
		await expect(b).resolves.toBe('B');
	});

	it('reject with the error code and name', async () => {
		const { socket } = connect();
		last().up();
		const p = socket.call('pool.query');
		last().serverSend({
			id: 0,
			error: { code: 'nas', message: 'Not allowed.', errname: 'EACCES' }
		});
		const err = await p.catch((e) => e);
		expect(err).toBeInstanceOf(NasError);
		expect(err).toMatchObject({ code: 'nas', errname: 'EACCES', message: 'Not allowed.' });
	});

	it('wrap the id after 2^32 - 1', () => {
		const { socket } = connect({ firstId: 2 ** 32 - 1 });
		last().up();
		socket.call('a').catch(() => {});
		socket.call('b').catch(() => {});
		expect(
			last()
				.ops('call')
				.map((c) => c.id)
		).toEqual([2 ** 32 - 1, 0]);
	});

	it('time out on their own and ignore a late reply', async () => {
		const { socket } = connect();
		last().up();
		const p = socket.call('slow');
		const err = p.catch((e) => e);
		await vi.advanceTimersByTimeAsync(CALL_TIMEOUT_MS);
		expect(await err).toMatchObject({ code: 'timeout' });
		last().serverSend({ id: 0, result: 'late' }); // no throw
	});

	it('in flight are rejected on close and never resent', async () => {
		const { socket } = connect();
		last().up();
		const p = socket.call('app.start').catch((e) => e);
		last().serverClose(1006);
		expect(await p).toMatchObject({ code: 'disconnected' });

		await vi.advanceTimersByTimeAsync(backoffMs(0, () => 0.5));
		last().up();
		expect(FakeSocket.all).toHaveLength(2);
		expect(last().ops('call')).toEqual([]);
	});

	it('queued before up survive a socket that never came up', async () => {
		const { socket } = connect();
		const p = socket.call('system.info');
		last().serverClose(1006);
		await vi.advanceTimersByTimeAsync(1000);
		last().up();
		expect(last().ops('call')).toHaveLength(1);
		last().serverSend({ id: 0, result: 'ok' });
		await expect(p).resolves.toBe('ok');
	});

	it('made while offline wait for the next socket', async () => {
		const { socket } = connect();
		last().up();
		last().serverClose(1006);
		const p = socket.call('system.info');
		await vi.advanceTimersByTimeAsync(1000);
		last().up();
		last().serverSend({ id: 0, result: 'ok' });
		await expect(p).resolves.toBe('ok');
	});
});

describe('subscriptions', () => {
	it('share one sub per topic and unsub after the last handler', () => {
		const { socket } = connect();
		last().up();
		const one = socket.subscribe('alert.list', () => {});
		const two = socket.subscribe('alert.list', () => {});
		expect(last().ops('sub')).toEqual([{ op: 'sub', topic: 'alert.list' }]);
		one();
		one(); // twice is harmless
		expect(last().ops('unsub')).toEqual([]);
		two();
		expect(last().ops('unsub')).toEqual([{ op: 'unsub', topic: 'alert.list' }]);
	});

	it('deliver acks and events, and say when to query again', () => {
		const { socket } = connect();
		last().up();
		const got: TopicMessage[] = [];
		socket.subscribe('alert.list', (m) => got.push(m));
		last().serverSend({ topic: 'alert.list', status: 'subscribed' });
		last().serverSend({ topic: 'alert.list', data: { kind: 'added', id: 2, fields: { n: 2 } } });
		last().serverSend({ topic: 'alert.list', status: 'missed', count: 3 });
		last().serverSend({ topic: 'other', data: { kind: 'added' } });
		expect(got).toEqual([
			{ status: 'subscribed' },
			{ data: { kind: 'added', id: 2, fields: { n: 2 } } },
			{ status: 'missed', count: 3 }
		]);
		expect(got.map(needsRefetch)).toEqual([true, false, true]);
	});

	it('are renewed on a new socket and reported as resumed', async () => {
		const { socket } = connect();
		last().up();
		const got: TopicMessage[] = [];
		socket.subscribe('alert.list', (m) => got.push(m));
		last().serverSend({ topic: 'alert.list', status: 'subscribed' });
		last().serverClose(1006);

		await vi.advanceTimersByTimeAsync(1000);
		expect(last().ops('sub')).toEqual([]);
		last().up();
		expect(last().ops('sub')).toEqual([{ op: 'sub', topic: 'alert.list' }]);
		last().serverSend({ topic: 'alert.list', status: 'subscribed' });
		expect(got.at(-1)).toEqual({ status: 'resumed' });
	});

	it('are dropped when ended, and subscribing again retries', () => {
		const { socket } = connect();
		last().up();
		const got: TopicMessage[] = [];
		const stop = socket.subscribe('pool.query', (m) => got.push(m));
		last().serverSend({
			topic: 'pool.query',
			status: 'ended',
			error: { code: 'nas', message: 'No.', errname: 'EACCES' }
		});
		expect(got).toEqual([
			{ status: 'ended', error: { code: 'nas', message: 'No.', errname: 'EACCES' } }
		]);
		stop(); // no unsub for an ended topic
		expect(last().ops('unsub')).toEqual([]);
		socket.subscribe('pool.query', () => {});
		expect(last().ops('sub')).toHaveLength(2);
	});
});

describe('session end', () => {
	it('close 4401 logs out: no reconnect, calls and topics end', async () => {
		const { socket, statuses } = connect();
		last().up();
		const got: TopicMessage[] = [];
		socket.subscribe('alert.list', (m) => got.push(m));
		const pending = socket.call('system.info').catch((e) => e);
		last().serverClose(CLOSE_LOGGED_OUT);

		expect(statuses.at(-1)).toBe('logged-out');
		expect(await pending).toMatchObject({ code: 'disconnected' });
		expect(got).toEqual([
			{ status: 'ended', error: expect.objectContaining({ code: 'logged_out' }) }
		]);
		await expect(socket.call('x')).rejects.toMatchObject({ code: 'logged_out' });
		await vi.advanceTimersByTimeAsync(60_000);
		expect(FakeSocket.all).toHaveLength(1);
		expect(fetchCalls).toEqual([]);
	});

	it('a refused upgrade with no session logs out', async () => {
		sessionStatus = 401;
		const { socket } = connect();
		last().serverClose(1006);
		await flush();
		expect(fetchCalls).toEqual(['/api/session']);
		expect(socket.status).toBe('logged-out');
		await vi.advanceTimersByTimeAsync(60_000);
		expect(FakeSocket.all).toHaveLength(1);
	});

	it('a refused upgrade with a live session retries with backoff', async () => {
		const { socket } = connect();
		last().serverClose(1006);
		await flush();
		expect(socket.status).toBe('offline');
		await vi.advanceTimersByTimeAsync(999);
		expect(FakeSocket.all).toHaveLength(1);
		await vi.advanceTimersByTimeAsync(1);
		expect(FakeSocket.all).toHaveLength(2);
	});

	it('an unreachable server keeps retrying', async () => {
		const { socket } = connect({ fetch: () => Promise.reject(new TypeError('offline')) });
		last().serverClose(1006);
		await vi.advanceTimersByTimeAsync(1000);
		expect(FakeSocket.all).toHaveLength(2);
		expect(socket.status).toBe('offline');
	});
});

describe('reconnect', () => {
	it('backs off 1 s·2ⁿ up to 30 s with ±20 % jitter', () => {
		const mid = () => 0.5;
		expect([0, 1, 2, 3, 4, 5, 6, 40].map((n) => backoffMs(n, mid))).toEqual([
			1000, 2000, 4000, 8000, 16000, 30000, 30000, 30000
		]);
		expect(backoffMs(0, () => 0)).toBe(800);
		expect(backoffMs(5, () => 0.999999)).toBe(36000);
	});

	it('grows the delay while attempts fail, resets after 10 s up', async () => {
		connect();
		const delays: number[] = [];
		for (let i = 0; i < 3; i++) {
			const before = FakeSocket.all.length;
			last().serverClose(1006);
			let waited = 0;
			while (FakeSocket.all.length === before) {
				await vi.advanceTimersByTimeAsync(100);
				waited += 100;
			}
			delays.push(waited);
		}
		expect(delays).toEqual([1000, 2000, 4000]);

		last().up();
		await vi.advanceTimersByTimeAsync(10_000);
		last().serverClose(1006);
		await vi.advanceTimersByTimeAsync(1000);
		expect(FakeSocket.all).toHaveLength(5);
	});

	it('wakes up at once when the page comes back', async () => {
		connect();
		last().serverClose(1006);
		await flush();
		wake();
		expect(FakeSocket.all).toHaveLength(2);
	});

	it('close() stops everything', async () => {
		const { socket } = connect();
		last().up();
		const p = socket.call('x').catch((e) => e);
		socket.close();
		expect(await p).toBeInstanceOf(NasError);
		await vi.advanceTimersByTimeAsync(60_000);
		expect(FakeSocket.all).toHaveLength(1);
	});
});

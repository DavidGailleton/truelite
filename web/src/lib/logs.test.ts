import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import type { Container } from './app-detail';
import { LOG_KEEP, LogBuffer, logTime, logTopic, parseLogBatch, pickContainer } from './logs';

const ID = '5c3e1d2b9f8a7e6d5c4b3a291807f6e5d4c3b2a1908f7e6d5c4b3a2910f8e7d6';

describe('logTopic', () => {
	it('names the event with its arguments as JSON', () => {
		const topic = logTopic('plex', ID);
		expect(topic.startsWith('app.container_log_follow:')).toBe(true);
		expect(JSON.parse(topic.slice(topic.indexOf(':') + 1))).toEqual({
			app_name: 'plex',
			container_id: ID,
			tail_lines: 200
		});
		expect(logTopic('plex', ID, 5)).toContain('"tail_lines":5');
	});
});

describe('parseLogBatch', () => {
	it('reads lines and skips odd ones', () => {
		expect(
			parseLogBatch({
				lines: [
					{ time: '2026-11-18 10:00:01+00:00', text: 'a' },
					{ time: null, text: 'b' },
					{ time: 3, text: 'c' },
					{ text: 4 },
					null
				]
			})
		).toEqual([
			{ time: '2026-11-18 10:00:01+00:00', text: 'a' },
			{ time: null, text: 'b' },
			{ time: null, text: 'c' }
		]);
	});

	it('refuses what is not a batch', () => {
		for (const fields of [null, 'x', {}, { lines: 'x' }]) expect(parseLogBatch(fields)).toBeNull();
		expect(parseLogBatch({ lines: [] })).toEqual([]);
	});
});

describe('LogBuffer', () => {
	const line = (text: string) => ({ time: null, text });

	it('appends with keys that never repeat and keeps the last lines', () => {
		const buffer = new LogBuffer(3);
		let entries = buffer.append([], [line('a'), line('b')]);
		entries = buffer.gap(entries, 4);
		entries = buffer.append(entries, [line('c'), line('d')]);
		expect(entries.map((e) => e.text)).toEqual(['', 'c', 'd']);
		expect(entries[0]?.skipped).toBe(4);
		expect(entries.map((e) => e.key)).toEqual([2, 3, 4]);
		// Cleared (a resubscription), keys still go on.
		expect(buffer.append([], [line('e')])[0]?.key).toBe(5);
	});

	it('returns a new array every time and keeps 1000 lines by default', () => {
		const buffer = new LogBuffer();
		const before = buffer.append([], [line('a')]);
		const after = buffer.append(before, []);
		expect(after).not.toBe(before);
		const many = buffer.append(
			[],
			Array.from({ length: LOG_KEEP + 5 }, (_, i) => line(`${i}`))
		);
		expect(many).toHaveLength(LOG_KEEP);
		expect(many[0]?.text).toBe('5');
		expect(buffer.gap([], 0)[0]?.skipped).toBe(1);
	});
});

describe('logTime', () => {
	let tz: string | undefined;
	beforeEach(() => {
		tz = process.env.TZ;
		process.env.TZ = 'UTC';
	});
	afterEach(() => {
		process.env.TZ = tz;
	});

	it('shows the local time of a Python timestamp', () => {
		expect(logTime('2026-11-18 10:00:01.123456789+00:00')).toBe('10:00:01');
		expect(logTime('2026-11-18 10:00:01+02:00')).toBe('08:00:01');
		expect(logTime('2026-11-18T23:59:59Z')).toBe('23:59:59');
	});

	it('gives null for nothing or nonsense', () => {
		expect(logTime(null)).toBeNull();
		expect(logTime('')).toBeNull();
		expect(logTime('yesterday')).toBeNull();
	});
});

describe('pickContainer', () => {
	const c = (service: string, state: Container['state']): Container => ({
		id: `${service}-id`,
		service,
		image: null,
		state,
		mounts: []
	});

	it('prefers the asked service, then a running one, then the first', () => {
		const all = [c('web', 'exited'), c('db', 'running'), c('cache', 'running')];
		expect(pickContainer(all, 'web')?.service).toBe('web');
		expect(pickContainer(all, 'gone')?.service).toBe('cache');
		expect(pickContainer(all, null)?.service).toBe('cache');
		expect(pickContainer([c('x', 'crashed')], null)?.service).toBe('x');
		expect(pickContainer([], 'web')).toBeNull();
	});
});

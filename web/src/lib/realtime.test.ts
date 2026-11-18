import { describe, expect, it } from 'vitest';
import {
	appendSample,
	formatBytes,
	formatPercent,
	formatRate,
	memorySplit,
	parseSample,
	pickInterface,
	type NetStats,
	type Sample
} from './realtime';

const GiB = 1024 ** 3;

// What the server sends for its fixture (server/src/truenas/realtime.rs tests).
const fields = {
	cpu: 12.3,
	temp: 47.5,
	mem: { total: 64 * GiB, available: 28 * GiB, arc: 23 * GiB },
	net: {
		br0: { rx: 1201, tx: 340, up: true, speed: null },
		enp3s0: { rx: 1250, tx: 360, up: true, speed: 1000 },
		enp4s0: { rx: 0, tx: 0, up: false, speed: null }
	}
};

const iface = (up: boolean, speed: number | null): NetStats => ({ rx: 0, tx: 0, up, speed });

describe('parseSample', () => {
	it('reads the compact shape', () => {
		const s = parseSample(fields);
		expect(s).toEqual(fields);
	});

	it('accepts a null temperature and no interfaces', () => {
		expect(parseSample({ ...fields, temp: null, net: {} })).toMatchObject({ temp: null, net: {} });
	});

	it('refuses anything else', () => {
		for (const bad of [
			null,
			undefined,
			[],
			'x',
			{},
			{ ...fields, cpu: '12' },
			{ ...fields, cpu: NaN },
			{ ...fields, temp: undefined },
			{ ...fields, mem: { total: 1, available: 1 } },
			{ ...fields, net: [] },
			{ ...fields, net: { eth0: { rx: 1, tx: 1, up: 'yes', speed: null } } },
			{ ...fields, net: { eth0: { rx: 1, tx: 1, up: true } } }
		]) {
			expect(parseSample(bad), JSON.stringify(bad)).toBeNull();
		}
	});
});

describe('appendSample', () => {
	const s = (cpu: number) => ({ ...parseSample(fields)!, cpu }) as Sample;

	it('keeps the newest samples, oldest first, without touching the input', () => {
		let h: Sample[] = [];
		for (let i = 0; i < 5; i++) h = appendSample(h, s(i), 3);
		expect(h.map((x) => x.cpu)).toEqual([2, 3, 4]);
		const before = [...h];
		const next = appendSample(h, s(9), 3);
		expect(h).toEqual(before);
		expect(next.map((x) => x.cpu)).toEqual([3, 4, 9]);
	});
});

describe('pickInterface', () => {
	it('keeps the saved interface while it exists', () => {
		const net = { a: iface(false, null), b: iface(true, 1000) };
		expect(pickInterface(net, 'a')).toBe('a');
		expect(pickInterface(net, 'gone')).toBe('b');
	});

	it('takes the fastest link that is up, ties by name', () => {
		expect(pickInterface({ z: iface(true, 1000), a: iface(true, 1000), b: iface(true, 100) })).toBe(
			'a'
		);
		expect(pickInterface({ fast: iface(false, 10000), slow: iface(true, 100) })).toBe('slow');
		expect(pickInterface({ br0: iface(true, null), eno1: iface(true, 1000) })).toBe('eno1');
	});

	it('falls back to the first name, then null', () => {
		expect(pickInterface({ b: iface(false, null), a: iface(false, null) })).toBe('a');
		expect(pickInterface({})).toBeNull();
	});
});

describe('memorySplit', () => {
	it('splits total into services, ZFS cache and free', () => {
		expect(memorySplit(fields.mem)).toEqual({
			total: 64 * GiB,
			services: 13 * GiB,
			arc: 23 * GiB,
			free: 28 * GiB
		});
	});

	it('never goes negative when the numbers disagree', () => {
		const m = memorySplit({ total: 10, available: 8, arc: 5 });
		expect(m).toEqual({ total: 10, services: 0, arc: 2, free: 8 });
		expect(memorySplit({ total: 10, available: 20, arc: 0 }).free).toBe(10);
	});
});

describe('formatting', () => {
	it('rates use SI units', () => {
		expect(formatRate(0)).toBe('0 B/s');
		expect(formatRate(999)).toBe('999 B/s');
		expect(formatRate(1000)).toBe('1 kB/s');
		expect(formatRate(1_234_567)).toBe('1.2 MB/s');
		expect(formatRate(98_700_000)).toBe('98.7 MB/s');
		expect(formatRate(123_400_000)).toBe('123 MB/s');
		expect(formatRate(-5)).toBe('0 B/s');
	});

	it('bytes use binary units', () => {
		expect(formatBytes(512)).toBe('512 B');
		expect(formatBytes(12.4 * GiB)).toBe('12.4 GiB');
		expect(formatBytes(64 * GiB)).toBe('64 GiB');
		expect(formatBytes(2048 * GiB)).toBe('2 TiB');
		expect(formatBytes(1023.99 * GiB)).toBe('1024 GiB');
	});

	it('percentages', () => {
		expect(formatPercent(0)).toBe('0%');
		expect(formatPercent(7.46)).toBe('7.5%');
		expect(formatPercent(42.4)).toBe('42%');
		expect(formatPercent(100.2)).toBe('100%');
	});
});

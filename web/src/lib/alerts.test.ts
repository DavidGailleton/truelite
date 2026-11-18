import { describe, expect, it } from 'vitest';
import {
	ago,
	applyAlertEvent,
	parseAlert,
	parseAlerts,
	plainText,
	sortAlerts,
	summarizeAlerts,
	toneOf,
	type Alert
} from './alerts';

const T0 = Date.UTC(2026, 10, 18, 8, 0, 0);

const raw = (over: Record<string, unknown> = {}) => ({
	uuid: 'u1',
	id: 'u1',
	klass: 'VolumeStatus',
	level: 'CRITICAL',
	formatted: 'Pool tank state is DEGRADED',
	text: 'Pool %(volume)s state is %(state)s',
	datetime: { $date: T0 },
	last_occurrence: { $date: T0 + 60_000 },
	dismissed: false,
	one_shot: false,
	...over
});

const alert = (uuid: string, tone: Alert['tone'], lastMs = T0, dismissed = false): Alert => ({
	uuid,
	tone,
	message: uuid,
	since: new Date(T0),
	last: new Date(lastMs),
	dismissed
});

describe('toneOf', () => {
	it('maps every middleware level', () => {
		for (const [level, tone] of [
			['EMERGENCY', 'critical'],
			['ALERT', 'critical'],
			['CRITICAL', 'critical'],
			['ERROR', 'critical'],
			['WARNING', 'warning'],
			['NOTICE', 'info'],
			['INFO', 'info'],
			['warning', 'warning']
		]) {
			expect(toneOf(level), level).toBe(tone);
		}
	});

	it('treats an unknown level as a warning', () => {
		expect(toneOf('SEVERE')).toBe('warning');
	});
});

describe('plainText', () => {
	it('turns breaks and list items into lines', () => {
		expect(plainText('Pool is DEGRADED.<br>Devices:<ul><li>sda</li><li>sdb</li></ul>')).toBe(
			'Pool is DEGRADED.\nDevices:\n• sda\n• sdb'
		);
	});

	it('drops tags and keeps link text', () => {
		expect(plainText('See <a href="https://x/">the guide</a> now.')).toBe('See the guide now.');
	});

	it('decodes entities once', () => {
		expect(plainText('a &lt;b&gt; &amp;amp; &quot;c&quot; &#39;d&#39; &#x41;&nbsp;e')).toBe(
			'a <b> &amp; "c" \'d\' A e'
		);
		expect(plainText('&bogus; &#0;')).toBe('&bogus; &#0;');
	});

	it('drops scripts and styles with their content', () => {
		expect(plainText('ok<script>alert(1)</script><style>p{}</style> done')).toBe('ok done');
	});

	it('keeps escaped markup as text', () => {
		expect(plainText('&lt;img src=x onerror=alert(1)&gt;')).toBe('<img src=x onerror=alert(1)>');
	});
});

describe('parseAlert', () => {
	it('reads an entry with $date datetimes', () => {
		expect(parseAlert(raw())).toEqual({
			uuid: 'u1',
			tone: 'critical',
			message: 'Pool tank state is DEGRADED',
			since: new Date(T0),
			last: new Date(T0 + 60_000),
			dismissed: false
		});
	});

	it('reads ISO datetimes and falls back to datetime', () => {
		const a = parseAlert(raw({ datetime: '2026-11-18T08:00:00Z', last_occurrence: null }));
		expect(a?.since).toEqual(new Date(T0));
		expect(a?.last).toEqual(new Date(T0));
	});

	it('falls back to text, then klass, when formatted is missing', () => {
		expect(parseAlert(raw({ formatted: null }))?.message).toBe(
			'Pool %(volume)s state is %(state)s'
		);
		expect(parseAlert(raw({ formatted: null, text: '' }))?.message).toBe('VolumeStatus');
		expect(parseAlert(raw({ formatted: '<br>', text: null, klass: null }))?.message).toBe(
			'Unnamed alert'
		);
	});

	it('refuses entries without uuid or level', () => {
		expect(parseAlert(raw({ uuid: '' }))).toBeNull();
		expect(parseAlert(raw({ uuid: 3 }))).toBeNull();
		expect(parseAlert(raw({ level: null }))).toBeNull();
		expect(parseAlert('x')).toBeNull();
	});

	it('reads dismissed', () => {
		expect(parseAlert(raw({ dismissed: true }))?.dismissed).toBe(true);
	});
});

describe('parseAlerts', () => {
	it('drops unreadable entries', () => {
		expect(parseAlerts([raw(), 7, raw({ uuid: 'u2' })])?.map((a) => a.uuid)).toEqual(['u1', 'u2']);
	});

	it('refuses a non-list', () => {
		expect(parseAlerts({})).toBeNull();
	});
});

describe('applyAlertEvent', () => {
	const list = [alert('u1', 'critical'), alert('u2', 'warning')];

	it('adds a new alert', () => {
		const next = applyAlertEvent(list, { kind: 'added', id: 'u3', fields: raw({ uuid: 'u3' }) });
		expect(next?.map((a) => a.uuid)).toEqual(['u1', 'u2', 'u3']);
	});

	it('replaces a changed alert by uuid, whatever the event id', () => {
		const next = applyAlertEvent(list, {
			kind: 'changed',
			id: 42,
			fields: raw({ uuid: 'u2', dismissed: true })
		});
		expect(next?.map((a) => [a.uuid, a.dismissed])).toEqual([
			['u1', false],
			['u2', true]
		]);
	});

	it('removes by uuid', () => {
		expect(applyAlertEvent(list, { kind: 'removed', id: 'u1' })?.map((a) => a.uuid)).toEqual([
			'u2'
		]);
	});

	it('asks for a query when it cannot apply the event', () => {
		expect(applyAlertEvent(list, { kind: 'removed', id: 7 })).toBeNull();
		expect(applyAlertEvent(list, { kind: 'removed', id: 'gone' })).toBeNull();
		expect(applyAlertEvent(list, { kind: 'added', id: 'u3', fields: { n: 1 } })).toBeNull();
		expect(applyAlertEvent(list, { kind: 'other', id: 'u1' })).toBeNull();
	});
});

describe('sortAlerts and summarizeAlerts', () => {
	it('puts the loudest, then the newest first', () => {
		const sorted = sortAlerts([
			alert('info', 'info', T0 + 9),
			alert('old-warn', 'warning', T0),
			alert('crit', 'critical', T0),
			alert('new-warn', 'warning', T0 + 5)
		]);
		expect(sorted.map((a) => a.uuid)).toEqual(['crit', 'new-warn', 'old-warn', 'info']);
	});

	it('hides dismissed alerts and counts them', () => {
		const s = summarizeAlerts([
			alert('a', 'critical', T0, true),
			alert('b', 'info'),
			alert('c', 'warning')
		]);
		expect(s.active.map((a) => a.uuid)).toEqual(['c', 'b']);
		expect(s.dismissed).toBe(1);
		expect(s.worst).toBe('warning');
	});

	it('has no worst tone when nothing is active', () => {
		expect(summarizeAlerts([alert('a', 'critical', T0, true)]).worst).toBeNull();
		expect(summarizeAlerts([]).worst).toBeNull();
	});
});

describe('ago', () => {
	it('reads like a person would say it', () => {
		const at = (s: number) => ago(new Date(T0), T0 + s * 1000);
		expect(at(-5)).toBe('just now');
		expect(at(59)).toBe('just now');
		expect(at(60)).toBe('1 min ago');
		expect(at(3599)).toBe('59 min ago');
		expect(at(3600)).toBe('1 h ago');
		expect(at(86_400)).toBe('1 day ago');
		expect(at(12 * 86_400)).toBe('12 days ago');
	});
});

// Alerts from `alert.list` (api.truenas.com v25.10; role ALERT_LIST_READ). Framework-free so it
// can be unit-tested: the Svelte binding lives in alerts.svelte.ts. The server trims each alert
// to the fields read here (`server/src/truenas/alerts.rs`).
import { isObj, isStr, parseDate } from './system';
import type { EventData } from './ws';

export const ALERT_LIST = 'alert.list';

/** How loud an alert is: critical (red), warning (yellow), info (neutral). */
export type Tone = 'critical' | 'warning' | 'info';

const TONES: Record<string, Tone> = {
	EMERGENCY: 'critical',
	ALERT: 'critical',
	CRITICAL: 'critical',
	ERROR: 'critical',
	WARNING: 'warning',
	NOTICE: 'info',
	INFO: 'info'
};

/** The tone of a middleware level. An unknown level is a warning: better seen than missed. */
export function toneOf(level: string): Tone {
	return TONES[level.toUpperCase()] ?? 'warning';
}

/** Plain words for each tone, shown next to the colour. */
export const TONE_LABEL: Record<Tone, string> = {
	critical: 'Critical',
	warning: 'Warning',
	info: 'Info'
};

export interface Alert {
	uuid: string;
	tone: Tone;
	/** The message as plain text, line breaks kept. */
	message: string;
	/** When it was first raised. */
	since: Date | null;
	/** When it last happened. */
	last: Date | null;
	/** Dismissed by someone in TrueNAS. */
	dismissed: boolean;
}

const ENTITIES: Record<string, string> = {
	amp: '&',
	lt: '<',
	gt: '>',
	quot: '"',
	apos: "'",
	nbsp: ' '
};

/**
 * `formatted` as plain text. TrueNAS puts a little HTML in some messages (`<br>`, lists, links);
 * truelite never renders it as markup. Blocks and breaks become lines, tags are dropped, entities
 * decoded.
 */
export function plainText(html: string): string {
	return html
		.replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1\s*>/gi, '')
		.replace(/<br\s*\/?>|<\/?(p|div|li|ul|ol|h\d)\b[^>]*>/gi, (tag) =>
			/^<li\b/i.test(tag) ? '\n• ' : '\n'
		)
		.replace(/<[^>]*>/g, '')
		.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name: string) => {
			if (name[0] === '#') {
				const code =
					name[1] === 'x' || name[1] === 'X'
						? parseInt(name.slice(2), 16)
						: parseInt(name.slice(1), 10);
				return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
			}
			return ENTITIES[name.toLowerCase()] ?? whole;
		})
		.replace(/[ \t]+\n/g, '\n')
		.replace(/\n{2,}/g, '\n')
		.trim();
}

/** One `alert.list` entry, or `null` when it lacks the fields the card needs. */
export function parseAlert(raw: unknown): Alert | null {
	if (!isObj(raw)) return null;
	const { uuid, level, formatted, text, klass } = raw;
	if (!isStr(uuid) || uuid === '' || !isStr(level)) return null;
	const message =
		(isStr(formatted) && plainText(formatted)) ||
		(isStr(text) && text.trim()) ||
		(isStr(klass) && klass) ||
		'Unnamed alert';
	return {
		uuid,
		tone: toneOf(level),
		message,
		since: parseDate(raw.datetime),
		last: parseDate(raw.last_occurrence) ?? parseDate(raw.datetime),
		dismissed: raw.dismissed === true
	};
}

/** The `alert.list` result; unreadable entries are dropped. `null` when it isn't a list. */
export function parseAlerts(raw: unknown): Alert[] | null {
	if (!Array.isArray(raw)) return null;
	return raw.map(parseAlert).filter((a): a is Alert => a !== null);
}

/** The alert list after an `alert.list` event, or `null` when it should be queried again. */
export function applyAlertEvent(alerts: readonly Alert[], event: EventData): Alert[] | null {
	if (event.kind === 'removed') {
		// The event id is the alert's uuid; anything else can't be matched: query again.
		const id = event.id;
		if (!isStr(id) || !alerts.some((a) => a.uuid === id)) return null;
		return alerts.filter((a) => a.uuid !== id);
	}
	if (event.kind !== 'added' && event.kind !== 'changed') return null;
	const alert = parseAlert(event.fields);
	if (!alert) return null;
	const at = alerts.findIndex((a) => a.uuid === alert.uuid);
	if (at < 0) return [...alerts, alert];
	return alerts.map((a, i) => (i === at ? alert : a));
}

const RANK: Record<Tone, number> = { critical: 0, warning: 1, info: 2 };

/** Loudest first, then the most recent first. */
export function sortAlerts(alerts: readonly Alert[]): Alert[] {
	return [...alerts].sort(
		(a, b) => RANK[a.tone] - RANK[b.tone] || (b.last?.getTime() ?? 0) - (a.last?.getTime() ?? 0)
	);
}

export interface AlertSummary {
	/** Not dismissed, loudest and newest first. */
	active: Alert[];
	/** How many were dismissed in TrueNAS (not shown). */
	dismissed: number;
	/** The loudest active tone; `null` when nothing is active. */
	worst: Tone | null;
}

/** What the dashboard shows. */
export function summarizeAlerts(alerts: readonly Alert[]): AlertSummary {
	const active = sortAlerts(alerts.filter((a) => !a.dismissed));
	return {
		active,
		dismissed: alerts.length - active.length,
		worst: active[0]?.tone ?? null
	};
}

/** `just now`, `5 min ago`, `3 h ago`, `1 day ago`, `12 days ago`. */
export function ago(date: Date, now: number): string {
	const s = Math.max(0, Math.floor((now - date.getTime()) / 1000));
	if (s < 60) return 'just now';
	if (s < 3600) return `${Math.floor(s / 60)} min ago`;
	if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
	const days = Math.floor(s / 86400);
	return `${days} ${days === 1 ? 'day' : 'days'} ago`;
}

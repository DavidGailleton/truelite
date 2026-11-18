// Relative times, shared by every card so one timestamp always reads the same.

/** `just now`, `5 min ago`, `3 h ago`, `yesterday`, `12 days ago`. Rounds down. */
export function ago(date: Date, now: number): string {
	const s = Math.max(0, Math.floor((now - date.getTime()) / 1000));
	if (s < 60) return 'just now';
	if (s < 3600) return `${Math.floor(s / 60)} min ago`;
	if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
	const days = Math.floor(s / 86400);
	return days === 1 ? 'yesterday' : `${days} days ago`;
}

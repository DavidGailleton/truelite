// Updating an app (#34). A newer catalog version, or newer images for custom apps, goes through
// `app.upgrade`; newer images of the same catalog version through `app.pull_images` (TrueNAS
// refuses `app.upgrade` without `upgrade_available`). Both are jobs (APPS_WRITE). The Update
// dialog shows `app.upgrade_summary` (APPS_READ): the versions and the catalog changelog.
// api.truenas.com v25.10; middlewared `plugins/apps/upgrade.py` (TS-25.10.8). Framework-free.
import { appVersion, type App } from './apps';
import { isObj, isStr } from './system';

export const UPGRADE_SUMMARY = 'app.upgrade_summary';

/** The NAS's changelog for apps that only have newer images: nothing worth showing. */
const IMAGES_ONLY_CHANGELOG = 'Image updates are available for this app';

/** Longest changelog shown, in characters. */
const CHANGELOG_MAX = 4000;

export interface UpgradeSummary {
	/** The app version the update installs (`10.11.2`); `null` if unknown. */
	to: string | null;
	/** Plain text; `null` when there is none. */
	changelog: string | null;
}

/** The `app.upgrade_summary` result, or `null` when it isn't an object. */
export function parseUpgradeSummary(raw: unknown): UpgradeSummary | null {
	if (!isObj(raw)) return null;
	return {
		to: appVersion(raw.upgrade_human_version, raw.upgrade_version),
		changelog: isStr(raw.changelog) ? changelogText(raw.changelog) : null
	};
}

const ENTITIES: Record<string, string> = {
	amp: '&',
	lt: '<',
	gt: '>',
	quot: '"',
	apos: "'",
	nbsp: ' '
};

function decodeEntities(text: string): string {
	return text.replace(/&(#x[0-9a-f]+|#\d+|[a-z]+);/gi, (whole, name: string) => {
		if (name[0] === '#') {
			const code =
				name[1] === 'x' || name[1] === 'X' ? parseInt(name.slice(2), 16) : parseInt(name.slice(1));
			return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
		}
		return ENTITIES[name.toLowerCase()] ?? whole;
	});
}

/**
 * The changelog as plain text: the catalog may send HTML. Tags are dropped (block ends become
 * line breaks, list items get a bullet), entities decoded, blank lines squeezed, long text cut.
 * The result is shown as text, never as HTML. `null` when nothing useful is left.
 */
export function changelogText(source: string): string | null {
	const text = decodeEntities(
		source
			.replace(/<(script|style)\b[\s\S]*?<\/\1\s*>/gi, '')
			.replace(/<li\b[^>]*>/gi, '\n• ')
			.replace(/<br\s*\/?>|<\/(p|div|li|ul|ol|h[1-6]|pre|blockquote|tr)\s*>/gi, '\n')
			.replace(/<[^>]*>/g, '')
	)
		.split('\n')
		.map((line) => line.replace(/[ \t\r\f\v]+/g, ' ').trim())
		.join('\n')
		.replace(/\n{3,}/g, '\n\n')
		.trim();
	if (text === '' || text === IMAGES_ONLY_CHANGELOG) return null;
	return text.length > CHANGELOG_MAX ? `${text.slice(0, CHANGELOG_MAX).trimEnd()}…` : text;
}

export interface UpdateCall {
	method: 'app.upgrade' | 'app.pull_images';
	params: unknown[];
}

/**
 * The job that updates `app`: `app.upgrade` to the latest version (the server always asks for
 * "latest"), with host-path snapshots if `snapshot`; or `app.pull_images` for newer images of
 * the same version (the server adds the redeploy).
 */
export function updateCall(app: Pick<App, 'id' | 'update'>, snapshot: boolean): UpdateCall {
	if (app.update === 'images') return { method: 'app.pull_images', params: [app.id] };
	return {
		method: 'app.upgrade',
		params: snapshot ? [app.id, { snapshot_hostpaths: true }] : [app.id]
	};
}

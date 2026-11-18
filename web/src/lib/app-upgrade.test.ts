import { describe, expect, it } from 'vitest';
import { changelogText, parseUpgradeSummary, updateCall } from './app-upgrade';

describe('parseUpgradeSummary', () => {
	it('reads the new app version and the changelog', () => {
		expect(
			parseUpgradeSummary({
				latest_version: '1.3.2',
				latest_human_version: '10.11.2_1.3.2',
				upgrade_version: '1.3.2',
				upgrade_human_version: '10.11.2_1.3.2',
				changelog: '<p>Fixes</p>',
				available_versions_for_upgrade: []
			})
		).toEqual({ to: '10.11.2', changelog: 'Fixes' });
	});

	it('copes with missing fields and odd input', () => {
		expect(parseUpgradeSummary({ changelog: null })).toEqual({ to: null, changelog: null });
		expect(parseUpgradeSummary(null)).toBeNull();
		expect(parseUpgradeSummary([])).toBeNull();
	});
});

describe('changelogText', () => {
	it('turns HTML into plain text with line breaks and bullets', () => {
		expect(
			changelogText(
				'<h2>1.3.2</h2>\n<ul>\n  <li>Bump <code>jellyfin</code> to 10.11.2</li>\n  <li>Fix &amp; tidy &lt;ports&gt;</li>\n</ul><p>See&nbsp;notes<br>below</p>'
			)
		).toBe('1.3.2\n\n• Bump jellyfin to 10.11.2\n\n• Fix & tidy <ports>\n\nSee notes\nbelow');
	});

	it('drops scripts and styles and keeps plain text as it is', () => {
		expect(changelogText('<script>alert(1)</script><style>p{}</style>ok')).toBe('ok');
		expect(changelogText('- one\n- two')).toBe('- one\n- two');
		expect(changelogText('&#x41;&#66;&bogus;')).toBe('AB&bogus;');
	});

	it('returns null for nothing useful', () => {
		expect(changelogText('  <p> </p> ')).toBeNull();
		expect(changelogText('Image updates are available for this app')).toBeNull();
	});

	it('cuts a very long changelog', () => {
		const text = changelogText('x'.repeat(5000));
		expect(text?.length).toBe(4001);
		expect(text?.endsWith('…')).toBe(true);
	});
});

describe('updateCall', () => {
	it('upgrades to a new version, with snapshots when asked', () => {
		expect(updateCall({ id: 'plex', update: 'version' }, false)).toEqual({
			method: 'app.upgrade',
			params: ['plex']
		});
		expect(updateCall({ id: 'plex', update: 'version' }, true)).toEqual({
			method: 'app.upgrade',
			params: ['plex', { snapshot_hostpaths: true }]
		});
	});

	it('pulls newer images of the same version', () => {
		expect(updateCall({ id: 'plex', update: 'images' }, true)).toEqual({
			method: 'app.pull_images',
			params: ['plex']
		});
	});
});

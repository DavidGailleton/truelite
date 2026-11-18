// The app's main navigation: the desktop icon rail and the mobile bottom tab bar.
import type { IconName } from './icons';

export interface NavItem {
	href: string;
	label: string;
	icon: IconName;
}

export const NAV_ITEMS: readonly NavItem[] = [
	{ href: '/', label: 'Dashboard', icon: 'layout-dashboard' },
	{ href: '/apps', label: 'Apps', icon: 'box' },
	{ href: '/storage', label: 'Storage', icon: 'hard-drive' },
	{ href: '/system', label: 'System', icon: 'server' }
];

/**
 * Whether the nav item `href` is the current page for `pathname`: `/` only matches itself, any
 * other item also matches the pages below it (`/apps/plex` is under Apps, `/appsx` is not).
 */
export function isActive(pathname: string, href: string): boolean {
	if (href === '/') return pathname === '/';
	const base = href.replace(/\/+$/, '');
	return pathname === base || pathname.startsWith(`${base}/`);
}

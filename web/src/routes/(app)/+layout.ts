// Every page behind the login: a visitor without a session goes to the login page first, so the
// shell never flashes for them. An unreachable server still gets the shell (it says so).
import { redirect } from '@sveltejs/kit';
import { restartLoginUrl } from '#lib/restart.js';
import { restart } from '#lib/restart.svelte.js';
import { loadSession } from '#lib/session.svelte.js';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ url, untrack }) => {
	if ((await loadSession()) !== 'out') return;
	// Untracked: reading the URL would re-run this load on every navigation. `url.hash` is off
	// limits in a load; `location` has it on a first load or reload, when it shows this URL.
	const next = untrack(() => {
		const hash = location.pathname === url.pathname ? location.hash : '';
		return url.pathname + url.search + hash;
	});
	// Reloaded during a NAS restart, once the session is gone: wait for the NAS on the login page.
	redirect(307, restart.wait ? restartLoginUrl(next) : `/login?next=${encodeURIComponent(next)}`);
};

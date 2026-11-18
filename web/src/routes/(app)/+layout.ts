// Every page behind the login: a visitor without a session goes to the login page first, so the
// shell never flashes for them. An unreachable server still gets the shell (it says so).
import { redirect } from '@sveltejs/kit';
import { loadSession } from '#lib/session.svelte.js';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ url }) => {
	if ((await loadSession()) === 'out') {
		redirect(307, `/login?next=${encodeURIComponent(url.pathname + url.search)}`);
	}
};

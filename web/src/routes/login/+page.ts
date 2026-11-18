// Already logged in: go straight to where the login would have led.
import { redirect } from '@sveltejs/kit';
import { safeNext } from '#lib/login.js';
import { loadSession } from '#lib/session.svelte.js';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ url }) => {
	if ((await loadSession()) === 'in') redirect(307, safeNext(url.searchParams.get('next')));
};

<script lang="ts">
	// Every page behind the login: navigation, connection banner, main landmark.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import AppShell from '#lib/components/AppShell.svelte';
	import { closeNas, connection } from '#lib/nas.svelte.js';
	import { clearSession } from '#lib/session.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	// The session ended while in use (expired, logged out elsewhere, server restarted): log in
	// again, then come back to this page.
	$effect(() => {
		if (connection.status !== 'logged-out') return;
		const next = encodeURIComponent(page.url.pathname + page.url.search);
		closeNas();
		clearSession();
		goto(`/login?ended=1&next=${next}`, { replaceState: true });
	});
</script>

<AppShell>
	{@render children()}
</AppShell>

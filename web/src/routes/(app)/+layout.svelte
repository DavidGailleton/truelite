<script lang="ts">
	// Every page behind the login: navigation, connection banner, main landmark.
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import AppShell from '#lib/components/AppShell.svelte';
	import RestartScreen from '#lib/components/RestartScreen.svelte';
	import { connection, nas } from '#lib/nas.svelte.js';
	import { restartLoginUrl, statusSignal } from '#lib/restart.js';
	import { endRestartWait, restart, useRestartWatch } from '#lib/restart.svelte.js';
	import { endSession, loadSession, session } from '#lib/session.svelte.js';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();

	// The session ended while in use (expired, logged out elsewhere, server restarted): log in
	// again, then come back to this page. During a NAS restart the login page first waits for
	// the NAS (a restart always ends the session: the NAS forgets truelite's token).
	$effect(() => {
		if (connection.status !== 'logged-out') return;
		const here = page.url.pathname + page.url.search + page.url.hash;
		endSession();
		const to = restart.wait
			? restartLoginUrl(here)
			: `/login?ended=1&next=${encodeURIComponent(here)}`;
		goto(to, { replaceState: true });
	});

	// Open the connection even while the restart screen hides the shell (after a reload): its
	// status says when the session ends.
	$effect(() => {
		nas();
	});

	// Follow the wait while the session lasts: the connection status is the server's own probe
	// of the NAS. The NAS seen down then up with the session still valid: it is back. A guess
	// the NAS never proves (still up after a grace) ends too. The rules live in restart.ts.
	useRestartWatch(() => statusSignal(connection.status));

	// The server was unreachable when the page opened: learn who is logged in once it answers.
	$effect(() => {
		if (connection.status === 'up' && !session.user) loadSession();
	});
</script>

{#if restart.wait}
	<RestartScreen wait={restart.wait} onstop={() => endRestartWait()} />
{:else}
	<AppShell>
		{@render children()}
	</AppShell>
{/if}

<script lang="ts">
	// Every page behind the login: navigation, connection banner, main landmark.
	import AppShell from '#lib/components/AppShell.svelte';
	import type { LayoutProps } from './$types';

	let { children }: LayoutProps = $props();
</script>

<AppShell>
	{@render children()}
</AppShell>

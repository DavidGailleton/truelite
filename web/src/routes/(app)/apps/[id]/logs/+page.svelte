<script lang="ts">
	// Container logs of one app: `/apps/<name>/logs?service=<container service>`.
	import { page } from '$app/state';
	import AppLogsView from '#lib/components/AppLogsView.svelte';

	const id = $derived(page.params.id ?? '');
</script>

<!-- A fresh view per app: one app's answers never land on another's page. -->
{#key id}
	<AppLogsView {id} />
{/key}

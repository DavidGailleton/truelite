<script lang="ts">
	// App detail: `/apps/<name>` (TrueNAS uses the app name as its id).
	import { page } from '$app/state';
	import AppDetailView from '#lib/components/AppDetailView.svelte';

	const id = $derived(page.params.id ?? '');
</script>

<!-- A fresh view per app: one app's answers never land on another's page. -->
{#key id}
	<AppDetailView {id} />
{/key}

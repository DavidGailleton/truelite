<script lang="ts">
	// One dataset's snapshots: `/storage/snapshots?dataset=<path>` (dataset paths hold `/`, so a
	// query string, not a route parameter). Reached from the dataset tree on the Storage page.
	import { page } from '$app/state';
	import EmptyState from '#lib/components/EmptyState.svelte';
	import PageHeader from '#lib/components/PageHeader.svelte';
	import SnapshotsView from '#lib/components/SnapshotsView.svelte';

	const dataset = $derived(page.url.searchParams.get('dataset') ?? '');
</script>

{#if dataset === ''}
	<PageHeader title="Snapshots" />
	<EmptyState
		icon="camera"
		title="Pick a dataset"
		text="Open a dataset's snapshots from the dataset list on the Storage page."
	>
		{#snippet action()}
			<a class="button-outline" href="/storage">Go to Storage</a>
		{/snippet}
	</EmptyState>
{:else}
	<!-- A fresh view per dataset: one dataset's answers never land on another's page. -->
	{#key dataset}
		<SnapshotsView {dataset} />
	{/key}
{/if}

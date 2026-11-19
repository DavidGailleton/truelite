<script lang="ts">
	// System page: updates (#40); reboot and shutdown come with #42.
	import PageHeader from '#lib/components/PageHeader.svelte';
	import UpdateCard from '#lib/components/UpdateCard.svelte';
	import { useSystemInfo } from '#lib/system.svelte.js';
	import { useUpdateStatus } from '#lib/update.svelte.js';

	useSystemInfo();
	const updateTopic = useUpdateStatus();
</script>

<PageHeader title="System" subtitle="Updates and power" />

<div class="grid">
	<UpdateCard onretry={updateTopic.retry} />
</div>

<style>
	.grid {
		display: grid;
		grid-template-columns: minmax(0, 1fr);
		gap: var(--space-4);
	}
	@media (min-width: 768px) {
		.grid {
			grid-template-columns: repeat(2, minmax(0, 1fr));
			gap: var(--space-6);
		}
	}
</style>

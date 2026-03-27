<script lang="ts">
  import { onMount } from 'svelte';
  import { experiments, type Experiment } from '$lib/api';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import ExperimentCard from '$lib/components/ExperimentCard.svelte';

  let items: Experiment[] = [];
  let error = '';

  onMount(async () => {
    try {
      const res = await experiments.list();
      items = res.experiments ?? [];
    } catch (e) {
      error = String(e);
    }
  });
</script>

{#if error}
  <ErrorCard message={error} />
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-flask text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No experiments yet.</p>
  </div>
{:else}
  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
    {#each items as exp}
      <ExperimentCard experiment={exp} />
    {/each}
  </div>
{/if}

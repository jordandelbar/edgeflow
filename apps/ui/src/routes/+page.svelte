<script lang="ts">
  import { onMount } from 'svelte';
  import { experiments, type Experiment } from '$lib/api';

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

  function fmt(ms: number) {
    return new Date(ms).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
  }
</script>

{#if error}
  <div class="flex items-center gap-2 text-red-600 bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-sm">
    <i class="fa-solid fa-circle-exclamation"></i>{error}
  </div>
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-flask text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No experiments yet.</p>
  </div>
{:else}
  <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
    {#each items as exp}
      <a
        href="/experiments/{exp.experiment_id}"
        class="block bg-white rounded-xl border border-gray-100 p-5 shadow-sm hover:shadow-md hover:-translate-y-0.5 transition-all duration-150 group"
      >
        <div class="flex items-start justify-between gap-2 mb-3">
          <div class="w-9 h-9 rounded-lg flex items-center justify-center flex-shrink-0" style="background:#edf4f1">
            <i class="fa-solid fa-flask text-sage text-sm"></i>
          </div>
          <span class="text-xs text-gray-400 font-mono mt-1">#{exp.experiment_id}</span>
        </div>
        <p class="font-semibold text-gray-800 group-hover:text-peach-dark transition-colors truncate">{exp.name}</p>
        <p class="text-xs text-gray-400 mt-1">{fmt(exp.creation_time)}</p>
      </a>
    {/each}
  </div>
{/if}

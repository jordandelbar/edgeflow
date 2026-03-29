<script lang="ts">
  import { onMount } from 'svelte';
  import { experiments, type Experiment } from '$lib/api';
  import ErrorCard from '$lib/components/ErrorCard.svelte';

  let items: Experiment[] = [];
  let error = '';

  onMount(async () => {
    try {
      const res = await experiments.list();
      items = (res.experiments ?? []).sort((a, b) => b.creation_time - a.creation_time);
    } catch (e) {
      error = String(e);
    }
  });

  function fmt(ms: number) {
    return new Date(ms).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
  }
</script>

{#if error}
  <ErrorCard message={error} />
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-flask text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No experiments yet.</p>
  </div>
{:else}
  <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b border-gray-100 text-left">
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Experiment</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden sm:table-cell">ID</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden md:table-cell">Created</th>
        </tr>
      </thead>
      <tbody>
        {#each items as exp (exp.experiment_id)}
          <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50 transition-colors">
            <td class="px-5 py-3.5">
              <a
                href="/experiments/{exp.experiment_id}"
                class="font-medium text-gray-800 hover:text-peach-dark transition-colors"
              >
                {exp.name}
              </a>
            </td>
            <td class="px-5 py-3.5 hidden sm:table-cell">
              <span class="font-mono text-xs text-gray-400">{exp.experiment_id}</span>
            </td>
            <td class="px-5 py-3.5 hidden md:table-cell text-gray-500 text-xs">
              {fmt(exp.creation_time)}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

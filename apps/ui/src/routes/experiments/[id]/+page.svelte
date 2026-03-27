<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { page } from '$app/stores';
  import { experiments, runs, type Experiment, type Run } from '$lib/api';

  let experiment: Experiment | null = null;
  let runList: Run[] = [];
  let error = '';
  let interval: ReturnType<typeof setInterval>;

  async function load() {
    const id = $page.params.id!;
    try {
      const [expRes, runsRes] = await Promise.all([
        experiments.get(id),
        runs.search([id]),
      ]);
      experiment = expRes.experiment;
      runList = (runsRes.runs ?? []).sort((a, b) => b.info.start_time - a.info.start_time);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    load();
    interval = setInterval(load, 10000);
  });

  onDestroy(() => clearInterval(interval));

  function duration(run: Run): string {
    if (!run.info.end_time) return '—';
    const s = (run.info.end_time - run.info.start_time) / 1000;
    return s < 60 ? `${s.toFixed(1)}s` : `${(s / 60).toFixed(1)}m`;
  }

  const statusStyle: Record<string, string> = {
    FINISHED: 'bg-sage-light/40 text-sage-dark',
    RUNNING:  'bg-peach-light/60 text-peach-dark',
    FAILED:   'bg-red-100 text-red-700',
    KILLED:   'bg-gray-100 text-gray-600',
  };

  $: metricKeys = [...new Set(runList.flatMap(r => r.data.metrics.map(m => m.key)))];

  function metricValue(run: Run, key: string): string {
    const m = run.data.metrics.find(m => m.key === key);
    if (m == null) return '—';
    return Number.isInteger(m.value) ? String(m.value) : m.value.toFixed(4);
  }
</script>

{#if error}
  <div class="flex items-center gap-2 text-red-600 bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-sm">
    <i class="fa-solid fa-circle-exclamation"></i>{error}
  </div>
{:else if experiment}
  <!-- Breadcrumb -->
  <div class="flex items-center gap-2 text-sm text-gray-400 mb-5">
    <a href="/" class="hover:text-gray-700 transition-colors">Experiments</a>
    <i class="fa-solid fa-chevron-right text-xs"></i>
    <span class="text-gray-700 font-medium">{experiment.name}</span>
  </div>

  <!-- Runs table -->
  {#if runList.length === 0}
    <div class="text-center py-16 text-gray-400">
      <i class="fa-solid fa-play-circle text-3xl mb-2 block opacity-30"></i>
      <p class="text-sm">No runs yet.</p>
    </div>
  {:else}
    <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
      <table class="w-full text-sm">
        <thead>
          <tr class="border-b border-gray-100 text-left">
            <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Run</th>
            <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Status</th>
            <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Started</th>
            <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Duration</th>
            {#each metricKeys as key}
              <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">{key}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each runList as run}
            <tr class="border-b border-gray-50 hover:bg-gray-50 transition-colors">
              <td class="px-5 py-3.5">
                <a
                  href="/experiments/{experiment.experiment_id}/runs/{run.info.run_id}"
                  class="font-medium text-gray-800 hover:text-peach-dark transition-colors"
                >
                  {run.info.run_name ?? run.info.run_id.slice(0, 8)}
                </a>
                <div class="text-xs text-gray-400 font-mono mt-0.5">{run.info.run_id.slice(0, 12)}</div>
              </td>
              <td class="px-5 py-3.5">
                <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {statusStyle[run.info.status] ?? 'bg-gray-100 text-gray-600'}">
                  {run.info.status}
                </span>
              </td>
              <td class="px-5 py-3.5 text-gray-500">
                {new Date(run.info.start_time).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' })}
              </td>
              <td class="px-5 py-3.5 text-gray-500 font-mono text-xs">{duration(run)}</td>
              {#each metricKeys as key}
                <td class="px-5 py-3.5 font-mono text-xs text-gray-700">{metricValue(run, key)}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/if}

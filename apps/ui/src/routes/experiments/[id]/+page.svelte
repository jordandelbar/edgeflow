<script lang="ts">
  import { experiments, runs, type Experiment, type Run } from '$lib/api';
  import { fmtDateTime } from '$lib/utils';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import BreadcrumbNav from '$lib/components/BreadcrumbNav.svelte';
  import StatusBadge from '$lib/components/StatusBadge.svelte';

  let { data }: { data: { id: string } } = $props();

  let experiment = $state<Experiment | null>(null);
  let runList    = $state<Run[]>([]);
  let error      = $state('');

  $effect(() => {
    const id = data.id;
    async function load() {
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
    load();
    const timer = setInterval(load, 10000);
    return () => clearInterval(timer);
  });

  function duration(run: Run): string {
    if (!run.info.end_time) return '—';
    const s = (run.info.end_time - run.info.start_time) / 1000;
    return s < 60 ? `${s.toFixed(1)}s` : `${(s / 60).toFixed(1)}m`;
  }

  let metricKeys = $derived([...new Set(runList.flatMap(r => r.data.metrics.map(m => m.key)))]);

  function metricValue(run: Run, key: string): string {
    const m = run.data.metrics.find(m => m.key === key);
    if (m == null) return '—';
    return Number.isInteger(m.value) ? String(m.value) : m.value.toFixed(4);
  }
</script>

{#if error}
  <ErrorCard message={error} />
{:else if experiment}
  <BreadcrumbNav items={[{ label: 'Experiments', href: '/experiments' }, { label: experiment.name }]} />

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
            {#each metricKeys as key (key)}
              <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">{key}</th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each runList as run (run.info.run_id)}
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
                <StatusBadge status={run.info.status} />
              </td>
              <td class="px-5 py-3.5 text-gray-500">
                {fmtDateTime(run.info.start_time)}
              </td>
              <td class="px-5 py-3.5 text-gray-500 font-mono text-xs">{duration(run)}</td>
              {#each metricKeys as key (key)}
                <td class="px-5 py-3.5 font-mono text-xs text-gray-700">{metricValue(run, key)}</td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/if}

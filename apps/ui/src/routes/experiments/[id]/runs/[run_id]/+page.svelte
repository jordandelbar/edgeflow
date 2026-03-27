<script lang="ts">
  import { onMount } from 'svelte';
  import { runs, metrics, artifacts, models, runTag, type Run, type Metric, type FileInfo } from '$lib/api';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import BreadcrumbNav from '$lib/components/BreadcrumbNav.svelte';
  import StatusBadge from '$lib/components/StatusBadge.svelte';

  export let data: { run_id: string };

  let run: Run | null = null;
  let metricKeys: string[] = [];
  let selectedMetric = '';
  let metricHistory: Metric[] = [];
  let fileList: FileInfo[] = [];
  let error = '';
  let promoting = false;
  let promoted = false;

  onMount(async () => {
    const run_id = data.run_id;
    try {
      const res = await runs.get(run_id);
      run = res.run;
      promoted = runTag(run, 'edgeflow.promoted') === 'true';
      metricKeys = [...new Set(run.data.metrics.map(m => m.key))];
      if (metricKeys.length > 0) {
        selectedMetric = metricKeys[0];
        await loadMetricHistory();
      }
      const artRes = await artifacts.list(run_id);
      fileList = artRes.files ?? [];
    } catch (e) {
      error = String(e);
    }
  });

  async function loadMetricHistory() {
    if (!run || !selectedMetric) return;
    const res = await metrics.getHistory(run.info.run_id, selectedMetric);
    metricHistory = res.metrics ?? [];
  }

  async function promote() {
    if (!run) return;
    promoting = true;
    try {
      await models.promote(run.info.run_id);
      promoted = true;
    } finally {
      promoting = false;
    }
  }
</script>

{#if error}
  <ErrorCard message={error} />
{:else if run}
  <BreadcrumbNav items={[
    { label: 'Experiments', href: '/experiments' },
    { label: run.info.experiment_id, href: `/experiments/${run.info.experiment_id}` },
    { label: run.info.run_name ?? run.info.run_id.slice(0, 8) },
  ]} />

  <!-- Header row -->
  <div class="flex items-start justify-between gap-4 mb-6">
    <div>
      <h1 class="text-xl font-bold text-gray-900">{run.info.run_name ?? run.info.run_id.slice(0, 8)}</h1>
      <div class="flex items-center gap-3 mt-1.5">
        <StatusBadge status={run.info.status} />
        <span class="text-xs text-gray-400">
          {new Date(run.info.start_time).toLocaleString('en-GB')}
        </span>
      </div>
    </div>

    {#if promoted}
      <div class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg bg-sage-light/30 text-sage-dark text-sm font-medium">
        <i class="fa-solid fa-check-circle text-xs"></i>
        Promoted to model
      </div>
    {:else if run.info.status === 'FINISHED'}
      <button
        on:click={promote}
        disabled={promoting}
        class="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
      >
        {#if promoting}
          <i class="fa-solid fa-spinner fa-spin text-xs"></i>
        {:else}
          <i class="fa-solid fa-arrow-up-right-dots text-xs"></i>
        {/if}
        Promote to Model
      </button>
    {/if}
  </div>

  <div class="space-y-5">

    <!-- Params -->
    <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
      <div class="px-5 py-3.5 border-b border-gray-100 flex items-center gap-2">
        <i class="fa-solid fa-sliders text-sage text-sm"></i>
        <h2 class="font-semibold text-gray-700 text-sm">Parameters</h2>
      </div>
      {#if run.data.params.length === 0}
        <p class="px-5 py-4 text-sm text-gray-400">None.</p>
      {:else}
        <table class="w-full text-sm">
          <tbody>
            {#each run.data.params as p}
              <tr class="border-b border-gray-50 last:border-0">
                <td class="px-5 py-2.5 font-medium text-gray-600 w-1/3 font-mono text-xs">{p.key}</td>
                <td class="px-5 py-2.5 text-gray-800 font-mono text-xs">{p.value}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <!-- Metrics -->
    <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
      <div class="px-5 py-3.5 border-b border-gray-100 flex items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <i class="fa-solid fa-chart-line text-sage text-sm"></i>
          <h2 class="font-semibold text-gray-700 text-sm">Metrics</h2>
        </div>
        {#if metricKeys.length > 1}
          <select
            bind:value={selectedMetric}
            on:change={loadMetricHistory}
            class="text-xs border border-gray-200 rounded-md px-2 py-1 text-gray-600 bg-white"
          >
            {#each metricKeys as key}<option value={key}>{key}</option>{/each}
          </select>
        {/if}
      </div>
      {#if metricKeys.length === 0}
        <p class="px-5 py-4 text-sm text-gray-400">None.</p>
      {:else}
        <table class="w-full text-sm">
          <thead>
            <tr class="border-b border-gray-100 text-left">
              <th class="px-5 py-2.5 text-xs font-semibold text-gray-400 uppercase tracking-wide">Step</th>
              <th class="px-5 py-2.5 text-xs font-semibold text-gray-400 uppercase tracking-wide">Value</th>
              <th class="px-5 py-2.5 text-xs font-semibold text-gray-400 uppercase tracking-wide">Timestamp</th>
            </tr>
          </thead>
          <tbody>
            {#each metricHistory as m}
              <tr class="border-b border-gray-50 last:border-0">
                <td class="px-5 py-2.5 text-gray-500 font-mono text-xs">{m.step}</td>
                <td class="px-5 py-2.5 text-gray-800 font-mono text-xs font-semibold">{m.value}</td>
                <td class="px-5 py-2.5 text-gray-400 text-xs">{new Date(m.timestamp).toLocaleString('en-GB')}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <!-- Artifacts -->
    <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
      <div class="px-5 py-3.5 border-b border-gray-100 flex items-center gap-2">
        <i class="fa-solid fa-box-archive text-sage text-sm"></i>
        <h2 class="font-semibold text-gray-700 text-sm">Artifacts</h2>
      </div>
      {#if fileList.length === 0}
        <p class="px-5 py-4 text-sm text-gray-400">None.</p>
      {:else}
        <ul class="divide-y divide-gray-50">
          {#each fileList as f}
            <li class="px-5 py-2.5 flex items-center gap-2.5 text-sm">
              {#if f.is_dir}
                <i class="fa-solid fa-folder text-peach-light w-4 text-center"></i>
                <span class="text-gray-600 font-mono text-xs">{f.path}</span>
              {:else}
                <i class="fa-solid fa-file text-gray-300 w-4 text-center"></i>
                <a
                  href="/api/2.0/mlflow/artifacts/get-artifact?run_id={run.info.run_id}&path={f.path}"
                  target="_blank"
                  class="text-sage-dark hover:text-sage font-mono text-xs hover:underline transition-colors"
                >
                  {f.path}
                </a>
                {#if f.file_size !== null}
                  <span class="text-gray-400 text-xs ml-auto">{(f.file_size / 1024).toFixed(1)} KB</span>
                {/if}
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>

  </div>
{/if}

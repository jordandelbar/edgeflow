<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { runs, metrics, artifacts, type Run, type Metric, type FileInfo } from '$lib/api';

  let run: Run | null = null;
  let metricKeys: string[] = [];
  let selectedMetric = '';
  let metricHistory: Metric[] = [];
  let fileList: FileInfo[] = [];
  let error = '';

  onMount(async () => {
    const run_id = $page.params.run_id;
    try {
      const res = await runs.get(run_id);
      run = res.run;
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
</script>

{#if error}
  <p class="error">{error}</p>
{:else if run}
  <h1>{run.info.run_name ?? run.info.run_id}</h1>
  <p class="meta">Status: {run.info.status} · Started: {new Date(run.info.start_time).toLocaleString()}</p>

  <section>
    <h2>Parameters</h2>
    {#if run.data.params.length === 0}
      <p>None.</p>
    {:else}
      <table>
        <thead><tr><th>Key</th><th>Value</th></tr></thead>
        <tbody>
          {#each run.data.params as p}
            <tr><td>{p.key}</td><td>{p.value}</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section>
    <h2>Metrics</h2>
    {#if metricKeys.length === 0}
      <p>None.</p>
    {:else}
      <label>
        Metric:
        <select bind:value={selectedMetric} on:change={loadMetricHistory}>
          {#each metricKeys as key}<option value={key}>{key}</option>{/each}
        </select>
      </label>
      <table>
        <thead><tr><th>Step</th><th>Value</th><th>Timestamp</th></tr></thead>
        <tbody>
          {#each metricHistory as m}
            <tr>
              <td>{m.step}</td>
              <td>{m.value}</td>
              <td>{new Date(m.timestamp).toLocaleString()}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>

  <section>
    <h2>Artifacts</h2>
    {#if fileList.length === 0}
      <p>None.</p>
    {:else}
      <ul>
        {#each fileList as f}
          <li>
            {#if f.is_dir}
              📁 {f.path}
            {:else}
              <a href="/api/2.0/mlflow/artifacts/get-artifact?run_id={run.info.run_id}&path={f.path}" target="_blank">
                {f.path}
              </a>
              {#if f.file_size !== null}
                <span class="meta">({(f.file_size / 1024).toFixed(1)} KB)</span>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  section { margin-top: 2rem; }
  table { border-collapse: collapse; width: 100%; }
  th, td { padding: 0.5rem 1rem; border-bottom: 1px solid #ddd; text-align: left; }
  .meta { color: #666; font-size: 0.85rem; }
  .error { color: red; }
  label { display: flex; align-items: center; gap: 0.5rem; margin-bottom: 0.75rem; }
</style>

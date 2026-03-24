<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/stores';
  import { experiments, runs, type Experiment, type Run } from '$lib/api';

  let experiment: Experiment | null = null;
  let runList: Run[] = [];
  let error = '';

  onMount(async () => {
    const id = $page.params.id;
    try {
      const [expRes, runsRes] = await Promise.all([
        experiments.get(id),
        runs.search([id]),
      ]);
      experiment = expRes.experiment;
      runList = runsRes.runs ?? [];
    } catch (e) {
      error = String(e);
    }
  });
</script>

{#if error}
  <p class="error">{error}</p>
{:else if experiment}
  <h1>{experiment.name}</h1>
  <p class="meta">ID: {experiment.experiment_id}</p>

  <h2>Runs</h2>
  {#if runList.length === 0}
    <p>No runs yet.</p>
  {:else}
    <table>
      <thead>
        <tr><th>Name</th><th>Status</th><th>Started</th><th>Duration</th></tr>
      </thead>
      <tbody>
        {#each runList as run}
          {@const duration = run.info.end_time
            ? `${((run.info.end_time - run.info.start_time) / 1000).toFixed(1)}s`
            : '—'}
          <tr>
            <td>
              <a href="/experiments/{experiment?.experiment_id}/runs/{run.info.run_id}">
                {run.info.run_name ?? run.info.run_id.slice(0, 8)}
              </a>
            </td>
            <td>{run.info.status}</td>
            <td>{new Date(run.info.start_time).toLocaleString()}</td>
            <td>{duration}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
{/if}

<style>
  table { border-collapse: collapse; width: 100%; }
  th, td { padding: 0.5rem 1rem; border-bottom: 1px solid #ddd; text-align: left; }
  .meta { color: #666; font-size: 0.85rem; }
  .error { color: red; }
</style>

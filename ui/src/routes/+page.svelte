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
</script>

<h1>Experiments</h1>

{#if error}
  <p class="error">{error}</p>
{:else if items.length === 0}
  <p>No experiments yet.</p>
{:else}
  <table>
    <thead>
      <tr><th>Name</th><th>ID</th><th>Created</th></tr>
    </thead>
    <tbody>
      {#each items as exp}
        <tr>
          <td><a href="/experiments/{exp.experiment_id}">{exp.name}</a></td>
          <td>{exp.experiment_id}</td>
          <td>{new Date(exp.creation_time).toLocaleString()}</td>
        </tr>
      {/each}
    </tbody>
  </table>
{/if}

<style>
  table { border-collapse: collapse; width: 100%; }
  th, td { padding: 0.5rem 1rem; border-bottom: 1px solid #ddd; text-align: left; }
  .error { color: red; }
</style>

<script lang="ts">
  import { onMount } from 'svelte';
  import { models, deployments, targets, modelName, type Run, type Deployment } from '$lib/api';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import DeployModal from '$lib/components/DeployModal.svelte';

  let items: Run[] = [];
  let knownTargets: { name: string; state: string; run_id: string; node: string | null }[] = [];
  let deployedOn: Record<string, string[]> = {};   // run_id → target names currently deployed
  let demoting: Record<string, boolean> = {};
  let deployModalRun: Run | null = null;
  let error = '';

  onMount(async () => {
    try {
      const [modelsRes, depsRes, tgRes] = await Promise.all([
        models.list(),
        deployments.list(),
        targets.list(),
      ]);
      items = modelsRes.runs ?? [];

      const targetNodeMap: Record<string, string | null> = {};
      for (const t of (tgRes.targets ?? [])) {
        targetNodeMap[t.target] = t.node;
      }

      const latestByTarget: Record<string, Deployment> = {};
      for (const d of (depsRes.deployments ?? [])) {
        if (!latestByTarget[d.target]) latestByTarget[d.target] = d;
      }
      knownTargets = Object.entries(latestByTarget).map(([name, d]) => ({
        name,
        state: d.state,
        run_id: d.run_id,
        node: targetNodeMap[name] ?? null,
      }));

      const newDeployedOn: Record<string, string[]> = {};
      for (const [target, d] of Object.entries(latestByTarget)) {
        if (d.state === 'deployed') {
          if (!newDeployedOn[d.run_id]) newDeployedOn[d.run_id] = [];
          newDeployedOn[d.run_id].push(target);
        }
      }
      deployedOn = newDeployedOn;
    } catch (e) {
      error = String(e);
    }
  });

  async function demote(run_id: string) {
    demoting[run_id] = true;
    demoting = demoting;
    try {
      await models.demote(run_id);
      items = items.filter(r => r.info.run_id !== run_id);
    } finally {
      demoting[run_id] = false;
      demoting = demoting;
    }
  }

  function onDeployed(e: CustomEvent<{ run_id: string; targets: string[] }>) {
    const { run_id, targets: newTargets } = e.detail;
    if (!deployedOn[run_id]) deployedOn[run_id] = [];
    for (const t of newTargets) {
      if (!deployedOn[run_id].includes(t)) deployedOn[run_id].push(t);
    }
    deployedOn = deployedOn;
  }
</script>

{#if deployModalRun}
  <DeployModal
    run={deployModalRun}
    {knownTargets}
    on:close={() => { deployModalRun = null; }}
    on:deployed={onDeployed}
  />
{/if}

{#if error}
  <ErrorCard message={error} />
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-brain text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No models yet.</p>
    <p class="text-xs mt-1">Promote a finished run from the Experiments section.</p>
  </div>
{:else}
  <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b border-gray-100 text-left">
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Model</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden sm:table-cell">Experiment</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden md:table-cell">Date</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden lg:table-cell">Metric</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Deployed on</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide text-right">Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each items as run}
          <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50/50 transition-colors">

            <!-- Model name -->
            <td class="px-5 py-3.5">
              <a
                href="/experiments/{run.info.experiment_id}/runs/{run.info.run_id}"
                class="font-medium text-gray-800 hover:text-peach-dark transition-colors"
              >
                {modelName(run)}
              </a>
              <div class="font-mono text-xs text-gray-400 mt-0.5">{run.info.run_id.slice(0, 12)}</div>
            </td>

            <!-- Experiment -->
            <td class="px-5 py-3.5 hidden sm:table-cell">
              <a href="/experiments/{run.info.experiment_id}" class="text-xs text-gray-500 hover:text-gray-700 transition-colors">
                <i class="fa-solid fa-flask mr-1"></i>{run.info.experiment_id}
              </a>
            </td>

            <!-- Date -->
            <td class="px-5 py-3.5 hidden md:table-cell text-xs text-gray-400">
              {new Date(run.info.start_time).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' })}
            </td>

            <!-- Primary metric -->
            <td class="px-5 py-3.5 hidden lg:table-cell font-mono text-xs text-gray-600">
              {#if run.data.metrics.length > 0}
                <span class="text-gray-400">{run.data.metrics[0].key}:</span>
                <strong>{run.data.metrics[0].value}</strong>
              {:else}
                <span class="text-gray-300">—</span>
              {/if}
            </td>

            <!-- Deployed on -->
            <td class="px-5 py-3.5">
              <div class="flex flex-wrap gap-1">
                {#each (deployedOn[run.info.run_id] ?? []) as target}
                  <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-sage-light/40 text-sage-dark">
                    <i class="fa-solid fa-circle text-xs"></i>{target}
                  </span>
                {/each}
              </div>
            </td>

            <!-- Actions -->
            <td class="px-5 py-3.5">
              <div class="flex items-center justify-end gap-1">
                <button
                  on:click={() => { deployModalRun = run; }}
                  title="Deploy"
                  class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold text-peach-dark hover:bg-peach-light/20 transition-colors"
                >
                  <i class="fa-solid fa-rocket text-xs"></i>Deploy
                </button>
                <button
                  on:click={() => demote(run.info.run_id)}
                  disabled={demoting[run.info.run_id]}
                  title="Demote — remove from Models"
                  class="p-1.5 rounded-lg text-gray-300 hover:text-red-400 hover:bg-red-50 transition-colors disabled:opacity-50"
                >
                  {#if demoting[run.info.run_id]}
                    <i class="fa-solid fa-spinner fa-spin text-xs"></i>
                  {:else}
                    <i class="fa-solid fa-circle-minus text-xs"></i>
                  {/if}
                </button>
              </div>
            </td>

          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{/if}

<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { registeredModels, type RegisteredModel, type Deployment } from '$lib/api';
  import { liveData, refreshLiveData } from '$lib/stores';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import DeployModal from '$lib/components/DeployModal.svelte';

  let items: RegisteredModel[] = [];
  let deployModalModel: RegisteredModel | null = null;
  let deletingModel: Record<string, boolean> = {};
  let error = '';
  let interval: ReturnType<typeof setInterval>;

  $: knownTargets = (() => {
    const targetMap: Record<string, typeof $liveData.targets[0]> = {};
    for (const t of $liveData.targets) targetMap[t.target] = t;
    const latestByTarget: Record<string, Deployment> = {};
    for (const d of $liveData.deployments) {
      if (!latestByTarget[d.target]) latestByTarget[d.target] = d;
    }
    return Object.entries(latestByTarget).map(([name, d]) => ({
      name, state: d.state,
      model_name: d.model_name ?? null,
      model_version: d.model_version ?? null,
      node: targetMap[name]?.node ?? null,
      sessions: targetMap[name]?.resources?.sessions ?? null,
    }));
  })();

  $: deployedOn = (() => {
    const result: Record<string, string[]> = {};
    const latestByTarget: Record<string, Deployment> = {};
    for (const d of $liveData.deployments) {
      if (!latestByTarget[d.target]) latestByTarget[d.target] = d;
    }
    for (const [tgt, d] of Object.entries(latestByTarget)) {
      if (d.state === 'deployed' && d.model_name && d.model_version) {
        const key = `${d.model_name}:${d.model_version}`;
        if (!result[key]) result[key] = [];
        result[key].push(tgt);
      }
    }
    return result;
  })();

  async function loadModels() {
    try {
      items = (await registeredModels.list()).registered_models ?? [];
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => { loadModels(); interval = setInterval(loadModels, 10000); });
  onDestroy(() => clearInterval(interval));

  async function deleteModel(name: string) {
    deletingModel[name] = true;
    deletingModel = deletingModel;
    try {
      await registeredModels.delete(name);
      items = items.filter(m => m.name !== name);
    } finally {
      deletingModel[name] = false;
      deletingModel = deletingModel;
    }
  }

  function latestVersion(model: RegisteredModel) {
    if (model.latest_versions.length === 0) return undefined;
    return model.latest_versions.reduce((a, b) =>
      parseInt(a.version) > parseInt(b.version) ? a : b
    );
  }

  function deployedTargets(model: RegisteredModel): string[] {
    const targets: string[] = [];
    for (const mv of model.latest_versions) {
      const key = `${mv.name}:${mv.version}`;
      for (const t of (deployedOn[key] ?? [])) {
        if (!targets.includes(t)) targets.push(t);
      }
    }
    return targets;
  }

  function onDeployed(_e: CustomEvent<{ run_id: string; targets: string[] }>) {
    refreshLiveData();
  }
</script>

{#if deployModalModel}
  <DeployModal
    registeredModel={deployModalModel}
    {knownTargets}
    on:close={() => { deployModalModel = null; }}
    on:deployed={onDeployed}
  />
{/if}

{#if error}
  <ErrorCard message={error} />
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-brain text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No registered models yet.</p>
    <p class="text-xs mt-1">Register a model from a finished run in the Experiments section.</p>
  </div>
{:else}
  <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b border-gray-100 text-left">
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Model</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden sm:table-cell">Versions</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden md:table-cell">Latest</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Deployed on</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide text-right">Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each items as model (model.name)}
          {@const mv = latestVersion(model)}
          <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50/50 transition-colors">

            <!-- Model name → detail page -->
            <td class="px-5 py-3.5">
              <a href="/models/{encodeURIComponent(model.name)}"
                class="font-medium text-gray-800 hover:text-peach-dark transition-colors">
                {model.name}
              </a>
            </td>

            <!-- Version count -->
            <td class="px-5 py-3.5 hidden sm:table-cell text-xs text-gray-500">
              {model.latest_versions.length}
            </td>

            <!-- Latest version + stage -->
            <td class="px-5 py-3.5 hidden md:table-cell">
              {#if mv}
                <span class="font-mono text-xs text-gray-600">v{mv.version}</span>
                <span class="ml-2 text-xs text-gray-400">{mv.current_stage}</span>
              {:else}
                <span class="text-gray-300 text-xs">—</span>
              {/if}
            </td>

            <!-- Deployed on (across all versions) -->
            <td class="px-5 py-3.5">
              <div class="flex flex-wrap gap-1">
                {#each deployedTargets(model) as target (target)}
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
                  on:click={() => { deployModalModel = model; }}
                  disabled={model.latest_versions.length === 0}
                  title="Deploy a version"
                  class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold text-peach-dark hover:bg-peach-light/20 transition-colors disabled:opacity-30"
                >
                  <i class="fa-solid fa-rocket text-xs"></i>Deploy
                </button>
                <button
                  on:click={() => deleteModel(model.name)}
                  disabled={deletingModel[model.name]}
                  title="Delete registered model"
                  class="p-1.5 rounded-lg text-gray-300 hover:text-red-400 hover:bg-red-50 transition-colors disabled:opacity-50"
                >
                  {#if deletingModel[model.name]}
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

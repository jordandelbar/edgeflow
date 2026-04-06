<script lang="ts">
  import { registeredModels, runs, type ModelVersion, type Run } from '$lib/api';
  import { liveData, refreshLiveData } from '$lib/stores';
  import { fmtDate } from '$lib/utils';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import BreadcrumbNav from '$lib/components/BreadcrumbNav.svelte';
  import DeployModal from '$lib/components/DeployModal.svelte';

  let { data }: { data: { name: string } } = $props();

  let versions           = $state<ModelVersion[]>([]);
  let runCache           = $state<Record<string, Run>>({});
  let deployModalVersion = $state<ModelVersion | null>(null);
  let editingStage       = $state<Record<string, boolean>>({});
  let stageInput         = $state<Record<string, string>>({});
  let savingStage        = $state<Record<string, boolean>>({});
  let deletingVersion    = $state<Record<string, boolean>>({});
  let error              = $state('');

  function mvKey(mv: ModelVersion) { return `${mv.name}:${mv.version}`; }

  $effect(() => {
    loadVersions();
    const timer = setInterval(loadVersions, 10000);
    return () => clearInterval(timer);
  });

  async function loadVersions() {
    try {
      const res = await registeredModels.listVersions(data.name);
      versions = res.model_versions ?? [];
    } catch (e) {
      error = String(e);
    }
  }

  async function getRun(run_id: string): Promise<Run | null> {
    if (runCache[run_id]) return runCache[run_id];
    try {
      const res = await runs.get(run_id);
      runCache[run_id] = res.run;
      return res.run;
    } catch { return null; }
  }

  function startEditStage(mv: ModelVersion) {
    const k = mvKey(mv);
    stageInput[k] = mv.current_stage;
    editingStage[k] = true;
  }

  async function saveStage(mv: ModelVersion) {
    const k = mvKey(mv);
    const newStage = stageInput[k]?.trim();
    if (!newStage) { editingStage[k] = false; return; }
    savingStage[k] = true;
    try {
      const res = await registeredModels.transitionStage(mv.name, mv.version, newStage);
      versions = versions.map(v =>
        v.name === mv.name && v.version === mv.version ? res.model_version : v
      );
      editingStage[k] = false;
    } catch (e) {
      error = String(e);
    } finally {
      savingStage[k] = false;
    }
  }

  async function deleteVersion(mv: ModelVersion) {
    const k = mvKey(mv);
    deletingVersion[k] = true;
    try {
      await registeredModels.deleteVersion(mv.name, mv.version);
      versions = versions.filter(v => !(v.name === mv.name && v.version === mv.version));
    } catch (e) {
      error = String(e);
    } finally {
      deletingVersion[k] = false;
    }
  }

  function stageBadge(stage: string) {
    return ({
      Production: { colour: 'bg-sage-light/40 text-sage-dark',  label: 'Production' },
      Staging:    { colour: 'bg-blue-50 text-blue-600',          label: 'Staging'    },
      Archived:   { colour: 'bg-gray-100 text-gray-400',         label: 'Archived'   },
      None:       { colour: 'bg-gray-50 text-gray-400',          label: 'None'       },
    } as Record<string, { colour: string; label: string }>)[stage]
      ?? { colour: 'bg-gray-50 text-gray-500', label: stage };
  }

  let knownTargets = $derived((() => {
    const targetMap: Record<string, typeof $liveData.targets[0]> = {};
    for (const t of $liveData.targets) targetMap[t.target] = t;
    const latestByTarget: Record<string, typeof $liveData.deployments[0]> = {};
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
  })());

  let deployedOn = $derived((() => {
    const result: Record<string, string[]> = {};
    const latestByTarget: Record<string, typeof $liveData.deployments[0]> = {};
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
  })());
</script>

{#if deployModalVersion}
  <DeployModal
    modelVersion={deployModalVersion}
    {knownTargets}
    onclose={() => { deployModalVersion = null; }}
    ondeployed={() => { refreshLiveData(); }}
  />
{/if}

<BreadcrumbNav items={[
  { label: 'Models', href: '/models' },
  { label: data.name },
]} />

{#if error}
  <ErrorCard message={error} />
{:else if versions.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-box-open text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No versions registered yet.</p>
  </div>
{:else}
  <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
    <table class="w-full text-sm">
      <thead>
        <tr class="border-b border-gray-100 text-left">
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Version</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Stage</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide hidden md:table-cell">Run</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide">Deployed on</th>
          <th class="px-5 py-3 font-semibold text-gray-500 text-xs uppercase tracking-wide text-right">Actions</th>
        </tr>
      </thead>
      <tbody>
        {#each versions as mv (mv.version)}
          {@const k = mvKey(mv)}
          <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50/50 transition-colors">

            <td class="px-5 py-3.5">
              <span class="font-mono text-sm font-semibold text-gray-800">v{mv.version}</span>
              <div class="text-xs text-gray-400 mt-0.5">
                {fmtDate(mv.creation_time)}
              </div>
            </td>

            <td class="px-5 py-3.5">
              {#if editingStage[k]}
                <div class="flex items-center gap-1.5">
                  <input
                    type="text"
                    bind:value={stageInput[k]}
                    onkeydown={(e) => { if (e.key === 'Enter') saveStage(mv); if (e.key === 'Escape') { editingStage[k] = false; } }}
                    placeholder="e.g. Production"
                    class="border border-gray-200 rounded-md px-2 py-0.5 text-xs w-28 focus:outline-none focus:ring-1 focus:ring-peach/50 focus:border-peach"
                  />
                  <button onclick={() => saveStage(mv)} disabled={savingStage[k]}
                    class="text-sage-dark hover:text-sage transition-colors disabled:opacity-50">
                    {#if savingStage[k]}
                      <i class="fa-solid fa-spinner fa-spin text-xs"></i>
                    {:else}
                      <i class="fa-solid fa-check text-xs"></i>
                    {/if}
                  </button>
                  <button onclick={() => { editingStage[k] = false; }}
                    class="text-gray-400 hover:text-gray-600 transition-colors">
                    <i class="fa-solid fa-xmark text-xs"></i>
                  </button>
                </div>
              {:else}
                {@const badge = stageBadge(mv.current_stage)}
                <button onclick={() => startEditStage(mv)}
                  class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium {badge.colour} hover:opacity-80 transition-opacity"
                  title="Click to change stage">
                  {badge.label}
                  <i class="fa-solid fa-pen text-[10px] opacity-50"></i>
                </button>
              {/if}
            </td>

            <td class="px-5 py-3.5 hidden md:table-cell">
              {#if mv.run_id}
                {#await getRun(mv.run_id)}
                  <span class="text-gray-300 text-xs animate-pulse">loading…</span>
                {:then run}
                  {#if run}
                    <a href="/experiments/{run.info.experiment_id}/runs/{run.info.run_id}"
                      class="text-xs text-gray-500 hover:text-peach-dark transition-colors">
                      <i class="fa-solid fa-flask mr-1 text-gray-300"></i>{run.info.run_name ?? run.info.run_id.slice(0, 8)}
                    </a>
                  {:else}
                    <span class="font-mono text-xs text-gray-400">{mv.run_id.slice(0, 8)}</span>
                  {/if}
                {:catch}
                  <span class="font-mono text-xs text-gray-400">{mv.run_id.slice(0, 8)}</span>
                {/await}
              {:else}
                <span class="text-gray-300 text-xs">—</span>
              {/if}
            </td>

            <td class="px-5 py-3.5">
              <div class="flex flex-wrap gap-1">
                {#each (deployedOn[k] ?? []) as target (target)}
                  <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-sage-light/40 text-sage-dark">
                    <i class="fa-solid fa-circle text-xs"></i>{target}
                  </span>
                {/each}
              </div>
            </td>

            <td class="px-5 py-3.5">
              <div class="flex items-center justify-end gap-1">
                <button
                  onclick={() => { deployModalVersion = mv; }}
                  title="Deploy this version"
                  class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold text-peach-dark hover:bg-peach-light/20 transition-colors"
                >
                  <i class="fa-solid fa-rocket text-xs"></i>Deploy
                </button>
                <button
                  onclick={() => deleteVersion(mv)}
                  disabled={deletingVersion[k]}
                  title="Delete this version"
                  class="p-1.5 rounded-lg text-gray-300 hover:text-red-400 hover:bg-red-50 transition-colors disabled:opacity-50"
                >
                  {#if deletingVersion[k]}
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

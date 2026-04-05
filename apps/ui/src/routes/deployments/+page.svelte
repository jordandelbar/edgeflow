<script lang="ts">
  import { targets, type Deployment, type ModelStatus, type TargetHealth, type TargetStats } from '$lib/api';
  import { liveData, refreshLiveData } from '$lib/stores';
  import { fmtDateTime } from '$lib/utils';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import DeployModal from '$lib/components/DeployModal.svelte';
  import TargetStatsView from '$lib/components/TargetStats.svelte';

  let byTarget: Record<string, Deployment[]> = {};
  let targetList: string[] = [];
  let targetHealth: Record<string, TargetHealth> = {};
  let modelStatus: Record<string, ModelStatus | null> = {};
  let targetStats:   Record<string, TargetStats | null> = {};
  let statsHistory:  Record<string, TargetStats[]> = {};
  let statsLoading:  Record<string, boolean> = {};
  let confirming: Record<string, boolean> = {};
  let tearing: Record<string, boolean> = {};

  let error   = '';
  let loading = true;

  // "New deployment" modal state
  let showDeployModal = false;

  $: processLiveData($liveData);

  function processLiveData(data: typeof $liveData) {
    error   = data.error;
    loading = !data.loaded;
    if (!data.loaded) return;

    const newByTarget: Record<string, Deployment[]> = {};
    for (const d of data.deployments) {
      if (!newByTarget[d.target]) newByTarget[d.target] = [];
      newByTarget[d.target].push(d);
    }
    const newTargetList = Object.keys(newByTarget);

    const newTargetHealth: Record<string, TargetHealth> = { ...targetHealth };
    for (const tg of data.targets) {
      newTargetHealth[tg.target] = tg.health;
    }
    targetHealth = newTargetHealth;

    for (const t of newTargetList) {
      if (!(t in modelStatus)) modelStatus[t] = null;
      if (!(t in confirming))  confirming[t]  = false;
      if (!(t in tearing))     tearing[t]     = false;
      if (!(t in statsLoading)) statsLoading[t] = true;
    }
    modelStatus  = modelStatus;
    confirming   = confirming;
    tearing      = tearing;
    statsLoading = statsLoading;

    byTarget   = newByTarget;
    targetList = newTargetList;

    newTargetList.forEach(t => {
      targets.model(t)
        .then(s => { modelStatus[t] = s; modelStatus = modelStatus; })
        .catch(() => { modelStatus[t] = null; modelStatus = modelStatus; });

      targets.stats(t)
        .then(s => {
          targetStats[t] = s;
          statsLoading[t] = false;
          const prev = statsHistory[t] ?? [];
          statsHistory[t] = [...prev, s].slice(-30);
          targetStats = targetStats; statsLoading = statsLoading; statsHistory = statsHistory;
        })
        .catch(() => { targetStats[t] = null; statsLoading[t] = false; targetStats = targetStats; statsLoading = statsLoading; });
    });
  }

  $: sortedTargets = [...targetList].sort((a, b) => a.localeCompare(b));

  async function teardown(t: string) {
    tearing[t] = true; tearing = tearing;
    try {
      await targets.teardown(t);
      delete byTarget[t]; byTarget = byTarget;
      targetList = targetList.filter(x => x !== t);
    } catch (e) {
      error = String(e);
    } finally {
      tearing[t] = false; confirming[t] = false;
      tearing = tearing; confirming = confirming;
    }
  }

  $: knownTargets = (() => {
    const latestByTarget: Record<string, Deployment> = {};
    for (const d of $liveData.deployments) {
      if (!latestByTarget[d.target]) latestByTarget[d.target] = d;
    }
    return Object.entries(latestByTarget).map(([name, d]) => ({
      name,
      state: d.state,
      model_name: d.model_name ?? null,
      model_version: d.model_version ?? null,
      node: $liveData.targets.find(t => t.target === name)?.node ?? null,
      sessions: $liveData.targets.find(t => t.target === name)?.resources?.sessions ?? null,
    }));
  })();

  const healthDot: Record<TargetHealth, { colour: string; label: string }> = {
    healthy:   { colour: 'text-sage',      label: 'healthy'   },
    stale:     { colour: 'text-amber-400', label: 'stale'     },
    unhealthy: { colour: 'text-red-400',   label: 'unhealthy' },
    unknown:   { colour: 'text-gray-300',  label: 'unknown'   },
  };
</script>

{#if showDeployModal}
  <DeployModal
    knownTargets={knownTargets}
    on:close={() => { showDeployModal = false; refreshLiveData(); }}
    on:deployed={() => { refreshLiveData(); }}
  />
{/if}

{#if error}
  <ErrorCard message={error} />
{:else if loading}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-spinner fa-spin text-2xl mb-3 block"></i>
  </div>
{:else if targetList.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-rocket text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No deployments yet.</p>
    <button
      on:click={() => { showDeployModal = true; }}
      class="mt-4 flex items-center gap-2 px-4 py-2 rounded-xl text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors mx-auto"
    >
      <i class="fa-solid fa-plus text-xs"></i>New deployment
    </button>
  </div>
{:else}
  <div class="flex items-center justify-between mb-4">
    <h1 class="text-sm font-semibold text-gray-500 uppercase tracking-wide">Deployments</h1>
    <button
      on:click={() => { showDeployModal = true; }}
      class="flex items-center gap-1.5 px-3 py-1.5 rounded-xl text-xs font-semibold bg-peach text-white hover:bg-peach-dark transition-colors"
    >
      <i class="fa-solid fa-plus text-xs"></i>New deployment
    </button>
  </div>

  <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
    <table class="w-full table-fixed">
      <thead>
        <tr class="border-b border-gray-50">
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 w-8"></th>
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400">Target</th>
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 hidden sm:table-cell w-56">Model</th>
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 hidden md:table-cell w-36">Since</th>
          <th class="px-4 py-2 text-right text-xs font-medium text-gray-400 w-28">Actions</th>
        </tr>
      </thead>
      <tbody>
    {#each sortedTargets as t (t)}
          {@const deps = byTarget[t]}
          {@const latest = deps[0]}
          {@const status = modelStatus[t]}
          {@const currentDep = status ? deps.find(d => d.deployment_id === status.deployment_id) : null}
          {@const hs = targetHealth[t] ?? 'unknown'}
          {@const podCount = $liveData.targets.find(x => x.target === t)?.pods?.length ?? 0}
          {@const tgt = $liveData.targets.find(x => x.target === t)}
          {@const stats = targetStats[t] ?? null}
          {@const hasStats = stats !== null || statsLoading[t]}

          <tr class="hover:bg-gray-50/50 transition-colors" class:border-b={!hasStats} class:border-gray-50={!hasStats}>
            <!-- Health dot + optional replica count -->
            <td class="pl-4 pr-2 py-3">
              <div class="flex flex-col items-center gap-0.5">
                <i class="fa-solid fa-circle text-xs {healthDot[hs].colour}" title={healthDot[hs].label}></i>
                {#if podCount > 1}
                  <span class="text-gray-400 font-mono leading-none" style="font-size:9px">{podCount}</span>
                {/if}
              </div>
            </td>

            <!-- Target name → link to detail -->
            <td class="px-4 py-3 truncate">
              <a href="/deployments/{encodeURIComponent(t)}"
                 class="font-medium text-sm text-gray-800 hover:text-sage-dark transition-colors">
                {t}
              </a>
            </td>

            <!-- Loaded model -->
            <td class="px-4 py-3 hidden sm:table-cell truncate">
              {#if currentDep?.model_name}
                <a href="/models/{encodeURIComponent(currentDep.model_name)}" class="text-xs text-sage-dark hover:underline">
                  <i class="fa-solid fa-brain text-sage mr-1"></i>{currentDep.model_name} <span class="font-mono">v{currentDep.model_version}</span>
                </a>
              {:else if status?.run_id}
                <span class="font-mono text-xs text-gray-600">
                  <i class="fa-solid fa-brain text-sage mr-1"></i>{status.run_id.slice(0, 12)}
                </span>
              {:else}
                <span class="text-xs text-gray-300 italic">no model</span>
              {/if}
            </td>

            <!-- Since -->
            <td class="px-4 py-3 hidden md:table-cell text-xs text-gray-400">
              {#if status?.loaded_at}
                {fmtDateTime(status.loaded_at)}
              {:else}
                {fmtDateTime(latest.created_at)}
              {/if}
            </td>

            <!-- Actions: only teardown -->
            <td class="px-4 py-3">
              <div class="flex items-center justify-end gap-1">
                {#if confirming[t]}
                  <button
                    on:click={() => teardown(t)}
                    disabled={tearing[t]}
                    class="flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-semibold bg-red-500 text-white hover:bg-red-600 transition-colors disabled:opacity-50"
                  >
                    {#if tearing[t]}<i class="fa-solid fa-spinner fa-spin"></i>{:else}Confirm{/if}
                  </button>
                  <button
                    on:click={() => { confirming[t] = false; confirming = confirming; }}
                    class="p-1.5 rounded-lg text-xs text-gray-400 hover:bg-gray-100 transition-colors"
                  >
                    <i class="fa-solid fa-xmark"></i>
                  </button>
                {:else}
                  <button
                    on:click={() => { confirming[t] = true; confirming = confirming; }}
                    title="Tear down target"
                    class="p-1.5 rounded-lg text-xs text-gray-300 hover:text-red-400 hover:bg-red-50 transition-colors"
                  >
                    <i class="fa-solid fa-trash"></i>
                  </button>
                {/if}
              </div>
            </td>
          </tr>

          <!-- Live stats row — only shown when Prometheus is configured -->
          {#if hasStats}
            <tr class="border-b border-gray-50 last:border-0">
              <td></td>
              <td colspan="4" class="px-4 pb-2.5">
                <TargetStatsView
                  {stats}
                  history={statsHistory[t] ?? []}
                  infra={tgt?.infra ?? null}
                  loading={statsLoading[t]}
                />
              </td>
            </tr>
          {/if}
    {/each}
      </tbody>
    </table>
  </div>
{/if}

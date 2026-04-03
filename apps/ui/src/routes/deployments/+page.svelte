<script lang="ts">
  import { runs, targets, type Deployment, type ModelStatus, type TargetHealth, type Target, type TargetPod, type ResourceSettings, type InfraSettings } from '$lib/api';
  import { liveData } from '$lib/stores';
  import { fmtDateTime, fmtAgo } from '$lib/utils';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import DeployStateBadge from '$lib/components/DeployStateBadge.svelte';

  let byTarget: Record<string, Deployment[]> = {};
  let targetList: string[] = [];
  let targetHealth: Record<string, TargetHealth> = {};
  let lastSeenAt: Record<string, number | null> = {};
  let modelStatus: Record<string, ModelStatus | null> = {};
  let runExpId: Record<string, string> = {};  // run_id → experiment_id for linking
  let expanded: Record<string, 'history' | 'test' | 'inspect' | null> = {};
  let targetMap: Record<string, Target> = {};
  let confirming: Record<string, boolean> = {};
  let tearing: Record<string, boolean> = {};

  let error   = '';
  let loading = true;

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

    // Build fresh objects so mutations don't trigger reactive loops
    const newTargetHealth: Record<string, TargetHealth>   = { ...targetHealth };
    const newLastSeenAt:   Record<string, number | null>  = { ...lastSeenAt };
    const newTargetMap:    Record<string, Target>         = { ...targetMap };
    for (const tg of data.targets) {
      newTargetHealth[tg.target]  = tg.health;
      newLastSeenAt[tg.target]    = tg.last_seen;
      newTargetMap[tg.target]     = tg;
    }
    targetHealth  = newTargetHealth;
    lastSeenAt    = newLastSeenAt;
    targetMap     = newTargetMap;

    // Init state only for targets we haven't seen before — preserves
    // expanded panels and playground inputs across refreshes.
    for (const t of newTargetList) {
      if (!(t in expanded))    expanded[t]    = null;
      if (!(t in modelStatus)) modelStatus[t] = null;
      if (!(t in confirming))  confirming[t]  = false;
      if (!(t in tearing))     tearing[t]     = false;
      if (!(t in playground))  playground[t]  = { inputs: [], nFeatures: null, featureNames: [], result: null, err: '', running: false };
    }
    expanded    = expanded;
    modelStatus = modelStatus;
    confirming  = confirming;
    tearing     = tearing;
    playground  = playground;

    byTarget   = newByTarget;
    targetList = newTargetList;

    // Re-probe all targets without resetting to 'checking' — avoids flicker.
    newTargetList.forEach(t => {
      targets.model(t)
        .then(s => {
          modelStatus[t] = s;
          modelStatus = modelStatus;
          if (s?.run_id && !(s.run_id in runExpId)) {
            runs.get(s.run_id)
              .then(r => { runExpId[s.run_id] = r.run.info.experiment_id; runExpId = runExpId; })
              .catch(() => {});
          }
        })
        .catch(() => { modelStatus[t] = null; modelStatus = modelStatus; });
    });
  }

  $: sortedTargets = [...targetList].sort((a, b) => a.localeCompare(b));

  // Per-target playground state
  type Playground = {
    inputs: number[];
    nFeatures: number | null;  // null = still loading params
    featureNames: string[];    // empty = use "Input N" labels
    result: Record<string, unknown> | null;
    err: string;
    running: boolean;
  };
  let playground: Record<string, Playground> = {};

  async function toggle(t: string, panel: 'history' | 'test' | 'inspect') {
    expanded[t] = expanded[t] === panel ? null : panel;
    expanded = expanded;

    // When opening the playground, load run params to discover n_features
    if (panel === 'test' && expanded[t] === 'test') {
      const status = modelStatus[t];
      const pg = playground[t];
      if (status?.run_id && pg.nFeatures === null) {
        try {
          const res = await runs.get(status.run_id);
          const params = res.run.data.params;
          const nf = parseInt(params.find(p => p.key === 'n_features')?.value ?? '', 10);
          const featuresParam = params.find(p => p.key === 'features')?.value ?? '';
          const featureNames = featuresParam ? featuresParam.split(',').map(s => s.trim()) : [];
          const n = isNaN(nf) ? 4 : nf;
          pg.nFeatures = n;
          pg.featureNames = featureNames;
          pg.inputs = Array(n).fill(0);
        } catch {
          pg.nFeatures = 4;
          pg.inputs = Array(4).fill(0);
        }
        playground = playground;
      }
    }
  }

  async function runPlayground(t: string) {
    const p = playground[t];
    p.err = '';
    p.result = null;

    const n = p.nFeatures ?? p.inputs.length;
    if (n === 0) { p.err = 'No inputs configured.'; playground = playground; return; }

    const data = p.inputs.slice(0, n);

    p.running = true;
    playground = playground;
    try {
      const res = await targets.playground(t, data);
      p.result = res;
    } catch (e) {
      p.err = String(e);
    } finally {
      p.running = false;
      playground = playground;
    }
  }

  async function teardown(t: string) {
    tearing[t] = true; tearing = tearing;
    try {
      await targets.teardown(t);
      // Remove from local state immediately — next load() will confirm.
      delete byTarget[t]; byTarget = byTarget;
      targetList = targetList.filter(x => x !== t);
    } catch (e) {
      error = String(e);
    } finally {
      tearing[t] = false; confirming[t] = false;
      tearing = tearing; confirming = confirming;
    }
  }

  // Per-target resource edit state
  let editingResources: Record<string, boolean> = {};
  let resourceDraft: Record<string, ResourceSettings> = {};
  let infraDraft: Record<string, InfraSettings> = {};
  let resourceSaving: Record<string, boolean> = {};
  let resourceError: Record<string, string> = {};
  let resourceNotice: Record<string, string> = {};

  function startEditResources(t: string) {
    const res = targetMap[t]?.resources;
    const inf = targetMap[t]?.infra;
    resourceDraft[t] = {
      sessions:       res?.sessions       ?? null,
      max_concurrent: res?.max_concurrent ?? null,
    };
    infraDraft[t] = {
      cpu_request:    inf?.cpu_request    ?? null,
      memory_request: inf?.memory_request ?? null,
      memory_limit:   inf?.memory_limit   ?? null,
      replicas:       inf?.replicas       ?? null,
      spread:         inf?.spread         ?? null,
      node_selector:  inf?.node_selector  ?? null,
    };
    resourceError[t] = '';
    editingResources[t] = true;
    editingResources = editingResources;
    resourceDraft = resourceDraft;
    infraDraft = infraDraft;
  }

  async function saveResources(t: string) {
    resourceSaving[t] = true; resourceSaving = resourceSaving;
    resourceError[t] = ''; resourceNotice[t] = '';
    try {
      const res = await targets.updateResources(t, resourceDraft[t], infraDraft[t]);
      targetMap[t] = res.target;
      targetMap = targetMap;
      editingResources[t] = false;
      editingResources = editingResources;
      if (!res.pod_restarted) {
        const draftInf = infraDraft[t];
        const prevInf  = res.target.infra;
        const needsRestart = draftInf.cpu_request    !== prevInf?.cpu_request
                          || draftInf.memory_request !== prevInf?.memory_request
                          || draftInf.memory_limit   !== prevInf?.memory_limit
                          || resourceDraft[t].max_concurrent !== res.target.resources?.max_concurrent;
        if (needsRestart) {
          resourceNotice[t] = 'Saved. CPU / memory / max_concurrent changes require a pod restart to take effect — k8s was not reachable.';
          resourceNotice = resourceNotice;
        }
      }
    } catch (e) {
      resourceError[t] = String(e);
    } finally {
      resourceSaving[t] = false; resourceSaving = resourceSaving;
    }
  }

  const healthDot: Record<TargetHealth, { colour: string; label: string }> = {
    healthy:   { colour: 'text-sage',      label: 'healthy'   },
    stale:     { colour: 'text-amber-400', label: 'stale'     },
    unhealthy: { colour: 'text-red-400',   label: 'unhealthy' },
    unknown:   { colour: 'text-gray-300',  label: 'unknown'   },
  };

  function podLabel(pod: TargetPod): string {
    // k8s pod names look like "edgeflow-inference-iris-<hash>"; trim the common prefix for display.
    return pod.pod_id.replace(/^edgeflow-inference-/, '');
  }
</script>

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
    <p class="text-xs mt-1">Deploy a model from the Models section.</p>
  </div>
{:else}
  <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
    <table class="w-full table-fixed">
      <thead>
        <tr class="border-b border-gray-50">
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 w-8"></th>
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400">Target</th>
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 hidden sm:table-cell w-56">Model</th>
          <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 hidden md:table-cell w-36">Since</th>
          <th class="px-4 py-2 text-right text-xs font-medium text-gray-400 w-36">Actions</th>
        </tr>
      </thead>
      <tbody>
    {#each sortedTargets as t (t)}
            {@const deps = byTarget[t]}
            {@const latest = deps[0]}
            {@const status = modelStatus[t]}
            {@const currentDep = status ? deps.find(d => d.deployment_id === status.deployment_id) : null}
            {@const pg = playground[t]}
            {@const ls = lastSeenAt[t]}
            {@const hs = targetHealth[t] ?? 'unknown'}

            <!-- Target row -->
            {@const podCount = targetMap[t]?.pods?.length ?? 0}
            <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50/50 transition-colors">
              <!-- Health dot + optional replica count -->
              <td class="pl-4 pr-2 py-3">
                <div class="flex flex-col items-center gap-0.5">
                  <i class="fa-solid fa-circle text-xs {healthDot[hs].colour}"
                     title="{healthDot[hs].label}{ls ? ` — heartbeat ${fmtAgo(ls)}` : ''}"></i>
                  {#if podCount > 1}
                    <span class="text-gray-400 font-mono leading-none" style="font-size:9px">{podCount}</span>
                  {/if}
                </div>
              </td>

              <!-- Target name -->
              <td class="px-4 py-3 truncate">
                <span class="font-medium text-sm text-gray-800">{t}</span>
              </td>

              <!-- Loaded model -->
              <td class="px-4 py-3 hidden sm:table-cell truncate">
                {#if currentDep?.model_name}
                  <a href="/models/{encodeURIComponent(currentDep.model_name)}" class="text-xs text-sage-dark hover:underline">
                    <i class="fa-solid fa-brain text-sage mr-1"></i>{currentDep.model_name} <span class="font-mono">v{currentDep.model_version}</span>
                  </a>
                {:else if status?.run_id}
                  {#if runExpId[status.run_id]}
                    <a href="/experiments/{runExpId[status.run_id]}/runs/{status.run_id}" class="font-mono text-xs text-sage-dark hover:underline">
                      <i class="fa-solid fa-brain text-sage mr-1"></i>{status.run_id.slice(0, 12)}
                    </a>
                  {:else}
                    <span class="font-mono text-xs text-gray-600">
                      <i class="fa-solid fa-brain text-sage mr-1"></i>{status.run_id.slice(0, 12)}
                    </span>
                  {/if}
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

              <!-- Actions -->
              <td class="px-4 py-3">
                <div class="flex items-center justify-end gap-1">

                  <!-- Inspect -->
                  <button
                    on:click={() => toggle(t, 'inspect')}
                    title="Inspect deployment"
                    class="p-1.5 rounded-lg text-xs transition-colors
                      {expanded[t] === 'inspect' ? 'bg-gray-200 text-gray-700' : 'text-gray-400 hover:text-gray-600 hover:bg-gray-100'}"
                  >
                    <i class="fa-solid fa-circle-info"></i>
                  </button>

                  <!-- Test — show for any state except unhealthy (pod is likely gone) -->
                  {#if hs !== 'unhealthy' && status?.run_id}
                    <button
                      on:click={() => toggle(t, 'test')}
                      title="Playground"
                      class="p-1.5 rounded-lg text-xs transition-colors
                        {expanded[t] === 'test' ? 'bg-peach text-white' : 'text-gray-400 hover:text-peach-dark hover:bg-peach-light/20'}"
                    >
                      <i class="fa-solid fa-flask-vial"></i>
                    </button>
                  {/if}

                  <!-- History -->
                  <button
                    on:click={() => toggle(t, 'history')}
                    title="Deployment history"
                    class="p-1.5 rounded-lg text-xs transition-colors
                      {expanded[t] === 'history' ? 'bg-gray-200 text-gray-700' : 'text-gray-400 hover:text-gray-600 hover:bg-gray-100'}"
                  >
                    <i class="fa-solid fa-clock-rotate-left"></i>
                  </button>

                  <!-- Teardown -->
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

            <!-- Playground panel (spans full row) -->
            {#if expanded[t] === 'test'}
              <tr class="border-b border-gray-50">
                <td colspan="5" class="px-4 py-4 bg-gray-50/50">
                  <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide mb-3">
                    <i class="fa-solid fa-triangle-exclamation text-peach mr-1"></i>Playground — not for production use
                  </p>

                  {#if pg.nFeatures === null}
                    <div class="flex items-center gap-2 text-xs text-gray-400">
                      <i class="fa-solid fa-spinner fa-spin"></i>Loading model inputs…
                    </div>
                  {:else}
                    <div class="flex items-end gap-2 flex-wrap">
                      {#each pg.inputs as _input, i (i)}
                        {@const label = pg.featureNames[i] ?? `Input ${i + 1}`}
                        <div class="flex-1 min-w-20">
                          <label for="input-{t}-{i}" class="block text-xs text-gray-500 mb-1 truncate" title={label}>{label}</label>
                          <input
                            id="input-{t}-{i}"
                            type="number"
                            step="any"
                            bind:value={playground[t].inputs[i]}
                            on:keydown={(e) => e.key === 'Enter' && runPlayground(t)}
                            class="w-full border border-gray-200 rounded-lg px-3 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
                          />
                        </div>
                      {/each}
                      <div class="flex items-end pb-0.5">
                        <button
                          on:click={() => runPlayground(t)}
                          disabled={pg.running}
                          class="flex items-center gap-2 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
                        >
                          {#if pg.running}
                            <i class="fa-solid fa-spinner fa-spin text-xs"></i>
                          {:else}
                            <i class="fa-solid fa-play text-xs"></i>
                          {/if}
                          Run
                        </button>
                      </div>
                    </div>
                  {/if}

                  {#if pg.err}
                    <p class="text-xs text-red-500 mt-2"><i class="fa-solid fa-circle-xmark mr-1"></i>{pg.err}</p>
                  {/if}
                  {#if pg.result}
                    <div class="mt-3 bg-white border border-gray-200 rounded-lg px-4 py-3 font-mono text-xs">
                      <pre class="text-gray-800 whitespace-pre-wrap">{JSON.stringify(pg.result, null, 2)}</pre>
                    </div>
                  {/if}
                </td>
              </tr>
            {/if}

            <!-- Inspect panel (spans full row) -->
            {#if expanded[t] === 'inspect'}
              {@const tgt = targetMap[t]}
              {@const res = tgt?.resources}
              {@const inf = tgt?.infra}
              {@const currentDep2 = status ? deps.find(d => d.deployment_id === status.deployment_id) : deps[0]}
              <tr class="border-b border-gray-50">
                <td colspan="5" class="px-4 py-4 bg-gray-50/50">
                  <div class="grid grid-cols-1 sm:grid-cols-2 gap-4">

                    <!-- Deployment -->
                    <div>
                      <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2">Deployment</p>
                      <dl class="space-y-1">
                        <div class="flex gap-2 text-xs">
                          <dt class="text-gray-400 w-28 shrink-0">ID</dt>
                          <dd class="font-mono text-gray-700 truncate">{currentDep2?.deployment_id ?? '—'}</dd>
                        </div>
                        <div class="flex gap-2 text-xs">
                          <dt class="text-gray-400 w-28 shrink-0">State</dt>
                          <dd class="text-gray-700">{currentDep2?.state ?? '—'}</dd>
                        </div>
                        {#if status?.run_id}
                          <div class="flex gap-2 text-xs">
                            <dt class="text-gray-400 w-28 shrink-0">Run</dt>
                            <dd class="font-mono text-gray-700 truncate">
                              {#if runExpId[status.run_id]}
                                <a href="/experiments/{runExpId[status.run_id]}/runs/{status.run_id}"
                                   class="text-sage-dark hover:underline">{status.run_id.slice(0, 16)}</a>
                              {:else}
                                {status.run_id.slice(0, 16)}
                              {/if}
                            </dd>
                          </div>
                        {/if}
                        {#if status?.loaded_at}
                          <div class="flex gap-2 text-xs">
                            <dt class="text-gray-400 w-28 shrink-0">Loaded at</dt>
                            <dd class="text-gray-700">{fmtDateTime(status.loaded_at)}</dd>
                          </div>
                        {/if}
                      </dl>
                    </div>

                    <!-- Resources -->
                    <div>
                      <div class="flex items-center justify-between mb-2">
                        <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide">Resources</p>
                        {#if !editingResources[t]}
                          <button
                            on:click={() => startEditResources(t)}
                            class="text-xs text-gray-400 hover:text-sage-dark transition-colors"
                            title="Edit resources"
                          >
                            <i class="fa-solid fa-pen text-xs"></i>
                          </button>
                        {/if}
                      </div>

                      {#if editingResources[t]}
                        <div class="space-y-2">
                          <div class="grid grid-cols-2 gap-2">
                            <div>
                              <label class="block text-xs text-gray-500 mb-1">Sessions</label>
                              <input type="number" min="1" bind:value={resourceDraft[t].sessions} placeholder="1"
                                class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                            </div>
                            <div>
                              <label class="block text-xs text-gray-500 mb-1">Max concurrent</label>
                              <input type="number" min="1" bind:value={resourceDraft[t].max_concurrent} placeholder="= sessions"
                                class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                            </div>
                            <div>
                              <label class="block text-xs text-gray-500 mb-1">CPU request</label>
                              <input type="text" bind:value={infraDraft[t].cpu_request} placeholder="100m"
                                class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                            </div>
                            <div>
                              <label class="block text-xs text-gray-500 mb-1">Memory request</label>
                              <input type="text" bind:value={infraDraft[t].memory_request} placeholder="256Mi"
                                class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                            </div>
                            <div>
                              <label class="block text-xs text-gray-500 mb-1">Memory limit</label>
                              <input type="text" bind:value={infraDraft[t].memory_limit} placeholder="512Mi"
                                class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                            </div>
                            <div>
                              <label class="block text-xs text-gray-500 mb-1">Replicas</label>
                              <input type="number" min="1" bind:value={infraDraft[t].replicas} placeholder="1"
                                class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                            </div>
                            <div class="col-span-2 flex items-center gap-2 pt-1">
                              <input type="checkbox" id="spread-{t}" bind:checked={infraDraft[t].spread}
                                class="rounded border-gray-300 text-sage focus:ring-sage/50" />
                              <label for="spread-{t}" class="text-xs text-gray-600 cursor-pointer">
                                Spread replicas across nodes
                                <span class="text-gray-400">(pod anti-affinity)</span>
                              </label>
                            </div>
                          </div>

                          {#if resourceError[t]}
                            <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{resourceError[t]}</p>
                          {/if}

                          <div class="flex items-center gap-2 pt-1">
                            <button
                              on:click={() => saveResources(t)}
                              disabled={resourceSaving[t]}
                              class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold bg-sage text-white hover:bg-sage-dark transition-colors disabled:opacity-50"
                            >
                              {#if resourceSaving[t]}
                                <i class="fa-solid fa-spinner fa-spin text-xs"></i>
                              {:else}
                                <i class="fa-solid fa-check text-xs"></i>
                              {/if}
                              Save
                            </button>
                            <button
                              on:click={() => { editingResources[t] = false; editingResources = editingResources; }}
                              class="text-xs text-gray-400 hover:text-gray-600 transition-colors"
                            >
                              Cancel
                            </button>
                            <span class="text-xs text-gray-400 ml-auto italic">
                              <i class="fa-solid fa-triangle-exclamation mr-1 text-amber-400"></i>CPU/memory changes restart the pod
                            </span>
                          </div>
                        </div>
                      {:else}
                        <dl class="space-y-1">
                          <div class="flex gap-2 text-xs">
                            <dt class="text-gray-400 w-28 shrink-0">Sessions</dt>
                            <dd class="font-mono text-gray-700">{res?.sessions ?? '—'}</dd>
                          </div>
                          <div class="flex gap-2 text-xs">
                            <dt class="text-gray-400 w-28 shrink-0">Max concurrent</dt>
                            <dd class="font-mono text-gray-700">{res?.max_concurrent ?? '—'}</dd>
                          </div>
                          {#if inf}
                            <div class="flex gap-2 text-xs">
                              <dt class="text-gray-400 w-28 shrink-0">CPU request</dt>
                              <dd class="font-mono text-gray-700">{inf.cpu_request ?? '—'}</dd>
                            </div>
                            <div class="flex gap-2 text-xs">
                              <dt class="text-gray-400 w-28 shrink-0">Memory request</dt>
                              <dd class="font-mono text-gray-700">{inf.memory_request ?? '—'}</dd>
                            </div>
                            <div class="flex gap-2 text-xs">
                              <dt class="text-gray-400 w-28 shrink-0">Memory limit</dt>
                              <dd class="font-mono text-gray-700">{inf.memory_limit ?? '—'}</dd>
                            </div>
                            {#if inf.replicas != null}
                              <div class="flex gap-2 text-xs">
                                <dt class="text-gray-400 w-28 shrink-0">Replicas</dt>
                                <dd class="font-mono text-gray-700">{inf.replicas}</dd>
                              </div>
                            {/if}
                            {#if inf.spread}
                              <div class="flex gap-2 text-xs">
                                <dt class="text-gray-400 w-28 shrink-0">Spread</dt>
                                <dd class="text-gray-700">anti-affinity</dd>
                              </div>
                            {/if}
                            {#if inf.node_selector}
                              <div class="flex gap-2 text-xs">
                                <dt class="text-gray-400 w-28 shrink-0">Node selector</dt>
                                <dd class="font-mono text-gray-700 truncate">
                                  {Object.entries(inf.node_selector).map(([k,v]) => `${k}=${v}`).join(', ')}
                                </dd>
                              </div>
                            {/if}
                          {:else}
                            <div class="flex gap-2 text-xs">
                              <dt class="text-gray-400 w-28 shrink-0 italic">k8s</dt>
                              <dd class="text-gray-400 italic">unreachable</dd>
                            </div>
                          {/if}
                        </dl>
                      {/if}
                    </div>

                  </div>

                  <!-- Pods -->
                  {#if tgt?.pods?.length}
                    <div class="mt-4">
                      <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide mb-2">
                        Pods
                        <span class="normal-case font-normal text-gray-300 ml-1">({tgt.pods.length})</span>
                      </p>
                      <div class="space-y-1">
                        {#each tgt.pods as pod (pod.pod_id)}
                          {@const ph = pod.health}
                          <div class="flex items-center gap-3 px-3 py-1.5 rounded-lg bg-white border border-gray-100 text-xs">
                            <i class="fa-solid fa-circle text-xs {healthDot[ph].colour} shrink-0"
                               title="{healthDot[ph].label}{pod.last_seen ? ` — ${fmtAgo(pod.last_seen)}` : ''}"></i>
                            <span class="font-mono text-gray-700 truncate flex-1" title={pod.pod_id}>{podLabel(pod)}</span>
                            {#if pod.node}
                              <span class="text-gray-400 shrink-0">{pod.node}</span>
                            {/if}
                            {#if pod.last_seen}
                              <span class="text-gray-300 shrink-0">{fmtAgo(pod.last_seen)}</span>
                            {/if}
                          </div>
                        {/each}
                      </div>
                    </div>
                  {/if}

                  {#if resourceNotice[t]}
                    <p class="text-xs text-amber-600 mt-3">
                      <i class="fa-solid fa-triangle-exclamation mr-1"></i>{resourceNotice[t]}
                    </p>
                  {/if}
                </td>
              </tr>
            {/if}

            <!-- History panel (spans full row) -->
            {#if expanded[t] === 'history'}
              <tr class="border-b border-gray-50">
                <td colspan="5" class="p-0">
                  <table class="w-full">
                    <thead>
                      <tr class="bg-gray-50 border-y border-gray-100">
                        <th class="px-8 py-2 text-left text-xs font-medium text-gray-400">Run</th>
                        <th class="px-4 py-2 text-left text-xs font-medium text-gray-400">State</th>
                        <th class="px-4 py-2 text-left text-xs font-medium text-gray-400">Deployed</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each deps as dep (dep.deployment_id)}
                        <tr class="border-b border-gray-50 last:border-0">
                          <td class="px-8 py-2 font-mono text-xs">
                            {#if dep.model_name}
                              <a href="/models" class="text-sage-dark hover:underline">
                                <i class="fa-solid fa-brain mr-1 text-sage opacity-60"></i>{dep.model_name} v{dep.model_version}
                              </a>
                            {:else if runExpId[dep.run_id]}
                              <a href="/experiments/{runExpId[dep.run_id]}/runs/{dep.run_id}" class="text-sage-dark hover:underline">{dep.run_id.slice(0, 12)}</a>
                            {:else}
                              <span class="text-gray-600">{dep.run_id.slice(0, 12)}</span>
                            {/if}
                          </td>
                          <td class="px-4 py-2"><DeployStateBadge state={dep.state} /></td>
                          <td class="px-4 py-2 text-xs text-gray-400">{fmtDateTime(dep.created_at)}</td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </td>
              </tr>
            {/if}

    {/each}
      </tbody>
    </table>
  </div>
{/if}

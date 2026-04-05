<script lang="ts">
  import { runs, targets, nodes, type Deployment, type ModelStatus, type TargetHealth, type Target, type TargetPod, type ResourceSettings, type InfraSettings } from '$lib/api';
  import { liveData, refreshLiveData } from '$lib/stores';
  import { fmtDateTime } from '$lib/utils';
  import BreadcrumbNav from '$lib/components/BreadcrumbNav.svelte';
  import DeployStateBadge from '$lib/components/DeployStateBadge.svelte';
  import ErrorCard from '$lib/components/ErrorCard.svelte';

  export let data: { target: string };
  const t = data.target;

  let tgt: Target | null = null;
  let deps: Deployment[] = [];
  let modelStatus: ModelStatus | null = null;
  let runExpId: Record<string, string> = {};
  let error = '';
  let loading = true;

  // Playground state
  type Playground = {
    inputs: number[];
    nFeatures: number | null;
    featureNames: string[];
    result: Record<string, unknown> | null;
    err: string;
    running: boolean;
  };
  let pg: Playground = { inputs: [], nFeatures: null, featureNames: [], result: null, err: '', running: false };

  // Active panel
  let panel: 'inspect' | 'test' | 'history' = 'inspect';

  $: processLiveData($liveData);

  function processLiveData(data: typeof $liveData) {
    error   = data.error;
    loading = !data.loaded;
    if (!data.loaded) return;

    const match = data.targets.find(x => x.target === t);
    if (match) tgt = match;

    deps = data.deployments
      .filter(d => d.target === t)
      .sort((a, b) => b.created_at - a.created_at);

    targets.model(t)
      .then(s => {
        modelStatus = s;
        if (s?.run_id && !(s.run_id in runExpId)) {
          runs.get(s.run_id)
            .then(r => { runExpId[s.run_id] = r.run.info.experiment_id; runExpId = runExpId; })
            .catch(() => {});
        }
      })
      .catch(() => { modelStatus = null; });
  }

  async function openTest() {
    panel = 'test';
    if (pg.nFeatures !== null) return;
    const status = modelStatus;
    if (!status?.run_id) return;
    try {
      const res = await runs.get(status.run_id);
      const params = res.run.data.params;
      const nf = parseInt(params.find(p => p.key === 'n_features')?.value ?? '', 10);
      const featuresParam = params.find(p => p.key === 'features')?.value ?? '';
      const featureNames = featuresParam ? featuresParam.split(',').map(s => s.trim()) : [];
      const n = isNaN(nf) ? 4 : nf;
      pg = { ...pg, nFeatures: n, featureNames, inputs: Array(n).fill(0) };
    } catch {
      pg = { ...pg, nFeatures: 4, inputs: Array(4).fill(0) };
    }
  }

  async function runPlayground() {
    pg = { ...pg, err: '', result: null, running: true };
    const n = pg.nFeatures ?? pg.inputs.length;
    if (n === 0) { pg = { ...pg, err: 'No inputs configured.', running: false }; return; }
    const data = pg.inputs.slice(0, n);
    try {
      const res = await targets.playground(t, data);
      pg = { ...pg, result: res };
    } catch (e) {
      pg = { ...pg, err: String(e) };
    } finally {
      pg = { ...pg, running: false };
    }
  }

  // Resource editing
  let editingResources = false;
  let resourceDraft: ResourceSettings = { sessions: null, max_concurrent: null };
  let infraDraft: InfraSettings = { cpu_request: null, memory_request: null, memory_limit: null, replicas: null, placement: null, node_selector: null };
  let pinnedNode = '';
  let availableNodes: string[] = [];
  let resourceSaving = false;
  let resourceError = '';
  let resourceNotice = '';

  async function startEditResources() {
    const res = tgt?.resources;
    const inf = tgt?.infra;
    resourceDraft = { sessions: res?.sessions ?? null, max_concurrent: res?.max_concurrent ?? null };
    infraDraft = {
      cpu_request:    inf?.cpu_request    ?? null,
      memory_request: inf?.memory_request ?? null,
      memory_limit:   inf?.memory_limit   ?? null,
      replicas:       inf?.replicas       ?? null,
      placement:      inf?.placement      ?? null,
      node_selector:  inf?.node_selector  ?? null,
    };
    pinnedNode = inf?.node_selector?.['kubernetes.io/hostname'] ?? '';
    resourceError = '';
    editingResources = true;
    try {
      const res2 = await nodes.list();
      availableNodes = res2.nodes ?? [];
    } catch { availableNodes = []; }
  }

  async function saveResources() {
    resourceSaving = true; resourceError = ''; resourceNotice = '';
    try {
      const infra = { ...infraDraft, node_selector: pinnedNode ? { 'kubernetes.io/hostname': pinnedNode } : null };
      const res = await targets.updateResources(t, resourceDraft, infra);
      tgt = res.target;
      editingResources = false;
      if (!res.pod_restarted) {
        const prevInf = res.target.infra;
        const needsRestart = infraDraft.cpu_request    !== prevInf?.cpu_request
                          || infraDraft.memory_request !== prevInf?.memory_request
                          || infraDraft.memory_limit   !== prevInf?.memory_limit
                          || resourceDraft.max_concurrent !== res.target.resources?.max_concurrent;
        if (needsRestart) resourceNotice = 'Saved. CPU / memory / max_concurrent changes require a pod restart to take effect — k8s was not reachable.';
      }
      refreshLiveData();
    } catch (e) {
      resourceError = String(e);
    } finally {
      resourceSaving = false;
    }
  }

  const healthDot: Record<TargetHealth, { colour: string; label: string }> = {
    healthy:   { colour: 'text-sage',      label: 'healthy'   },
    stale:     { colour: 'text-amber-400', label: 'stale'     },
    unhealthy: { colour: 'text-red-400',   label: 'unhealthy' },
    unknown:   { colour: 'text-gray-300',  label: 'unknown'   },
  };

  function podLabel(pod: TargetPod): string {
    return pod.pod_id.replace(/^edgeflow-inference-/, '');
  }

  $: hs = tgt?.health ?? 'unknown';
  $: currentDep = modelStatus ? deps.find(d => d.deployment_id === modelStatus!.deployment_id) ?? deps[0] : deps[0];
  $: inf = tgt?.infra;
  $: res = tgt?.resources;
</script>

<BreadcrumbNav items={[{ label: 'Deployments', href: '/deployments' }, { label: t }]} />

{#if error}
  <ErrorCard message={error} />
{:else if loading}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-spinner fa-spin text-2xl mb-3 block"></i>
  </div>
{:else}

  <!-- Header -->
  <div class="flex items-center justify-between mb-6">
    <div class="flex items-center gap-3">
      <i class="fa-solid fa-circle text-sm {healthDot[hs].colour}" title={healthDot[hs].label}></i>
      <h1 class="text-xl font-semibold text-gray-800">{t}</h1>
      {#if tgt?.pods && tgt.pods.length > 1}
        <span class="text-xs text-gray-400 font-mono">{tgt.pods.length} pods</span>
      {/if}
    </div>

    <!-- Tab switcher -->
    <div class="flex items-center gap-1 bg-gray-100 rounded-xl p-1">
      <button
        on:click={() => { panel = 'inspect'; }}
        class="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors
          {panel === 'inspect' ? 'bg-white shadow-sm text-gray-800' : 'text-gray-500 hover:text-gray-700'}"
      >
        <i class="fa-solid fa-circle-info mr-1.5"></i>Inspect
      </button>
      {#if hs !== 'unhealthy' && modelStatus?.run_id}
        <button
          on:click={openTest}
          class="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors
            {panel === 'test' ? 'bg-white shadow-sm text-gray-800' : 'text-gray-500 hover:text-gray-700'}"
        >
          <i class="fa-solid fa-flask-vial mr-1.5"></i>Playground
        </button>
      {/if}
      <button
        on:click={() => { panel = 'history'; }}
        class="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors
          {panel === 'history' ? 'bg-white shadow-sm text-gray-800' : 'text-gray-500 hover:text-gray-700'}"
      >
        <i class="fa-solid fa-clock-rotate-left mr-1.5"></i>History
      </button>
    </div>
  </div>

  <!-- ── Inspect panel ─────────────────────────────────────── -->
  {#if panel === 'inspect'}
    <div class="grid grid-cols-1 sm:grid-cols-2 gap-4 mb-4">

      <!-- Deployment -->
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm px-5 py-4">
        <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide mb-3">Deployment</p>
        <dl class="space-y-1.5">
          <div class="flex gap-2 text-xs">
            <dt class="text-gray-400 w-28 shrink-0">ID</dt>
            <dd class="font-mono text-gray-700 truncate">{currentDep?.deployment_id ?? '—'}</dd>
          </div>
          <div class="flex gap-2 text-xs">
            <dt class="text-gray-400 w-28 shrink-0">State</dt>
            <dd>{#if currentDep}<DeployStateBadge state={currentDep.state} />{:else}—{/if}</dd>
          </div>
          {#if currentDep?.model_name}
            <div class="flex gap-2 text-xs">
              <dt class="text-gray-400 w-28 shrink-0">Model</dt>
              <dd>
                <a href="/models/{encodeURIComponent(currentDep.model_name)}" class="text-sage-dark hover:underline">
                  {currentDep.model_name} <span class="font-mono">v{currentDep.model_version}</span>
                </a>
              </dd>
            </div>
          {/if}
          {#if modelStatus?.run_id}
            <div class="flex gap-2 text-xs">
              <dt class="text-gray-400 w-28 shrink-0">Run</dt>
              <dd class="font-mono text-gray-700 truncate">
                {#if runExpId[modelStatus.run_id]}
                  <a href="/experiments/{runExpId[modelStatus.run_id]}/runs/{modelStatus.run_id}"
                     class="text-sage-dark hover:underline">{modelStatus.run_id.slice(0, 16)}</a>
                {:else}
                  {modelStatus.run_id.slice(0, 16)}
                {/if}
              </dd>
            </div>
          {/if}
          {#if modelStatus?.loaded_at}
            <div class="flex gap-2 text-xs">
              <dt class="text-gray-400 w-28 shrink-0">Loaded at</dt>
              <dd class="text-gray-700">{fmtDateTime(modelStatus.loaded_at)}</dd>
            </div>
          {/if}
        </dl>
      </div>

      <!-- Resources -->
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm px-5 py-4">
        <div class="flex items-center justify-between mb-3">
          <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide">Resources</p>
          {#if !editingResources}
            <button
              on:click={startEditResources}
              class="text-xs text-gray-400 hover:text-sage-dark transition-colors"
              title="Edit resources"
            >
              <i class="fa-solid fa-pen text-xs"></i>
            </button>
          {/if}
        </div>

        {#if editingResources}
          <div class="space-y-2">
            <div class="grid grid-cols-2 gap-2">
              <div>
                <label class="block text-xs text-gray-500 mb-1">Sessions
                  <input type="number" min="1" bind:value={resourceDraft.sessions} placeholder="1"
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                </label>
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Max concurrent
                  <input type="number" min="1" bind:value={resourceDraft.max_concurrent} placeholder="= sessions"
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                </label>
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">CPU request
                  <input type="text" bind:value={infraDraft.cpu_request} placeholder="100m"
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                </label>
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Memory request
                  <input type="text" bind:value={infraDraft.memory_request} placeholder="256Mi"
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                </label>
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Memory limit
                  <input type="text" bind:value={infraDraft.memory_limit} placeholder="512Mi"
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                </label>
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Replicas
                  <input type="number" min="1" bind:value={infraDraft.replicas} placeholder="1"
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white" />
                </label>
              </div>
              <div class="col-span-2">
                <label class="block text-xs text-gray-500 mb-1">Placement
                  <select bind:value={infraDraft.placement}
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white">
                    <option value={null}>None — scheduler decides</option>
                    <option value="spread">Spread — anti-affinity (different nodes)</option>
                    <option value="pack">Pack — affinity (same node)</option>
                  </select>
                </label>
              </div>
              <div class="col-span-2">
                <label class="block text-xs text-gray-500 mb-1">Node pin
                  <select bind:value={pinnedNode}
                    class="w-full border border-gray-200 rounded px-2 py-1 text-xs focus:outline-none focus:ring-1 focus:ring-sage/50 bg-white">
                    <option value="">Any node</option>
                    {#each availableNodes as n (n)}
                      <option value={n}>{n}</option>
                    {/each}
                  </select>
                </label>
                <p class="text-xs text-gray-400 mt-0.5">Sets <code>kubernetes.io/hostname</code> — pin pods to a specific node (e.g. one with a GPU).</p>
              </div>
            </div>

            {#if resourceError}
              <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{resourceError}</p>
            {/if}

            <div class="flex items-center gap-2 pt-1">
              <button
                on:click={saveResources}
                disabled={resourceSaving}
                class="flex items-center gap-1.5 px-3 py-1 rounded-lg text-xs font-semibold bg-sage text-white hover:bg-sage-dark transition-colors disabled:opacity-50"
              >
                {#if resourceSaving}
                  <i class="fa-solid fa-spinner fa-spin text-xs"></i>
                {:else}
                  <i class="fa-solid fa-check text-xs"></i>
                {/if}
                Save
              </button>
              <button
                on:click={() => { editingResources = false; }}
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
          <dl class="space-y-1.5">
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
              {#if inf.placement}
                <div class="flex gap-2 text-xs">
                  <dt class="text-gray-400 w-28 shrink-0">Placement</dt>
                  <dd class="text-gray-700">{inf.placement === 'spread' ? 'Spread (anti-affinity)' : 'Pack (affinity)'}</dd>
                </div>
              {/if}
              {#if inf.node_selector?.['kubernetes.io/hostname']}
                <div class="flex gap-2 text-xs">
                  <dt class="text-gray-400 w-28 shrink-0">Node pin</dt>
                  <dd class="font-mono text-gray-700">{inf.node_selector['kubernetes.io/hostname']}</dd>
                </div>
              {/if}
              {#if inf.node_selector && Object.keys(inf.node_selector).some(k => k !== 'kubernetes.io/hostname')}
                <div class="flex gap-2 text-xs">
                  <dt class="text-gray-400 w-28 shrink-0">Node selector</dt>
                  <dd class="font-mono text-gray-700 truncate">
                    {Object.entries(inf.node_selector).filter(([k]) => k !== 'kubernetes.io/hostname').map(([k,v]) => `${k}=${v}`).join(', ')}
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

        {#if resourceNotice}
          <p class="text-xs text-amber-600 mt-3">
            <i class="fa-solid fa-triangle-exclamation mr-1"></i>{resourceNotice}
          </p>
        {/if}
      </div>
    </div>

    <!-- Pods -->
    {#if tgt?.pods?.length}
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm px-5 py-4">
        <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide mb-3">
          Pods <span class="normal-case font-normal text-gray-300 ml-1">({tgt.pods.length})</span>
        </p>
        <div class="space-y-1">
          {#each tgt.pods as pod (pod.pod_id)}
            {@const ph = pod.health}
            <div class="flex items-center gap-3 px-3 py-1.5 rounded-lg bg-gray-50 border border-gray-100 text-xs">
              <i class="fa-solid fa-circle text-xs {healthDot[ph].colour} shrink-0" title={healthDot[ph].label}></i>
              <span class="font-mono text-gray-700 truncate flex-1" title={pod.pod_id}>{podLabel(pod)}</span>
              {#if pod.node}
                <span class="text-gray-400 shrink-0">{pod.node}</span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}

  <!-- ── Playground panel ──────────────────────────────────── -->
  {:else if panel === 'test'}
    <div class="bg-white rounded-xl border border-gray-100 shadow-sm px-5 py-4">
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
              <label for="input-{i}" class="block text-xs text-gray-500 mb-1 truncate" title={label}>{label}</label>
              <input
                id="input-{i}"
                type="number"
                step="any"
                bind:value={pg.inputs[i]}
                on:keydown={(e) => e.key === 'Enter' && runPlayground()}
                class="w-full border border-gray-200 rounded-lg px-3 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
              />
            </div>
          {/each}
          <div class="flex items-end pb-0.5">
            <button
              on:click={runPlayground}
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
    </div>

  <!-- ── History panel ─────────────────────────────────────── -->
  {:else if panel === 'history'}
    <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">
      <table class="w-full">
        <thead>
          <tr class="bg-gray-50 border-b border-gray-100">
            <th class="px-5 py-2.5 text-left text-xs font-medium text-gray-400">Run</th>
            <th class="px-4 py-2.5 text-left text-xs font-medium text-gray-400">State</th>
            <th class="px-4 py-2.5 text-left text-xs font-medium text-gray-400">Deployed</th>
          </tr>
        </thead>
        <tbody>
          {#each deps as dep (dep.deployment_id)}
            <tr class="border-b border-gray-50 last:border-0">
              <td class="px-5 py-2.5 font-mono text-xs">
                {#if dep.model_name}
                  <a href="/models/{encodeURIComponent(dep.model_name)}" class="text-sage-dark hover:underline">
                    <i class="fa-solid fa-brain mr-1 text-sage opacity-60"></i>{dep.model_name} v{dep.model_version}
                  </a>
                {:else if runExpId[dep.run_id]}
                  <a href="/experiments/{runExpId[dep.run_id]}/runs/{dep.run_id}" class="text-sage-dark hover:underline">{dep.run_id.slice(0, 12)}</a>
                {:else}
                  <span class="text-gray-600">{dep.run_id.slice(0, 12)}</span>
                {/if}
              </td>
              <td class="px-4 py-2.5"><DeployStateBadge state={dep.state} /></td>
              <td class="px-4 py-2.5 text-xs text-gray-400">{fmtDateTime(dep.created_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
{/if}

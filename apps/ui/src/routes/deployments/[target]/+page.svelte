<script lang="ts">
  import { runs, targets, nodes, type Deployment, type ModelStatus, type TargetHealth, type Target, type TargetPod, type ResourceSettings, type InfraSettings, type Schema, type SchemaField } from '$lib/api';
  import { liveData, refreshLiveData } from '$lib/stores';
  import { fmtDateTime } from '$lib/utils';
  import BreadcrumbNav from '$lib/components/BreadcrumbNav.svelte';
  import DeployStateBadge from '$lib/components/DeployStateBadge.svelte';
  import ErrorCard from '$lib/components/ErrorCard.svelte';

  let { data }: { data: { target: string } } = $props();
  const t = data.target;

  let tgt         = $state<Target | null>(null);
  let deps        = $state<Deployment[]>([]);
  let modelStatus = $state<ModelStatus | null>(null);
  let runExpId:    Record<string, string> = $state({});
  let error   = $state('');
  let loading = $state(true);

  // Playground state. The active `config` varies per input mode declared by
  // the deployed model's `schema.json`. Mode is decided once in `openTest()`
  // and is not expected to change for the lifetime of the panel.

  type FloatBytesPg = {
    mode: 'float_bytes';
    nFeatures: number;
    featureNames: string[];
    inputs: number[];
  };

  type JsonField = {
    name: string;
    /** Non-null = categorical select; null = numeric input. Derived from
     * the encoding payload structure (categories list / map keys), so it
     * works for any encoder that follows the same shape - we do not
     * dispatch on the encoder's `type` tag. */
    options: string[] | null;
    current: string;
  };
  type JsonPg = {
    mode: 'json';
    fields: JsonField[];
  };

  type ImagePg = {
    mode: 'image';
    width: number | null;
    height: number | null;
    mime: string;
    file: File | null;
  };

  type FallbackPg = {
    mode: 'fallback';
    body: string;
    contentType: string;
    reason: string;
  };

  type Playground = {
    config: FloatBytesPg | JsonPg | ImagePg | FallbackPg | null;
    result: unknown;
    err: string;
    running: boolean;
  };
  let pg = $state<Playground>({ config: null, result: null, err: '', running: false });

  let panel: 'inspect' | 'test' | 'history' = $state('inspect');

  $effect(() => {
    const live = $liveData;
    error   = live.error;
    loading = !live.loaded;
    if (!live.loaded) return;

    const match = live.targets.find(x => x.target === t);
    if (match) tgt = match;

    deps = live.deployments
      .filter(d => d.target === t)
      .sort((a, b) => b.created_at - a.created_at);

    targets.model(t)
      .then(s => {
        modelStatus = s;
        if (s?.run_id && !(s.run_id in runExpId)) {
          const runId = s.run_id;
          runs.get(runId)
            .then(r => { runExpId[runId] = r.run.info.experiment_id; })
            .catch(() => {});
        }
      })
      .catch(() => { modelStatus = null; });
  });

  /** Duck-typed dispatch on the encoding's payload, not its `type` tag.
   * Any encoder that exposes a finite set of valid string values via
   * `categories` or `map` keys gets a select widget without UI changes. */
  function fieldOptions(enc: SchemaField['encoding']): string[] | null {
    if (!enc) return null;
    if (Array.isArray(enc.categories) && enc.categories.length > 0) {
      return enc.categories;
    }
    if (enc.map && typeof enc.map === 'object') {
      const keys = Object.keys(enc.map);
      if (keys.length > 0) return keys;
    }
    return null;
  }

  /** Pull `n_features` and `features` (CSV) from MLflow run params. Used as
   * a label-source supplement when the schema only tells us the count. */
  async function readRunParams(): Promise<{ nFeatures: number | null; featureNames: string[] }> {
    const status = modelStatus;
    if (!status?.run_id) return { nFeatures: null, featureNames: [] };
    try {
      const res = await runs.get(status.run_id);
      const params = res.run.data.params;
      const nfRaw = params.find(p => p.key === 'n_features')?.value;
      const namesRaw = params.find(p => p.key === 'features')?.value ?? '';
      const nf = nfRaw !== undefined ? parseInt(nfRaw, 10) : NaN;
      const featureNames = namesRaw ? namesRaw.split(',').map(s => s.trim()) : [];
      return { nFeatures: isNaN(nf) ? null : nf, featureNames };
    } catch {
      return { nFeatures: null, featureNames: [] };
    }
  }

  async function openTest() {
    panel = 'test';
    if (pg.config !== null) return;

    let schema: Schema | null = null;
    try { schema = await targets.schema(t); } catch { /* leave null - openTest falls through to run-params or float-bytes default */ }

    const fmt = schema?.input?.format;

    if (fmt === 'json' && schema?.input?.fields?.length) {
      const fields: JsonField[] = schema.input.fields.map(f => ({
        name: f.name,
        options: fieldOptions(f.encoding),
        current: '',
      }));
      pg.config = { mode: 'json', fields };
      return;
    }

    if (fmt === 'image') {
      pg.config = {
        mode: 'image',
        width: schema?.input?.width ?? null,
        height: schema?.input?.height ?? null,
        mime: schema?.input?.mime ?? 'image/jpeg',
        file: null,
      };
      return;
    }

    if (fmt === 'float_bytes' || fmt === undefined) {
      const { nFeatures: nFromRun, featureNames } = await readRunParams();
      const nFeatures = schema?.input?.n_features ?? nFromRun ?? 4;
      pg.config = {
        mode: 'float_bytes',
        nFeatures,
        featureNames,
        inputs: Array(nFeatures).fill(0),
      };
      return;
    }

    pg.config = {
      mode: 'fallback',
      body: '',
      contentType: 'application/octet-stream',
      reason: `Unknown input format "${fmt}". Send a raw body with a Content-Type the pod understands.`,
    };
  }

  async function runPlayground() {
    pg.err = '';
    pg.result = null;
    pg.running = true;
    try {
      if (!pg.config) throw new Error('Playground not initialized.');
      const cfg = pg.config;

      // Concrete union of what we actually send: packed float bytes, a JSON
      // string, or an image's bytes. All are valid `fetch` body inputs.
      let body: ArrayBuffer | string;
      let contentType: string;

      if (cfg.mode === 'float_bytes') {
        if (cfg.nFeatures === 0) throw new Error('No inputs configured.');
        const buf = new ArrayBuffer(cfg.nFeatures * 4);
        const view = new DataView(buf);
        for (let i = 0; i < cfg.nFeatures; i++) {
          view.setFloat32(i * 4, cfg.inputs[i] ?? 0, true);
        }
        body = buf;
        contentType = 'application/octet-stream';
      } else if (cfg.mode === 'json') {
        const obj: Record<string, string | number> = {};
        for (const f of cfg.fields) {
          if (f.options === null) {
            const n = parseFloat(f.current);
            if (!Number.isFinite(n)) throw new Error(`Field "${f.name}" must be a number.`);
            obj[f.name] = n;
          } else {
            if (!f.current) throw new Error(`Field "${f.name}" is required.`);
            obj[f.name] = f.current;
          }
        }
        body = JSON.stringify(obj);
        contentType = 'application/json';
      } else if (cfg.mode === 'image') {
        if (!cfg.file) throw new Error('No file selected.');
        body = await cfg.file.arrayBuffer();
        contentType = cfg.file.type || cfg.mime;
      } else {
        if (!cfg.body) throw new Error('Body is empty.');
        body = cfg.body;
        contentType = cfg.contentType;
      }

      pg.result = await targets.playground(t, body, contentType);
    } catch (e) {
      pg.err = String(e);
    } finally {
      pg.running = false;
    }
  }

  let editingResources = $state(false);
  let resourceDraft    = $state<ResourceSettings>({ sessions: null, max_concurrent: null });
  let infraDraft       = $state<InfraSettings>({ cpu_request: null, memory_request: null, memory_limit: null, replicas: null, placement: null, node_selector: null });
  let pinnedNode       = $state('');
  let availableNodes   = $state<string[]>([]);
  let resourceSaving   = $state(false);
  let resourceError    = $state('');
  let resourceNotice   = $state('');

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
        if (needsRestart) resourceNotice = 'Saved. CPU / memory / max_concurrent changes require a pod restart to take effect - k8s was not reachable.';
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

  let hs         = $derived(tgt?.health ?? 'unknown');
  let currentDep = $derived(modelStatus ? deps.find(d => d.deployment_id === modelStatus!.deployment_id) ?? deps[0] : deps[0]);
  let inf        = $derived(tgt?.infra);
  let res        = $derived(tgt?.resources);
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
        onclick={() => { panel = 'inspect'; }}
        class="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors
          {panel === 'inspect' ? 'bg-white shadow-sm text-gray-800' : 'text-gray-500 hover:text-gray-700'}"
      >
        <i class="fa-solid fa-circle-info mr-1.5"></i>Inspect
      </button>
      {#if hs !== 'unhealthy' && modelStatus?.run_id}
        <button
          onclick={openTest}
          class="px-3 py-1.5 rounded-lg text-xs font-medium transition-colors
            {panel === 'test' ? 'bg-white shadow-sm text-gray-800' : 'text-gray-500 hover:text-gray-700'}"
        >
          <i class="fa-solid fa-flask-vial mr-1.5"></i>Playground
        </button>
      {/if}
      <button
        onclick={() => { panel = 'history'; }}
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
            <dd class="font-mono text-gray-700 truncate">{currentDep?.deployment_id ?? '-'}</dd>
          </div>
          <div class="flex gap-2 text-xs">
            <dt class="text-gray-400 w-28 shrink-0">State</dt>
            <dd>{#if currentDep}<DeployStateBadge state={currentDep.state} />{:else}-{/if}</dd>
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
              onclick={startEditResources}
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
                    <option value={null}>None - scheduler decides</option>
                    <option value="spread">Spread - anti-affinity (different nodes)</option>
                    <option value="pack">Pack - affinity (same node)</option>
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
                <p class="text-xs text-gray-400 mt-0.5">Sets <code>kubernetes.io/hostname</code> - pin pods to a specific node (e.g. one with a GPU).</p>
              </div>
            </div>

            {#if resourceError}
              <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{resourceError}</p>
            {/if}

            <div class="flex items-center gap-2 pt-1">
              <button
                onclick={saveResources}
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
                onclick={() => { editingResources = false; }}
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
              <dd class="font-mono text-gray-700">{res?.sessions ?? '-'}</dd>
            </div>
            <div class="flex gap-2 text-xs">
              <dt class="text-gray-400 w-28 shrink-0">Max concurrent</dt>
              <dd class="font-mono text-gray-700">{res?.max_concurrent ?? '-'}</dd>
            </div>
            {#if inf}
              <div class="flex gap-2 text-xs">
                <dt class="text-gray-400 w-28 shrink-0">CPU request</dt>
                <dd class="font-mono text-gray-700">{inf.cpu_request ?? '-'}</dd>
              </div>
              <div class="flex gap-2 text-xs">
                <dt class="text-gray-400 w-28 shrink-0">Memory request</dt>
                <dd class="font-mono text-gray-700">{inf.memory_request ?? '-'}</dd>
              </div>
              <div class="flex gap-2 text-xs">
                <dt class="text-gray-400 w-28 shrink-0">Memory limit</dt>
                <dd class="font-mono text-gray-700">{inf.memory_limit ?? '-'}</dd>
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
        <i class="fa-solid fa-triangle-exclamation text-peach mr-1"></i>Playground - not for production use
      </p>

      {#if pg.config === null}
        <div class="flex items-center gap-2 text-xs text-gray-400">
          <i class="fa-solid fa-spinner fa-spin"></i>Loading model inputs…
        </div>
      {:else if pg.config.mode === 'float_bytes'}
        <div class="flex items-end gap-2 flex-wrap">
          {#each pg.config.inputs as _input, i (i)}
            {@const label = pg.config.featureNames[i] ?? `Input ${i + 1}`}
            <div class="flex-1 min-w-20">
              <label for="input-{i}" class="block text-xs text-gray-500 mb-1 truncate" title={label}>{label}</label>
              <input
                id="input-{i}"
                type="number"
                step="any"
                bind:value={pg.config.inputs[i]}
                onkeydown={(e) => e.key === 'Enter' && runPlayground()}
                class="w-full border border-gray-200 rounded-lg px-3 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
              />
            </div>
          {/each}
          <div class="flex items-end pb-0.5">
            <button
              onclick={runPlayground}
              disabled={pg.running}
              class="flex items-center gap-2 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
            >
              {#if pg.running}<i class="fa-solid fa-spinner fa-spin text-xs"></i>{:else}<i class="fa-solid fa-play text-xs"></i>{/if}Run
            </button>
          </div>
        </div>
      {:else if pg.config.mode === 'json'}
        <div class="flex items-end gap-2 flex-wrap">
          {#each pg.config.fields as field, i (field.name)}
            <div class="flex-1 min-w-32">
              <label for="field-{i}" class="block text-xs text-gray-500 mb-1 truncate" title={field.name}>{field.name}</label>
              {#if field.options !== null}
                <select
                  id="field-{i}"
                  bind:value={field.current}
                  class="w-full border border-gray-200 rounded-lg px-3 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
                >
                  <option value="" disabled>—</option>
                  {#each field.options as opt (opt)}
                    <option value={opt}>{opt}</option>
                  {/each}
                </select>
              {:else}
                <input
                  id="field-{i}"
                  type="number"
                  step="any"
                  bind:value={field.current}
                  onkeydown={(e) => e.key === 'Enter' && runPlayground()}
                  class="w-full border border-gray-200 rounded-lg px-3 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
                />
              {/if}
            </div>
          {/each}
          <div class="flex items-end pb-0.5">
            <button
              onclick={runPlayground}
              disabled={pg.running}
              class="flex items-center gap-2 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
            >
              {#if pg.running}<i class="fa-solid fa-spinner fa-spin text-xs"></i>{:else}<i class="fa-solid fa-play text-xs"></i>{/if}Run
            </button>
          </div>
        </div>
      {:else if pg.config.mode === 'image'}
        <div class="space-y-2">
          {#if pg.config.width && pg.config.height}
            <p class="text-xs text-gray-500">
              Model expects images decoded to {pg.config.width}×{pg.config.height}. Any reasonable JPEG/PNG works - the
              pod's pre-transform handles the resize.
            </p>
          {/if}
          <div class="flex items-end gap-3 flex-wrap">
            <div class="flex-1 min-w-64">
              <label for="image-file" class="block text-xs text-gray-500 mb-1">Image</label>
              <input
                id="image-file"
                type="file"
                accept="image/*"
                onchange={(e) => {
                  const f = (e.currentTarget as HTMLInputElement).files?.[0] ?? null;
                  if (pg.config?.mode === 'image') pg.config.file = f;
                }}
                class="block w-full text-xs text-gray-700 file:mr-3 file:px-3 file:py-1.5 file:rounded-lg file:border-0 file:text-xs file:font-semibold file:bg-peach file:text-white hover:file:bg-peach-dark"
              />
              {#if pg.config.file}
                <p class="text-[11px] text-gray-400 mt-1 truncate">
                  {pg.config.file.name} · {(pg.config.file.size / 1024).toFixed(1)} KB · {pg.config.file.type || pg.config.mime}
                </p>
              {/if}
            </div>
            <div class="flex items-end pb-0.5">
              <button
                onclick={runPlayground}
                disabled={pg.running || !pg.config.file}
                class="flex items-center gap-2 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
              >
                {#if pg.running}<i class="fa-solid fa-spinner fa-spin text-xs"></i>{:else}<i class="fa-solid fa-play text-xs"></i>{/if}Run
              </button>
            </div>
          </div>
        </div>
      {:else}
        <div class="space-y-2">
          <p class="text-xs text-amber-600"><i class="fa-solid fa-circle-info mr-1"></i>{pg.config.reason}</p>
          <div class="flex flex-col gap-2">
            <label for="raw-content-type" class="block text-xs text-gray-500">Content-Type</label>
            <input
              id="raw-content-type"
              type="text"
              bind:value={pg.config.contentType}
              class="border border-gray-200 rounded-lg px-3 py-1.5 text-sm font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
            />
            <label for="raw-body" class="block text-xs text-gray-500">Body</label>
            <textarea
              id="raw-body"
              rows="6"
              bind:value={pg.config.body}
              class="w-full border border-gray-200 rounded-lg px-3 py-2 text-xs font-mono focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach bg-white"
            ></textarea>
            <div>
              <button
                onclick={runPlayground}
                disabled={pg.running}
                class="flex items-center gap-2 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
              >
                {#if pg.running}<i class="fa-solid fa-spinner fa-spin text-xs"></i>{:else}<i class="fa-solid fa-play text-xs"></i>{/if}Run
              </button>
            </div>
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

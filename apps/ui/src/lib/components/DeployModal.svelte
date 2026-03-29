<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { deployments, nodes, type RegisteredModel, type ModelVersion, type Deployment, type ResourceSettings } from '$lib/api';
  import DeployStateBadge from './DeployStateBadge.svelte';

  // Provide either a specific version (skips version picker) or a whole model.
  export let modelVersion: ModelVersion | null = null;
  export let registeredModel: RegisteredModel | null = null;
  export let knownTargets: { name: string; state: string; model_name: string | null; model_version: string | null; node: string | null }[];

  // Resolved once the user picks (or was given) a version.
  let resolvedVersion: ModelVersion | null = modelVersion;

  function stageBadge(stage: string) {
    return ({
      Production: { colour: 'bg-sage-light/40 text-sage-dark',  label: 'Production' },
      Staging:    { colour: 'bg-blue-50 text-blue-600',          label: 'Staging'    },
      Archived:   { colour: 'bg-gray-100 text-gray-400',         label: 'Archived'   },
      None:       { colour: 'bg-gray-50 text-gray-400',          label: 'None'       },
    } as Record<string, { colour: string; label: string }>)[stage]
      ?? { colour: 'bg-gray-50 text-gray-500', label: stage };
  }

  const dispatch = createEventDispatcher<{
    close: void;
    deployed: { run_id: string; targets: string[] };
  }>();

  const DEFAULT_RESOURCES: ResourceSettings = {
    cpu_request:    '100m',
    memory_request: '256Mi',
    memory_limit:   '512Mi',
    max_concurrent: 8,
  };

  type ActiveDep = { target: string; dep: Deployment };

  let addingNew = false;
  let newTarget = '';
  let selectedNodes: string[] = [];
  let showAdvanced = false;
  let resources: ResourceSettings = { ...DEFAULT_RESOURCES };
  let err = '';
  let activeDeps: ActiveDep[] = [];
  let polling = false;

  let nodeList: string[] = [];
  let loadingNodes = false;

  let activeIntervals: ReturnType<typeof setInterval>[] = [];

  function nodeSuffix(node: string): string {
    return node.split('-').slice(-2).join('-');
  }

  function toggleNode(node: string) {
    const idx = selectedNodes.indexOf(node);
    if (idx === -1) selectedNodes = [...selectedNodes, node];
    else selectedNodes = selectedNodes.filter(n => n !== node);
  }

  async function openNewTarget() {
    addingNew = true;
    if (nodeList.length === 0 && !loadingNodes) {
      loadingNodes = true;
      try {
        const res = await nodes.list();
        nodeList = res.nodes;
        if (nodeList.length === 1) selectedNodes = [nodeList[0]];
      } catch { /* k8s may not be reachable */ }
      loadingNodes = false;
    } else if (nodeList.length === 1) {
      selectedNodes = [nodeList[0]];
    }
  }

  function pollOne(dep_id: string, target: string) {
    const iv = setInterval(async () => {
      try {
        const res = await deployments.getById(dep_id);
        const idx = activeDeps.findIndex(d => d.target === target);
        if (idx !== -1) {
          activeDeps[idx].dep = res.deployment;
          activeDeps = activeDeps;
        }
        if (['deployed', 'failed', 'superseded'].includes(res.deployment.state)) {
          clearInterval(iv);
          activeIntervals = activeIntervals.filter(i => i !== iv);
          const allSettled = activeDeps.every(d =>
            ['deployed', 'failed', 'superseded'].includes(d.dep.state)
          );
          if (allSettled) {
            polling = false;
            dispatch('deployed', {
              run_id: resolvedVersion?.run_id ?? '',
              targets: activeDeps.filter(d => d.dep.state === 'deployed').map(d => d.target),
            });
          }
        }
      } catch {
        clearInterval(iv);
        activeIntervals = activeIntervals.filter(i => i !== iv);
      }
    }, 2000);
    activeIntervals.push(iv);
  }

  async function deployToExisting(target: string) {
    if (!resolvedVersion) return;
    err = '';
    polling = true;
    activeDeps = [];
    try {
      const res = await deployments.create(resolvedVersion.name, resolvedVersion.version, target, null);
      activeDeps = [{ target, dep: res.deployment }];
      pollOne(res.deployment.deployment_id, target);
    } catch (e) {
      err = String(e);
      polling = false;
    }
  }

  async function deployNew() {
    if (!resolvedVersion) return;
    if (!newTarget.trim()) { err = 'Target name is required.'; return; }
    if (selectedNodes.length === 0) { err = 'Select at least one node.'; return; }

    err = '';
    polling = true;
    activeDeps = [];

    const base = newTarget.trim();
    const pairs = selectedNodes.map(node => ({
      node,
      target: selectedNodes.length === 1 ? base : `${base}-${nodeSuffix(node)}`,
    }));

    const results = await Promise.allSettled(
      pairs.map(({ node, target }) =>
        deployments.create(resolvedVersion!.name, resolvedVersion!.version, target, node, resources)
          .then(res => ({ target, dep: res.deployment }))
      )
    );

    const succeeded = results
      .filter((r): r is PromiseFulfilledResult<ActiveDep> => r.status === 'fulfilled')
      .map(r => r.value);
    const failCount = results.filter(r => r.status === 'rejected').length;

    activeDeps = succeeded;
    if (failCount > 0) err = `${failCount} deployment${failCount > 1 ? 's' : ''} failed to start.`;
    if (succeeded.length === 0) polling = false;

    succeeded.forEach(({ target, dep }) => pollOne(dep.deployment_id, target));
  }

  function close() {
    activeIntervals.forEach(clearInterval);
    dispatch('close');
  }

  onDestroy(() => { activeIntervals.forEach(clearInterval); });
</script>

<!-- Backdrop -->
<div
  class="fixed inset-0 z-40 bg-black/30"
  on:click={close}
  on:keydown={(e) => e.key === 'Escape' && close()}
  role="button"
  tabindex="-1"
  aria-label="Close"
/>

<!-- Panel -->
<div
  class="fixed z-50 top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2
         w-full max-w-md bg-white rounded-2xl shadow-xl border border-gray-100 overflow-hidden"
  role="dialog"
  aria-modal="true"
>
  <!-- Header -->
  <div class="px-5 py-4 border-b border-gray-100 flex items-center justify-between gap-3">
    <div>
      {#if resolvedVersion}
        <p class="font-semibold text-gray-800 text-sm">{resolvedVersion.name} <span class="text-gray-400 font-normal">v{resolvedVersion.version}</span></p>
        {#if resolvedVersion.run_id}
          <p class="text-xs text-gray-400 font-mono mt-0.5">{resolvedVersion.run_id.slice(0, 12)}</p>
        {/if}
      {:else}
        <p class="font-semibold text-gray-800 text-sm">{registeredModel?.name}</p>
        <p class="text-xs text-gray-400 mt-0.5">Select a version to deploy</p>
      {/if}
    </div>
    <button on:click={close} class="p-1.5 rounded-lg text-gray-400 hover:text-gray-600 hover:bg-gray-100 transition-colors">
      <i class="fa-solid fa-xmark"></i>
    </button>
  </div>

  <!-- Body -->
  <div class="px-5 py-4 space-y-3">

    {#if !resolvedVersion}
      <!-- ── Step 1: version picker ─────────────────────────────────── -->
      <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide">Choose version</p>
      <div class="space-y-1.5">
        {#each (registeredModel?.latest_versions ?? []) as mv}
          {@const badge = stageBadge(mv.current_stage)}
          <button
            on:click={() => { resolvedVersion = mv; }}
            class="w-full flex items-center justify-between px-3 py-2.5 rounded-lg border border-gray-200 hover:border-peach hover:bg-peach-light/5 transition-colors text-left"
          >
            <div class="flex items-center gap-2.5">
              <span class="font-mono text-sm font-semibold text-gray-800">v{mv.version}</span>
              {#if mv.run_id}
                <span class="text-xs text-gray-400 font-mono">{mv.run_id.slice(0, 8)}</span>
              {/if}
            </div>
            <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {badge.colour}">
              {badge.label}
            </span>
          </button>
        {/each}
      </div>

    {:else if activeDeps.length > 0}
      <!-- ── Deployment results ──────────────────────────────────────── -->
      <div class="space-y-2">
        {#each activeDeps as { target, dep }}
          <div class="flex items-center gap-2 text-sm">
            <DeployStateBadge state={dep.state} />
            <span class="text-gray-400 text-xs">→ {target}</span>
          </div>
        {/each}
      </div>
      <div class="flex items-center gap-3 pt-1">
        {#if polling}
          <span class="text-xs text-gray-400 italic">
            <i class="fa-solid fa-spinner fa-spin mr-1"></i>polling…
          </span>
        {/if}
        <button on:click={close} class="text-xs text-gray-400 hover:text-gray-600 transition-colors">
          <i class="fa-solid fa-xmark mr-1"></i>{polling ? 'Cancel' : 'Close'}
        </button>
      </div>

    {:else}
      <!-- ── Step 2: target picker ───────────────────────────────────── -->
      <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide">Deploy to target</p>

      <div class="flex flex-wrap gap-2">
        {#each knownTargets as t}
          <button
            on:click={() => deployToExisting(t.name)}
            class="flex items-center gap-2 px-3 py-1.5 rounded-lg border text-sm font-medium transition-colors
              hover:border-peach hover:text-peach-dark border-gray-200 text-gray-700"
          >
            <i class="fa-solid fa-server text-xs text-gray-400"></i>
            {t.name}
            {#if t.model_name === resolvedVersion.name && t.model_version === resolvedVersion.version}
              <DeployStateBadge state={t.state} />
            {/if}
          </button>
        {/each}

        {#if !addingNew}
          <button
            on:click={openNewTarget}
            class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-dashed border-gray-300 text-sm text-gray-400 hover:border-peach hover:text-peach-dark transition-colors"
          >
            <i class="fa-solid fa-plus text-xs"></i>New target
          </button>
        {/if}
      </div>

      {#if addingNew}
        <div class="space-y-3 pt-1">

          <div class="flex gap-2">
            <input
              type="text"
              placeholder="Target name"
              bind:value={newTarget}
              class="flex-1 border border-gray-200 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach"
            />
            <button
              on:click={() => { addingNew = false; }}
              class="px-2 py-1.5 rounded-lg text-gray-400 hover:bg-gray-100 transition-colors"
            >
              <i class="fa-solid fa-xmark text-sm"></i>
            </button>
          </div>

          <div>
            <p class="text-xs text-gray-500 mb-2"><i class="fa-solid fa-server mr-1"></i>Nodes</p>
            {#if loadingNodes}
              <p class="text-xs text-gray-400 italic"><i class="fa-solid fa-spinner fa-spin mr-1"></i>Discovering nodes…</p>
            {:else if nodeList.length === 0}
              <p class="text-xs text-red-400"><i class="fa-solid fa-circle-xmark mr-1"></i>No nodes discovered — is the cluster reachable?</p>
            {:else if nodeList.length === 1}
              <p class="text-xs text-gray-500 font-mono">{nodeList[0]}</p>
            {:else}
              <div class="flex flex-wrap gap-2">
                {#each nodeList as n}
                  {@const selected = selectedNodes.includes(n)}
                  <button
                    on:click={() => toggleNode(n)}
                    class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-xs font-medium transition-colors
                      {selected ? 'border-sage bg-sage-light/30 text-sage-dark' : 'border-gray-200 text-gray-500 hover:border-gray-300'}"
                  >
                    <i class="fa-solid fa-{selected ? 'square-check' : 'square'} text-xs"></i>
                    {nodeSuffix(n)}
                  </button>
                {/each}
              </div>
            {/if}

            {#if selectedNodes.length > 1 && newTarget.trim()}
              <p class="text-xs text-gray-400 mt-2">
                Creates: {selectedNodes.map(n => `${newTarget.trim()}-${nodeSuffix(n)}`).join(', ')}
              </p>
            {/if}
          </div>

          <button
            on:click={() => { showAdvanced = !showAdvanced; }}
            class="flex items-center gap-1 text-xs text-gray-400 hover:text-gray-600 transition-colors"
          >
            <i class="fa-solid fa-chevron-{showAdvanced ? 'up' : 'down'} text-xs"></i>
            Resource settings
          </button>

          {#if showAdvanced}
            <div class="grid grid-cols-2 gap-2 bg-gray-50 rounded-lg p-3">
              <div>
                <label class="block text-xs text-gray-500 mb-1">CPU request</label>
                <input type="text" bind:value={resources.cpu_request} placeholder="100m"
                  class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 bg-white" />
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Memory request</label>
                <input type="text" bind:value={resources.memory_request} placeholder="256Mi"
                  class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 bg-white" />
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Memory limit</label>
                <input type="text" bind:value={resources.memory_limit} placeholder="512Mi"
                  class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 bg-white" />
              </div>
              <div>
                <label class="block text-xs text-gray-500 mb-1">Max concurrent</label>
                <input type="number" min="1" bind:value={resources.max_concurrent} placeholder="8"
                  class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 bg-white" />
              </div>
            </div>
          {/if}

          <div class="flex items-center justify-between pt-1">
            {#if err}
              <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{err}</p>
            {:else}
              <span></span>
            {/if}
            <button
              on:click={deployNew}
              disabled={loadingNodes || nodeList.length === 0 || selectedNodes.length === 0}
              class="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
            >
              <i class="fa-solid fa-rocket text-xs"></i>
              Deploy{selectedNodes.length > 1 ? ` to ${selectedNodes.length} nodes` : ''}
            </button>
          </div>

        </div>
      {/if}

      {#if !addingNew && err}
        <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{err}</p>
      {/if}

    {/if}

  </div>
</div>

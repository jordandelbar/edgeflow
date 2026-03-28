<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { models, deployments, targets, nodes, modelName, type Run, type Deployment, type ResourceSettings } from '$lib/api';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import DeployStateBadge from '$lib/components/DeployStateBadge.svelte';

  let items: Run[] = [];
  let knownTargets: { name: string; state: string; run_id: string; node: string | null }[] = [];
  let demoting: Record<string, boolean> = {};
  let deployedOn: Record<string, string[]> = {};   // run_id → target names currently deployed
  let nodeList: string[] = [];
  let loadingNodes = false;
  let error = '';

  const DEFAULT_RESOURCES: ResourceSettings = {
    cpu_request:    '100m',
    memory_request: '256Mi',
    memory_limit:   '512Mi',
    max_concurrent: 8,
  };

  type ActiveDep = { target: string; dep: Deployment };

  // Per-card deploy state
  type CardState = {
    open: boolean;
    addingNew: boolean;
    newTarget: string;
    selectedNodes: string[];
    showAdvanced: boolean;
    resources: ResourceSettings;
    err: string;
    activeDeps: ActiveDep[];
    polling: boolean;
  };
  let cards: Record<string, CardState> = {};
  let activeIntervals: ReturnType<typeof setInterval>[] = [];

  function emptyCard(): CardState {
    return { open: false, addingNew: false, newTarget: '', selectedNodes: [], showAdvanced: false, resources: { ...DEFAULT_RESOURCES }, err: '', activeDeps: [], polling: false };
  }

  onMount(async () => {
    try {
      const [modelsRes, depsRes, tgRes] = await Promise.all([
        models.list(),
        deployments.list(),
        targets.list(),
      ]);
      items = modelsRes.runs ?? [];
      items.forEach(r => { cards[r.info.run_id] = emptyCard(); });

      // Build node info from target records
      const targetNodeMap: Record<string, string | null> = {};
      for (const t of (tgRes.targets ?? [])) {
        targetNodeMap[t.target] = t.node;
      }

      // Extract latest state per target
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

      // Map run_id → targets where it is the active deployed deployment
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

  function toggle(run_id: string) {
    const wasOpen = cards[run_id].open || cards[run_id].activeDeps.length > 0;
    cards[run_id] = emptyCard();
    cards[run_id].open = !wasOpen;
    cards = cards;
  }

  function toggleNode(run_id: string, node: string) {
    const c = cards[run_id];
    const idx = c.selectedNodes.indexOf(node);
    if (idx === -1) c.selectedNodes = [...c.selectedNodes, node];
    else c.selectedNodes = c.selectedNodes.filter(n => n !== node);
    cards = cards;
  }

  // Short suffix from a full node name, e.g. "k3d-edgeflow-agent-0" → "agent-0"
  function nodeSuffix(node: string): string {
    const parts = node.split('-');
    return parts.slice(-2).join('-');
  }

  // Derive target name(s) from base name + selected nodes
  function targetNames(base: string, selected: string[]): string[] {
    if (selected.length <= 1) return [base];
    return selected.map(n => `${base}-${nodeSuffix(n)}`);
  }

  async function openNewTarget(run_id: string) {
    cards[run_id].addingNew = true;
    cards = cards;
    if (nodeList.length === 0 && !loadingNodes) {
      loadingNodes = true;
      try {
        const res = await nodes.list();
        nodeList = res.nodes;
        // Auto-select when there's exactly one node
        if (nodeList.length === 1) {
          cards[run_id].selectedNodes = [nodeList[0]];
          cards = cards;
        }
      } catch { /* k8s may not be reachable */ }
      loadingNodes = false;
    } else if (nodeList.length === 1) {
      cards[run_id].selectedNodes = [nodeList[0]];
      cards = cards;
    }
  }

  async function deployToExisting(run: Run, target: string) {
    const c = cards[run.info.run_id];
    c.err = '';
    c.polling = true;
    cards = cards;
    try {
      const res = await deployments.create(run.info.run_id, target, null);
      c.activeDeps = [{ target, dep: res.deployment }];
      cards = cards;
      pollOne(run.info.run_id, res.deployment.deployment_id, target);
    } catch (e) {
      c.err = String(e);
      c.polling = false;
      cards = cards;
    }
  }

  async function deployNew(run: Run) {
    const c = cards[run.info.run_id];
    if (!c.newTarget.trim()) { c.err = 'Target name is required.'; cards = cards; return; }
    if (c.selectedNodes.length === 0) { c.err = 'Select at least one node.'; cards = cards; return; }

    c.err = '';
    c.polling = true;
    c.activeDeps = [];
    cards = cards;

    const base = c.newTarget.trim();
    const pairs = c.selectedNodes.map((node, i) => ({
      node,
      target: c.selectedNodes.length === 1 ? base : `${base}-${nodeSuffix(node)}`,
    }));

    const results = await Promise.allSettled(
      pairs.map(({ node, target }) =>
        deployments.create(run.info.run_id, target, node, c.resources)
          .then(res => ({ target, dep: res.deployment }))
      )
    );

    const succeeded = results
      .filter((r): r is PromiseFulfilledResult<ActiveDep> => r.status === 'fulfilled')
      .map(r => r.value);
    const failCount = results.filter(r => r.status === 'rejected').length;

    c.activeDeps = succeeded;
    if (failCount > 0) c.err = `${failCount} deployment${failCount > 1 ? 's' : ''} failed to start.`;
    if (succeeded.length === 0) c.polling = false;
    cards = cards;

    succeeded.forEach(({ target, dep }) => pollOne(run.info.run_id, dep.deployment_id, target));
  }

  function pollOne(run_id: string, dep_id: string, target: string) {
    const iv = setInterval(async () => {
      try {
        const res = await deployments.getById(dep_id);
        const idx = cards[run_id].activeDeps.findIndex(d => d.target === target);
        if (idx !== -1) {
          cards[run_id].activeDeps[idx].dep = res.deployment;
          cards = cards;
        }
        if (['deployed', 'failed', 'superseded'].includes(res.deployment.state)) {
          clearInterval(iv);
          activeIntervals = activeIntervals.filter(i => i !== iv);
          const allSettled = cards[run_id].activeDeps.every(d =>
            ['deployed', 'failed', 'superseded'].includes(d.dep.state)
          );
          if (allSettled) { cards[run_id].polling = false; cards = cards; }
        }
      } catch {
        clearInterval(iv);
        activeIntervals = activeIntervals.filter(i => i !== iv);
      }
    }, 2000);
    activeIntervals.push(iv);
  }

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

  onDestroy(() => { activeIntervals.forEach(clearInterval); });
</script>

{#if error}
  <ErrorCard message={error} />
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-brain text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No models yet.</p>
    <p class="text-xs mt-1">Promote a finished run from the Experiments section.</p>
  </div>
{:else}
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
    {#each items as run}
      {@const c = cards[run.info.run_id] ?? emptyCard()}
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">

        <!-- Card header -->
        <div class="px-5 py-4 flex items-start justify-between gap-3">
          <div class="flex items-start gap-3">
            <div class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" style="background:#edf4f1">
              <i class="fa-solid fa-brain text-sage text-sm"></i>
            </div>
            <div>
              <a href="/experiments/{run.info.experiment_id}/runs/{run.info.run_id}" class="font-semibold text-gray-800 hover:text-sage-dark transition-colors">
                {modelName(run)}
              </a>
              <p class="text-xs text-gray-400 font-mono mt-0.5">{run.info.run_id.slice(0, 12)}</p>
            </div>
          </div>
          <div class="flex items-center gap-2 shrink-0">
            <div class="flex flex-wrap gap-1.5 justify-end">
              {#each (deployedOn[run.info.run_id] ?? []) as target}
                <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-sage-light/40 text-sage-dark">
                  <i class="fa-solid fa-circle text-xs"></i>{target}
                </span>
              {/each}
            </div>
            <button
              on:click={() => demote(run.info.run_id)}
              disabled={demoting[run.info.run_id]}
              title="Demote — remove from Models"
              class="p-1 rounded text-gray-300 hover:text-red-400 hover:bg-red-50 transition-colors disabled:opacity-50"
            >
              {#if demoting[run.info.run_id]}
                <i class="fa-solid fa-spinner fa-spin text-xs"></i>
              {:else}
                <i class="fa-solid fa-circle-minus text-xs"></i>
              {/if}
            </button>
          </div>
        </div>

        <!-- Meta row -->
        <div class="px-5 pb-3 flex items-center gap-4 text-xs text-gray-400">
          <a href="/experiments/{run.info.experiment_id}" class="hover:text-gray-600 transition-colors">
            <i class="fa-solid fa-flask mr-1"></i>exp {run.info.experiment_id}
          </a>
          <span><i class="fa-solid fa-calendar mr-1"></i>{new Date(run.info.start_time).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' })}</span>
          {#if run.data.metrics.length > 0}
            <span class="ml-auto font-mono text-gray-500">
              {run.data.metrics[0].key}: <strong>{run.data.metrics[0].value}</strong>
            </span>
          {/if}
        </div>

        <!-- Deploy area -->
        <div class="border-t border-gray-100">

          {#if c.activeDeps.length > 0}
            <!-- Active deployment results -->
            <div class="px-5 py-3 space-y-2">
              {#each c.activeDeps as { target, dep }}
                <div class="flex items-center gap-2 text-sm">
                  <DeployStateBadge state={dep.state} />
                  <span class="text-gray-400 text-xs">→ {target}</span>
                </div>
              {/each}
              <div class="pt-1">
                {#if c.polling}
                  <span class="text-xs text-gray-400 italic">
                    <i class="fa-solid fa-spinner fa-spin mr-1"></i>polling…
                  </span>
                {:else}
                  <button on:click={() => toggle(run.info.run_id)} class="text-xs text-gray-400 hover:text-gray-600">
                    <i class="fa-solid fa-xmark mr-1"></i>Close
                  </button>
                {/if}
              </div>
            </div>

          {:else if !c.open}
            <!-- Collapsed: show Deploy button -->
            <button
              on:click={() => toggle(run.info.run_id)}
              class="w-full flex items-center justify-center gap-2 px-5 py-3 text-sm font-semibold text-peach-dark hover:bg-peach-light/10 transition-colors"
            >
              <i class="fa-solid fa-rocket text-xs"></i>Deploy
            </button>

          {:else}
            <!-- Expanded: target picker -->
            <div class="px-5 py-4 space-y-3">
              <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide">Deploy to target</p>

              <div class="flex flex-wrap gap-2">
                {#each knownTargets as t}
                  <button
                    on:click={() => deployToExisting(run, t.name)}
                    class="flex items-center gap-2 px-3 py-1.5 rounded-lg border text-sm font-medium transition-colors
                      hover:border-peach hover:text-peach-dark border-gray-200 text-gray-700"
                  >
                    <i class="fa-solid fa-server text-xs text-gray-400"></i>
                    {t.name}
                    {#if t.run_id === run.info.run_id}
                      <DeployStateBadge state={t.state} />
                    {/if}
                    {#if t.node}
                      <span class="text-xs text-gray-400 font-mono" title={t.node}>{nodeSuffix(t.node)}</span>
                    {/if}
                  </button>
                {/each}

                <!-- Add new target -->
                {#if !c.addingNew}
                  <button
                    on:click={() => openNewTarget(run.info.run_id)}
                    class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-dashed border-gray-300 text-sm text-gray-400 hover:border-peach hover:text-peach-dark transition-colors"
                  >
                    <i class="fa-solid fa-plus text-xs"></i>New target
                  </button>
                {/if}
              </div>

              {#if c.addingNew}
                <div class="space-y-3">

                  <!-- Target base name -->
                  <div class="flex gap-2">
                    <input
                      type="text"
                      placeholder="Target name"
                      bind:value={cards[run.info.run_id].newTarget}
                      class="flex-1 border border-gray-200 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach"
                    />
                    <button
                      on:click={() => { cards[run.info.run_id].addingNew = false; cards = cards; }}
                      class="px-2 py-1.5 rounded-lg text-gray-400 hover:bg-gray-100 transition-colors"
                    >
                      <i class="fa-solid fa-xmark text-sm"></i>
                    </button>
                  </div>

                  <!-- Node selection -->
                  <div>
                    <p class="text-xs text-gray-500 mb-2">
                      <i class="fa-solid fa-server mr-1"></i>Nodes
                    </p>
                    {#if loadingNodes}
                      <p class="text-xs text-gray-400 italic"><i class="fa-solid fa-spinner fa-spin mr-1"></i>Discovering nodes…</p>
                    {:else if nodeList.length === 0}
                      <p class="text-xs text-red-400"><i class="fa-solid fa-circle-xmark mr-1"></i>No nodes discovered — is the cluster reachable?</p>
                    {:else if nodeList.length === 1}
                      <p class="text-xs text-gray-500 font-mono">{nodeList[0]}</p>
                    {:else}
                      <div class="flex flex-wrap gap-2">
                        {#each nodeList as n}
                          {@const selected = c.selectedNodes.includes(n)}
                          <button
                            on:click={() => toggleNode(run.info.run_id, n)}
                            class="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border text-xs font-medium transition-colors
                              {selected
                                ? 'border-sage bg-sage-light/30 text-sage-dark'
                                : 'border-gray-200 text-gray-500 hover:border-gray-300'}"
                          >
                            <i class="fa-solid fa-{selected ? 'square-check' : 'square'} text-xs"></i>
                            {nodeSuffix(n)}
                          </button>
                        {/each}
                      </div>
                    {/if}

                    <!-- Target name preview when multiple nodes selected -->
                    {#if c.selectedNodes.length > 1 && c.newTarget.trim()}
                      <p class="text-xs text-gray-400 mt-2">
                        Creates:
                        {targetNames(c.newTarget.trim(), c.selectedNodes).join(', ')}
                      </p>
                    {/if}
                  </div>

                  <!-- Advanced: resource settings -->
                  <button
                    on:click={() => { cards[run.info.run_id].showAdvanced = !c.showAdvanced; cards = cards; }}
                    class="flex items-center gap-1 text-xs text-gray-400 hover:text-gray-600 transition-colors"
                  >
                    <i class="fa-solid fa-chevron-{c.showAdvanced ? 'up' : 'down'} text-xs"></i>
                    Resource settings
                  </button>

                  {#if c.showAdvanced}
                    <div class="grid grid-cols-2 gap-2 bg-gray-50 rounded-lg p-3">
                      <div>
                        <label class="block text-xs text-gray-500 mb-1">CPU request</label>
                        <input type="text" bind:value={cards[run.info.run_id].resources.cpu_request} placeholder="100m"
                          class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 focus:border-peach bg-white" />
                      </div>
                      <div>
                        <label class="block text-xs text-gray-500 mb-1">Memory request</label>
                        <input type="text" bind:value={cards[run.info.run_id].resources.memory_request} placeholder="256Mi"
                          class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 focus:border-peach bg-white" />
                      </div>
                      <div>
                        <label class="block text-xs text-gray-500 mb-1">Memory limit</label>
                        <input type="text" bind:value={cards[run.info.run_id].resources.memory_limit} placeholder="512Mi"
                          class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 focus:border-peach bg-white" />
                      </div>
                      <div>
                        <label class="block text-xs text-gray-500 mb-1">Max concurrent infer</label>
                        <input type="number" min="1" bind:value={cards[run.info.run_id].resources.max_concurrent} placeholder="8"
                          class="w-full border border-gray-200 rounded px-2 py-1 text-xs font-mono focus:outline-none focus:ring-1 focus:ring-peach/50 focus:border-peach bg-white" />
                      </div>
                    </div>
                  {/if}

                  <!-- Deploy button -->
                  <div class="flex items-center justify-between">
                    {#if c.err}
                      <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{c.err}</p>
                    {:else}
                      <span></span>
                    {/if}
                    <button
                      on:click={() => deployNew(run)}
                      disabled={loadingNodes || nodeList.length === 0 || c.selectedNodes.length === 0}
                      class="flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors disabled:opacity-50"
                    >
                      <i class="fa-solid fa-rocket text-xs"></i>
                      Deploy{c.selectedNodes.length > 1 ? ` to ${c.selectedNodes.length} nodes` : ''}
                    </button>
                  </div>

                </div>
              {/if}

              {#if !c.addingNew && c.err}
                <p class="text-xs text-red-500">{c.err}</p>
              {/if}

              <div class="flex justify-end">
                <button on:click={() => toggle(run.info.run_id)} class="text-xs text-gray-400 hover:text-gray-600">
                  Cancel
                </button>
              </div>
            </div>
          {/if}

        </div>
      </div>
    {/each}
  </div>
{/if}

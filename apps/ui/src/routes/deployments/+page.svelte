<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { deployments, targets, runs, type Deployment, type ModelStatus } from '$lib/api';
  import ErrorCard from '$lib/components/ErrorCard.svelte';
  import DeployStateBadge from '$lib/components/DeployStateBadge.svelte';

  let byTarget: Record<string, Deployment[]> = {};
  let targetList: string[] = [];
  let nodeForTarget: Record<string, string | null> = {};
  let modelStatus: Record<string, ModelStatus | null> = {};
  let podHealth: Record<string, 'up' | 'down' | 'checking'> = {};
  let expanded: Record<string, 'history' | 'test' | null> = {};
  let confirming: Record<string, boolean> = {};
  let tearing: Record<string, boolean> = {};
  let error = '';
  let loading = true;
  let interval: ReturnType<typeof setInterval>;

  type NodeGroup = { node: string | null; targets: string[] };
  $: nodeGroups = (() => {
    const groups = new Map<string | null, string[]>();
    for (const t of targetList) {
      const node = nodeForTarget[t] ?? null;
      if (!groups.has(node)) groups.set(node, []);
      groups.get(node)!.push(t);
    }
    return [...groups.entries()]
      .map(([node, tgts]) => ({ node, targets: tgts }) as NodeGroup)
      .sort((a, b) => {
        if (a.node === null) return 1;
        if (b.node === null) return -1;
        return a.node.localeCompare(b.node);
      });
  })();

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

  async function load() {
    try {
      const [depRes, tgRes] = await Promise.all([
        deployments.list(),
        targets.list(),
      ]);
      const all = depRes.deployments ?? [];

      const newByTarget: Record<string, Deployment[]> = {};
      for (const d of all) {
        if (!newByTarget[d.target]) newByTarget[d.target] = [];
        newByTarget[d.target].push(d);
      }
      const newTargetList = Object.keys(newByTarget);

      // Update node info from target records (preserved across refreshes)
      for (const tg of (tgRes.targets ?? [])) {
        nodeForTarget[tg.target] = tg.node;
      }
      nodeForTarget = nodeForTarget;

      // Init state only for targets we haven't seen before — preserves
      // expanded panels and playground inputs across refreshes.
      for (const t of newTargetList) {
        if (!(t in expanded))    expanded[t]    = null;
        if (!(t in modelStatus)) modelStatus[t] = null;
        if (!(t in podHealth))   podHealth[t]   = 'checking';
        if (!(t in confirming))  confirming[t]  = false;
        if (!(t in tearing))     tearing[t]     = false;
        if (!(t in playground))  playground[t]  = { inputs: [], nFeatures: null, featureNames: [], result: null, err: '', running: false };
      }

      byTarget   = newByTarget;
      targetList = newTargetList;

      // Re-probe all targets without resetting to 'checking' — avoids flicker.
      newTargetList.forEach(t => {
        targets.model(t)
          .then(s => { modelStatus[t] = s; modelStatus = modelStatus; })
          .catch(() => { modelStatus[t] = null; modelStatus = modelStatus; });
        targets.health(t)
          .then(() => { podHealth[t] = 'up';   podHealth = podHealth; })
          .catch(() => { podHealth[t] = 'down'; podHealth = podHealth; });
      });
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load();
    interval = setInterval(load, 5000);
  });

  onDestroy(() => clearInterval(interval));

  async function toggle(t: string, panel: 'history' | 'test') {
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

  function fmt(ms: number) {
    return new Date(ms).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  }

  function fmtLoaded(iso: string) {
    return new Date(iso).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
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
  <div class="space-y-6">
    {#each nodeGroups as group}
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">

        <!-- Node group header -->
        <div class="px-4 py-2.5 bg-gray-50 border-b border-gray-100 flex items-center gap-2">
          <i class="fa-solid fa-server text-xs text-sage"></i>
          <span class="text-xs font-semibold text-gray-600 uppercase tracking-wide">
            {group.node ?? 'No node assigned'}
          </span>
          <span class="ml-auto text-xs text-gray-400">
            {group.targets.length} {group.targets.length === 1 ? 'target' : 'targets'}
          </span>
        </div>

        <!-- Target rows -->
        <table class="w-full">
          <thead>
            <tr class="border-b border-gray-50">
              <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 w-6"></th>
              <th class="px-4 py-2 text-left text-xs font-medium text-gray-400">Target</th>
              <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 hidden sm:table-cell">Model</th>
              <th class="px-4 py-2 text-left text-xs font-medium text-gray-400 hidden md:table-cell">Since</th>
              <th class="px-4 py-2 text-right text-xs font-medium text-gray-400">Actions</th>
            </tr>
          </thead>
          <tbody>
          {#each group.targets as t}
            {@const deps = byTarget[t]}
            {@const latest = deps[0]}
            {@const status = modelStatus[t]}
            {@const ph = podHealth[t]}
            {@const pg = playground[t]}

            <!-- Target row -->
            <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50/50 transition-colors">
              <!-- Health dot -->
              <td class="pl-4 pr-2 py-3">
                {#if ph === 'checking'}
                  <i class="fa-solid fa-spinner fa-spin text-gray-300 text-xs"></i>
                {:else if ph === 'up'}
                  <i class="fa-solid fa-circle text-sage text-xs" title="healthy"></i>
                {:else}
                  <i class="fa-solid fa-circle text-red-400 text-xs" title="unreachable"></i>
                {/if}
              </td>

              <!-- Target name -->
              <td class="px-4 py-3">
                <span class="font-medium text-sm text-gray-800">{t}</span>
              </td>

              <!-- Loaded model -->
              <td class="px-4 py-3 hidden sm:table-cell">
                {#if status?.run_id}
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
                  {fmtLoaded(status.loaded_at)}
                {:else}
                  {fmt(latest.created_at)}
                {/if}
              </td>

              <!-- Actions -->
              <td class="px-4 py-3">
                <div class="flex items-center justify-end gap-1">

                  <!-- Test -->
                  {#if ph === 'up'}
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
                      {#each pg.inputs as _, i}
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
                      {#each deps as dep}
                        <tr class="border-b border-gray-50 last:border-0">
                          <td class="px-8 py-2 font-mono text-xs text-gray-600">{dep.run_id.slice(0, 12)}</td>
                          <td class="px-4 py-2"><DeployStateBadge state={dep.state} /></td>
                          <td class="px-4 py-2 text-xs text-gray-400">{fmt(dep.created_at)}</td>
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
    {/each}
  </div>
{/if}

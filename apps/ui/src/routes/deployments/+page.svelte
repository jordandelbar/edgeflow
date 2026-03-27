<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { deployments, targets, runs, type Deployment, type ModelStatus } from '$lib/api';

  let byTarget: Record<string, Deployment[]> = {};
  let targetList: string[] = [];
  let modelStatus: Record<string, ModelStatus | null> = {};
  let podHealth: Record<string, 'up' | 'down' | 'checking'> = {};
  let expanded: Record<string, 'history' | 'test' | null> = {};
  let error = '';
  let loading = true;
  let interval: ReturnType<typeof setInterval>;

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
      const res = await deployments.list();
      const all = res.deployments ?? [];

      const newByTarget: Record<string, Deployment[]> = {};
      for (const d of all) {
        if (!newByTarget[d.target]) newByTarget[d.target] = [];
        newByTarget[d.target].push(d);
      }
      const newTargetList = Object.keys(newByTarget);

      // Init state only for targets we haven't seen before — preserves
      // expanded panels and playground inputs across refreshes.
      for (const t of newTargetList) {
        if (!(t in expanded))    expanded[t]    = null;
        if (!(t in modelStatus)) modelStatus[t] = null;
        if (!(t in podHealth))   podHealth[t]   = 'checking';
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

  const stateStyle: Record<string, string> = {
    pending:    'bg-gray-100 text-gray-500',
    deploying:  'bg-peach-light/60 text-peach-dark',
    upgrading:  'bg-peach-light/60 text-peach-dark',
    deployed:   'bg-sage-light/40 text-sage-dark',
    failed:     'bg-red-100 text-red-600',
    superseded: 'bg-gray-100 text-gray-400',
  };

  const stateIcon: Record<string, string> = {
    pending:    'fa-solid fa-clock',
    deploying:  'fa-solid fa-spinner fa-spin',
    upgrading:  'fa-solid fa-arrows-rotate',
    deployed:   'fa-solid fa-circle-check',
    failed:     'fa-solid fa-circle-xmark',
    superseded: 'fa-solid fa-circle-minus',
  };

  function fmt(ms: number) {
    return new Date(ms).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  }

  function fmtLoaded(iso: string) {
    return new Date(iso).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  }
</script>

{#if error}
  <div class="flex items-center gap-2 text-red-600 bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-sm">
    <i class="fa-solid fa-circle-exclamation"></i>{error}
  </div>
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
  <div class="space-y-4">
    {#each targetList as t}
      {@const deps = byTarget[t]}
      {@const latest = deps[0]}
      {@const status = modelStatus[t]}
      {@const ph = podHealth[t]}
      {@const pg = playground[t]}

      <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">

        <!-- Target header -->
        <div class="px-5 py-4 flex items-center gap-3">
          <div class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" style="background:#edf4f1">
            <i class="fa-solid fa-server text-sage text-sm"></i>
          </div>

          <div class="flex-1 min-w-0">
            <p class="font-semibold text-gray-800">{t}</p>
            {#if status?.run_id}
              <p class="text-xs text-gray-400 mt-0.5">
                <i class="fa-solid fa-microchip mr-1"></i>
                <span class="font-mono">{status.run_id.slice(0, 12)}</span>
                · loaded {fmtLoaded(status.loaded_at)}
              </p>
            {:else}
              <p class="text-xs text-gray-400 mt-0.5">{deps.length} deployment{deps.length !== 1 ? 's' : ''} · last {fmt(latest.created_at)}</p>
            {/if}
          </div>

          <!-- Pod health indicator -->
          {#if ph === 'checking'}
            <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-gray-100 text-gray-400">
              <i class="fa-solid fa-spinner fa-spin text-xs"></i>pod
            </span>
          {:else if ph === 'up'}
            <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-sage-light/40 text-sage-dark">
              <i class="fa-solid fa-circle text-xs"></i>healthy
            </span>
          {:else}
            <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium bg-red-100 text-red-600">
              <i class="fa-solid fa-circle-xmark text-xs"></i>unreachable
            </span>
          {/if}

          <!-- Test button (only when pod is up) -->
          {#if ph === 'up'}
            <button
              on:click={() => toggle(t, 'test')}
              class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-semibold transition-colors
                {expanded[t] === 'test'
                  ? 'bg-peach text-white'
                  : 'border border-peach text-peach-dark hover:bg-peach-light/20'}"
            >
              <i class="fa-solid fa-flask-vial"></i>Test
            </button>
          {/if}

          <!-- History chevron -->
          <button
            on:click={() => toggle(t, 'history')}
            class="text-gray-400 hover:text-gray-600 transition-colors"
          >
            <i class="fa-solid {expanded[t] === 'history' ? 'fa-chevron-up' : 'fa-chevron-down'} text-xs"></i>
          </button>
        </div>

        <!-- Playground panel -->
        {#if expanded[t] === 'test'}
          <div class="border-t border-gray-100 px-5 py-4 space-y-3 bg-gray-50/50">
            <p class="text-xs font-semibold text-gray-400 uppercase tracking-wide">
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
              <p class="text-xs text-red-500"><i class="fa-solid fa-circle-xmark mr-1"></i>{pg.err}</p>
            {/if}

            {#if pg.result}
              <div class="bg-white border border-gray-200 rounded-lg px-4 py-3 font-mono text-xs">
                <pre class="text-gray-800 whitespace-pre-wrap">{JSON.stringify(pg.result, null, 2)}</pre>
              </div>
            {/if}
          </div>
        {/if}

        <!-- History panel -->
        {#if expanded[t] === 'history'}
          <div class="border-t border-gray-100">
            <table class="w-full text-sm">
              <thead>
                <tr class="border-b border-gray-100">
                  <th class="px-5 py-2.5 text-xs font-semibold text-gray-400 uppercase tracking-wide text-left">Run</th>
                  <th class="px-5 py-2.5 text-xs font-semibold text-gray-400 uppercase tracking-wide text-left">State</th>
                  <th class="px-5 py-2.5 text-xs font-semibold text-gray-400 uppercase tracking-wide text-left">Deployed</th>
                </tr>
              </thead>
              <tbody>
                {#each deps as dep}
                  <tr class="border-b border-gray-50 last:border-0 hover:bg-gray-50 transition-colors">
                    <td class="px-5 py-2.5 font-mono text-xs text-gray-600">{dep.run_id.slice(0, 12)}</td>
                    <td class="px-5 py-2.5">
                      <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium {stateStyle[dep.state] ?? 'bg-gray-100 text-gray-600'}">
                        <i class="{stateIcon[dep.state] ?? 'fa-solid fa-circle'} text-xs"></i>
                        {dep.state}
                      </span>
                    </td>
                    <td class="px-5 py-2.5 text-xs text-gray-400">{fmt(dep.created_at)}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {/if}

      </div>
    {/each}
  </div>
{/if}

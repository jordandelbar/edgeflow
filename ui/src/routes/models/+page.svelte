<script lang="ts">
  import { onMount } from 'svelte';
  import { models, deployments, modelName, runTag, type Run, type Deployment } from '$lib/api';

  let items: Run[] = [];
  let knownTargets: { name: string; state: string }[] = [];
  let deployedOn: Record<string, string[]> = {};   // run_id → target names currently deployed
  let error = '';

  // Per-card deploy state
  type CardState = {
    open: boolean;
    addingNew: boolean;
    newTarget: string;
    err: string;
    dep: Deployment | null;
    polling: boolean;
  };
  let cards: Record<string, CardState> = {};

  onMount(async () => {
    try {
      const [modelsRes, depsRes] = await Promise.all([
        models.list(),
        deployments.list(),
      ]);
      items = modelsRes.runs ?? [];
      items.forEach(r => {
        cards[r.info.run_id] = { open: false, addingNew: false, newTarget: '', err: '', dep: null, polling: false };
      });

      // Extract latest state per target
      const latestByTarget: Record<string, Deployment> = {};
      for (const d of (depsRes.deployments ?? [])) {
        if (!latestByTarget[d.target]) latestByTarget[d.target] = d;
      }
      knownTargets = Object.entries(latestByTarget).map(([name, d]) => ({ name, state: d.state }));

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
    const c = cards[run_id];
    c.open = !c.open;
    c.addingNew = false;
    c.newTarget = '';
    c.err = '';
    c.dep = null;
    cards = cards;
  }

  async function deployTo(run: Run, target: string) {
    const c = cards[run.info.run_id];
    c.err = '';
    c.polling = true;
    cards = cards;
    try {
      const res = await deployments.create(run.info.run_id, target);
      c.dep = res.deployment;
      cards = cards;
      poll(run.info.run_id, res.deployment.deployment_id);
    } catch (e) {
      c.err = String(e);
      c.polling = false;
      cards = cards;
    }
  }

  async function deployNew(run: Run) {
    const c = cards[run.info.run_id];
    if (!c.newTarget.trim()) { c.err = 'Target name is required.'; cards = cards; return; }
    await deployTo(run, c.newTarget.trim());
  }

  function poll(run_id: string, dep_id: string) {
    const iv = setInterval(async () => {
      try {
        const res = await deployments.getById(dep_id);
        cards[run_id].dep = res.deployment;
        cards = cards;
        if (['deployed', 'failed', 'superseded'].includes(res.deployment.state)) {
          cards[run_id].polling = false;
          cards = cards;
          clearInterval(iv);
        }
      } catch { clearInterval(iv); }
    }, 2000);
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
    upgrading:  'fa-solid fa-spinner fa-spin',
    deployed:   'fa-solid fa-circle-check',
    failed:     'fa-solid fa-circle-xmark',
    superseded: 'fa-solid fa-circle-minus',
  };
</script>

{#if error}
  <div class="flex items-center gap-2 text-red-600 bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-sm">
    <i class="fa-solid fa-circle-exclamation"></i>{error}
  </div>
{:else if items.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-brain text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No models yet.</p>
    <p class="text-xs mt-1">Promote a finished run from the Experiments section.</p>
  </div>
{:else}
  <div class="grid grid-cols-1 lg:grid-cols-2 gap-4">
    {#each items as run}
      {@const c = cards[run.info.run_id] ?? { open: false, addingNew: false, newTarget: '', err: '', dep: null, polling: false }}
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">

        <!-- Card header -->
        <div class="px-5 py-4 flex items-start justify-between gap-3">
          <div class="flex items-start gap-3">
            <div class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" style="background:#edf4f1">
              <i class="fa-solid fa-brain text-sage text-sm"></i>
            </div>
            <div>
              <p class="font-semibold text-gray-800">{modelName(run)}</p>
              <p class="text-xs text-gray-400 font-mono mt-0.5">{run.info.run_id.slice(0, 12)}</p>
            </div>
          </div>
          <div class="flex flex-wrap gap-1.5 justify-end shrink-0">
            {#each (deployedOn[run.info.run_id] ?? []) as target}
              <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-sage-light/40 text-sage-dark">
                <i class="fa-solid fa-circle text-xs"></i>{target}
              </span>
            {/each}
          </div>
        </div>

        <!-- Meta row -->
        <div class="px-5 pb-3 flex items-center gap-4 text-xs text-gray-400">
          <span><i class="fa-solid fa-flask mr-1"></i>exp {run.info.experiment_id}</span>
          <span><i class="fa-solid fa-calendar mr-1"></i>{new Date(run.info.start_time).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' })}</span>
          {#if run.data.metrics.length > 0}
            <span class="ml-auto font-mono text-gray-500">
              {run.data.metrics[0].key}: <strong>{run.data.metrics[0].value}</strong>
            </span>
          {/if}
        </div>

        <!-- Deploy area -->
        <div class="border-t border-gray-100">

          {#if c.dep}
            <!-- Active deployment result -->
            <div class="px-5 py-3 flex items-center justify-between gap-3">
              <div class="flex items-center gap-2 text-sm">
                <i class="{stateIcon[c.dep.state] ?? 'fa-solid fa-circle'} text-xs {c.polling ? 'text-peach' : ''}"></i>
                <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {stateStyle[c.dep.state] ?? 'bg-gray-100 text-gray-600'}">
                  {c.dep.state}
                </span>
                <span class="text-gray-400 text-xs">→ {c.dep.target}</span>
              </div>
              {#if c.polling}
                <span class="text-xs text-gray-400 italic">polling…</span>
              {:else}
                <button on:click={() => toggle(run.info.run_id)} class="text-xs text-gray-400 hover:text-gray-600">
                  <i class="fa-solid fa-xmark"></i>
                </button>
              {/if}
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
                    on:click={() => deployTo(run, t.name)}
                    class="flex items-center gap-2 px-3 py-1.5 rounded-lg border text-sm font-medium transition-colors
                      hover:border-peach hover:text-peach-dark border-gray-200 text-gray-700"
                  >
                    <i class="fa-solid fa-server text-xs text-gray-400"></i>
                    {t.name}
                    <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-full text-xs {stateStyle[t.state] ?? 'bg-gray-100 text-gray-500'}">
                      <i class="{stateIcon[t.state] ?? 'fa-solid fa-circle'} text-xs"></i>
                      {t.state}
                    </span>
                  </button>
                {/each}

                <!-- Add new target -->
                {#if !c.addingNew}
                  <button
                    on:click={() => { cards[run.info.run_id].addingNew = true; cards = cards; }}
                    class="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-dashed border-gray-300 text-sm text-gray-400 hover:border-peach hover:text-peach-dark transition-colors"
                  >
                    <i class="fa-solid fa-plus text-xs"></i>New target
                  </button>
                {/if}
              </div>

              {#if c.addingNew}
                <div class="flex gap-2">
                  <input
                    type="text"
                    placeholder="Target name"
                    bind:value={cards[run.info.run_id].newTarget}
                    on:keydown={(e) => e.key === 'Enter' && deployNew(run)}
                    class="flex-1 border border-gray-200 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach"
                  />
                  <button
                    on:click={() => deployNew(run)}
                    class="px-4 py-1.5 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors"
                  >
                    <i class="fa-solid fa-rocket text-xs mr-1"></i>Deploy
                  </button>
                  <button
                    on:click={() => { cards[run.info.run_id].addingNew = false; cards = cards; }}
                    class="px-2 py-1.5 rounded-lg text-gray-400 hover:bg-gray-100 transition-colors"
                  >
                    <i class="fa-solid fa-xmark text-sm"></i>
                  </button>
                </div>
              {/if}

              {#if c.err}
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

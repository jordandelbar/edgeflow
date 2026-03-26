<script lang="ts">
  import { onMount } from 'svelte';
  import { models, deployments, modelName, runTag, type Run, type Deployment } from '$lib/api';

  let items: Run[] = [];
  let error = '';

  // Per-card deploy state
  let deploying: Record<string, { open: boolean; target: string; err: string; dep: Deployment | null; polling: boolean }> = {};

  onMount(async () => {
    try {
      const res = await models.list();
      items = res.runs ?? [];
      items.forEach(r => {
        deploying[r.info.run_id] = { open: false, target: '', err: '', dep: null, polling: false };
      });
    } catch (e) {
      error = String(e);
    }
  });

  function toggle(run_id: string) {
    deploying[run_id].open = !deploying[run_id].open;
    deploying[run_id].err = '';
    deploying = deploying; // trigger reactivity
  }

  async function deploy(run: Run) {
    const s = deploying[run.info.run_id];
    if (!s.target.trim()) { s.err = 'Target name is required.'; deploying = deploying; return; }
    s.err = '';
    s.polling = true;
    deploying = deploying;

    try {
      const res = await deployments.create(run.info.run_id, s.target.trim());
      s.dep = res.deployment;
      deploying = deploying;
      poll(run.info.run_id, res.deployment.deployment_id);
    } catch (e) {
      s.err = String(e);
      s.polling = false;
      deploying = deploying;
    }
  }

  function poll(run_id: string, dep_id: string) {
    const iv = setInterval(async () => {
      try {
        const res = await deployments.getById(dep_id);
        deploying[run_id].dep = res.deployment;
        deploying = deploying;
        if (['healthy', 'failed', 'superseded'].includes(res.deployment.state)) {
          deploying[run_id].polling = false;
          deploying = deploying;
          clearInterval(iv);
        }
      } catch {
        clearInterval(iv);
      }
    }, 2000);
  }

  const stateStyle: Record<string, string> = {
    pending:    'bg-gray-100 text-gray-600',
    deploying:  'bg-peach-light/60 text-peach-dark',
    upgrading:  'bg-peach-light/60 text-peach-dark',
    healthy:    'bg-sage-light/40 text-sage-dark',
    failed:     'bg-red-100 text-red-700',
    superseded: 'bg-gray-100 text-gray-500',
  };

  const stateIcon: Record<string, string> = {
    pending:    'fa-solid fa-clock',
    deploying:  'fa-solid fa-spinner fa-spin',
    upgrading:  'fa-solid fa-spinner fa-spin',
    healthy:    'fa-solid fa-circle-check',
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
      {@const s = deploying[run.info.run_id] ?? { open: false, target: '', err: '', dep: null, polling: false }}
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
          <span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-sage-light/30 text-sage-dark shrink-0">
            <i class="fa-solid fa-check text-xs"></i>promoted
          </span>
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
          {#if !s.open && !s.dep}
            <button
              on:click={() => toggle(run.info.run_id)}
              class="w-full flex items-center justify-center gap-2 px-5 py-3 text-sm font-semibold text-peach-dark hover:bg-peach-light/10 transition-colors"
            >
              <i class="fa-solid fa-rocket text-xs"></i>Deploy
            </button>
          {:else if s.dep}
            <!-- Deployment state -->
            <div class="px-5 py-3 flex items-center justify-between gap-3">
              <div class="flex items-center gap-2 text-sm">
                <i class="{stateIcon[s.dep.state] ?? 'fa-solid fa-circle'} text-xs {s.dep.state === 'deploying' || s.dep.state === 'upgrading' ? 'text-peach' : ''}"></i>
                <span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {stateStyle[s.dep.state] ?? 'bg-gray-100 text-gray-600'}">
                  {s.dep.state}
                </span>
                <span class="text-gray-400 text-xs">→ {s.dep.target}</span>
              </div>
              {#if s.polling}
                <span class="text-xs text-gray-400 italic">polling…</span>
              {/if}
            </div>
          {:else}
            <!-- Deploy form -->
            <div class="px-5 py-3 space-y-2.5">
              <input
                type="text"
                placeholder="Target name (e.g. iris-inference)"
                bind:value={deploying[run.info.run_id].target}
                on:input={() => { deploying[run.info.run_id].err = ''; deploying = deploying; }}
                class="w-full border border-gray-200 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-peach/50 focus:border-peach"
              />
              {#if s.err}
                <p class="text-xs text-red-500">{s.err}</p>
              {/if}
              <div class="flex gap-2">
                <button
                  on:click={() => deploy(run)}
                  class="flex-1 flex items-center justify-center gap-2 py-2 rounded-lg text-sm font-semibold bg-peach text-white hover:bg-peach-dark transition-colors"
                >
                  <i class="fa-solid fa-rocket text-xs"></i>Start deployment
                </button>
                <button
                  on:click={() => toggle(run.info.run_id)}
                  class="px-3 py-2 rounded-lg text-sm text-gray-500 hover:bg-gray-100 transition-colors"
                >
                  <i class="fa-solid fa-xmark"></i>
                </button>
              </div>
            </div>
          {/if}
        </div>

      </div>
    {/each}
  </div>
{/if}

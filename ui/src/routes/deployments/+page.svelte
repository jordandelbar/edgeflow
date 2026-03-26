<script lang="ts">
  import { onMount } from 'svelte';
  import { deployments, type Deployment } from '$lib/api';

  let byTarget: Record<string, Deployment[]> = {};
  let targets: string[] = [];
  let expanded: Record<string, boolean> = {};
  let error = '';
  let loading = true;

  onMount(async () => {
    try {
      const res = await deployments.list();
      const all = res.deployments ?? [];
      // Group by target, preserving created_at DESC order from server
      for (const d of all) {
        if (!byTarget[d.target]) byTarget[d.target] = [];
        byTarget[d.target].push(d);
      }
      targets = Object.keys(byTarget);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  });

  function toggleExpand(t: string) {
    expanded[t] = !expanded[t];
    expanded = expanded;
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
    upgrading:  'fa-solid fa-arrows-rotate',
    healthy:    'fa-solid fa-circle-check',
    failed:     'fa-solid fa-circle-xmark',
    superseded: 'fa-solid fa-circle-minus',
  };

  function fmt(ms: number) {
    return new Date(ms).toLocaleString('en-GB', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
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
{:else if targets.length === 0}
  <div class="text-center py-20 text-gray-400">
    <i class="fa-solid fa-rocket text-4xl mb-3 block opacity-30"></i>
    <p class="text-sm">No deployments yet.</p>
    <p class="text-xs mt-1">Deploy a model from the Models section.</p>
  </div>
{:else}
  <div class="space-y-4">
    {#each targets as t}
      {@const deps = byTarget[t]}
      {@const latest = deps[0]}
      <div class="bg-white rounded-xl border border-gray-100 shadow-sm overflow-hidden">

        <!-- Target header -->
        <button
          on:click={() => toggleExpand(t)}
          class="w-full px-5 py-4 flex items-center gap-4 hover:bg-gray-50 transition-colors text-left"
        >
          <div class="w-9 h-9 rounded-lg flex items-center justify-center shrink-0" style="background:#edf4f1">
            <i class="fa-solid fa-server text-sage text-sm"></i>
          </div>
          <div class="flex-1 min-w-0">
            <p class="font-semibold text-gray-800">{t}</p>
            <p class="text-xs text-gray-400 mt-0.5">{deps.length} deployment{deps.length !== 1 ? 's' : ''} · last {fmt(latest.created_at)}</p>
          </div>
          <span class="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-medium {stateStyle[latest.state] ?? 'bg-gray-100 text-gray-600'}">
            <i class="{stateIcon[latest.state] ?? 'fa-solid fa-circle'} text-xs"></i>
            {latest.state}
          </span>
          <i class="fa-solid {expanded[t] ? 'fa-chevron-up' : 'fa-chevron-down'} text-xs text-gray-400 ml-1"></i>
        </button>

        <!-- History -->
        {#if expanded[t]}
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

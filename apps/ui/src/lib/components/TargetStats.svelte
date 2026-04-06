<script lang="ts">
  import type { TargetStats, InfraSettings } from '$lib/api';

  let {
    stats,
    history = [],
    infra = null,
    podCount = 1,
    loading = false,
  }: {
    stats: TargetStats | null;
    history?: TargetStats[];
    infra?: InfraSettings | null;
    podCount?: number;
    loading?: boolean;
  } = $props();

  const W = 80, H = 20;

  function sparkPaths(h: TargetStats[]): { p50: string; p99: string } {
    if (h.length < 2) return { p50: '', p99: '' };
    const p50vals = h.map(s => s.p50_ms ?? 0);
    const p99vals = h.map(s => s.p99_ms ?? 0);
    const max = Math.max(...p50vals, ...p99vals, 0.001);

    function toPath(vals: number[]): string {
      const pts = vals.map((v, i) => ({
        x: (i / (vals.length - 1)) * W,
        y: H - 1 - (v / max) * (H - 3),
      }));
      return pts.reduce((path, pt, i) => {
        if (i === 0) return `M ${pt.x.toFixed(1)} ${pt.y.toFixed(1)}`;
        const prev = pts[i - 1];
        const cx = ((prev.x + pt.x) / 2).toFixed(1);
        return `${path} C ${cx} ${prev.y.toFixed(1)} ${cx} ${pt.y.toFixed(1)} ${pt.x.toFixed(1)} ${pt.y.toFixed(1)}`;
      }, '');
    }

    return { p50: toPath(p50vals), p99: toPath(p99vals) };
  }

  function parseK8sBytes(s: string): number | null {
    const match = s.match(/^(\d+(?:\.\d+)?)\s*(Ki|Mi|Gi|Ti|K|M|G|T|)$/);
    if (!match) return null;
    const n = parseFloat(match[1]);
    const suffixes: Record<string, number> = {
      Ki: 1024, Mi: 1024 ** 2, Gi: 1024 ** 3, Ti: 1024 ** 4,
      K:  1000, M:  1000 ** 2, G:  1000 ** 3, T:  1000 ** 4,
      '': 1,
    };
    return n * (suffixes[match[2]] ?? 1);
  }

  function fmtBytes(b: number): string {
    if (b >= 1024 ** 3) return (b / 1024 ** 3).toFixed(1) + ' GB';
    if (b >= 1024 ** 2) return (b / 1024 ** 2).toFixed(0) + ' MB';
    if (b >= 1024)      return (b / 1024).toFixed(0) + ' KB';
    return b + ' B';
  }

  function fmtMs(ms: number): string {
    return ms < 1 ? ms.toFixed(2) + 'ms' : ms.toFixed(1) + 'ms';
  }

  function fmtRps(rps: number): string {
    return Math.round(rps) + '/s';
  }

  function fmtPct(ratio: number): string {
    const pct = ratio * 100;
    return pct < 10 ? pct.toFixed(1) + '%' : Math.round(pct) + '%';
  }

  let spark        = $derived(sparkPaths(history));
  let limitBytes   = $derived(infra?.memory_limit   ? parseK8sBytes(infra.memory_limit)   : null);
  let requestBytes = $derived(infra?.memory_request ? parseK8sBytes(infra.memory_request) : null);
  let provisionedBytes = $derived(
    (limitBytes ?? requestBytes) != null ? (limitBytes ?? requestBytes)! * podCount : null
  );
  let memPct    = $derived(
    stats?.memory_bytes != null && provisionedBytes
      ? Math.min(100, (stats.memory_bytes / provisionedBytes) * 100)
      : null
  );
  let memColour = $derived(
    memPct == null ? 'bg-gray-200' : memPct > 85 ? 'bg-red-400' : memPct > 65 ? 'bg-amber-400' : 'bg-sage'
  );
  let cpuPct    = $derived(stats?.cpu_ratio != null ? Math.min(100, stats.cpu_ratio * 100) : null);
  let cpuColour = $derived(
    cpuPct == null ? 'bg-gray-200' : cpuPct > 85 ? 'bg-red-400' : cpuPct > 65 ? 'bg-amber-400' : 'bg-sage'
  );
  let latencyRows = $derived(
    stats
      ? (['p50', 'p95', 'p99'] as const).map(k => [k, stats[`${k}_ms` as keyof TargetStats]] as [string, number | null])
      : [] as [string, number | null][]
  );
</script>

{#if loading}
  <div class="flex items-center gap-3 animate-pulse">
    <div class="h-3 w-10 rounded bg-gray-100"></div>
    <div class="h-3 w-24 rounded bg-gray-100"></div>
    <div class="h-2 w-20 rounded bg-gray-100"></div>
  </div>

{:else if stats}
  <div class="flex items-center gap-3 flex-wrap">

    <!-- req/s -->
    {#if stats.rps != null}
      <span class="inline-flex items-center gap-1 text-xs font-mono text-gray-500">
        <i class="fa-solid fa-bolt text-peach" style="font-size:9px"></i>{fmtRps(stats.rps)}
      </span>
    {/if}

    <!-- latency: sparkline + labeled values -->
    {#if stats.p50_ms != null}
      <span class="inline-flex items-center gap-2 whitespace-nowrap">

        {#if history.length >= 3}
          <span class="inline-flex flex-col gap-0.5">
            <span class="rounded overflow-hidden border border-gray-100 bg-gray-50 inline-block" style="line-height:0">
              <svg width={W} height={H} style="display:block">
                {#if spark.p99}
                  <path d={spark.p99} fill="none" stroke="#f4a27a" stroke-width="1"
                        stroke-linecap="round" stroke-linejoin="round" opacity="0.5" />
                {/if}
                {#if spark.p50}
                  <path d={spark.p50} fill="none" stroke="#7aaa8a" stroke-width="1.5"
                        stroke-linecap="round" stroke-linejoin="round" />
                {/if}
              </svg>
            </span>
            <span class="inline-flex justify-between px-0.5" style="width:{W}px">
              <span class="inline-flex items-center gap-0.5">
                <span class="inline-block w-2 h-0.5 rounded" style="background:#7aaa8a"></span>
                <span class="text-gray-300 font-mono" style="font-size:8px">p50</span>
              </span>
              <span class="inline-flex items-center gap-0.5">
                <span class="inline-block w-2 h-0.5 rounded" style="background:#f4a27a; opacity:0.7"></span>
                <span class="text-gray-300 font-mono" style="font-size:8px">p99</span>
              </span>
            </span>
          </span>
        {/if}

        {#each latencyRows as [label, val] (label)}
          {#if val != null}
            <span class="inline-flex items-center gap-0.5">
              <span class="text-gray-300 font-mono" style="font-size:9px">{label}</span>
              <span class="text-xs text-gray-500 font-mono">{fmtMs(val)}</span>
            </span>
          {/if}
        {/each}

      </span>
    {/if}

    <!-- CPU bar -->
    {#if cpuPct != null}
      <span class="inline-flex items-center gap-1.5" title="CPU usage: {fmtPct(stats.cpu_ratio ?? 0)} of one core">
        <span class="text-gray-300 font-mono" style="font-size:9px">cpu</span>
        <span class="relative w-10 h-1.5 rounded-full bg-gray-100 overflow-hidden">
          <span
            class="absolute inset-y-0 left-0 rounded-full transition-all duration-500 {cpuColour}"
            style="width:{cpuPct}%"
          ></span>
        </span>
        <span class="text-xs text-gray-400 font-mono whitespace-nowrap">{fmtPct(stats.cpu_ratio ?? 0)}</span>
      </span>
    {/if}

    <!-- Memory bar -->
    {#if stats.memory_bytes != null}
      <span class="inline-flex items-center gap-1.5" title="{fmtBytes(stats.memory_bytes)}{provisionedBytes ? ' / ' + fmtBytes(provisionedBytes) : ''}">
        <span class="text-gray-300 font-mono" style="font-size:9px">mem</span>
        {#if memPct != null}
          <span class="relative w-10 h-1.5 rounded-full bg-gray-100 overflow-hidden">
            <span
              class="absolute inset-y-0 left-0 rounded-full transition-all duration-500 {memColour}"
              style="width:{memPct}%"
            ></span>
          </span>
        {/if}
        <span class="text-xs text-gray-400 font-mono whitespace-nowrap">
          {fmtBytes(stats.memory_bytes)}{provisionedBytes ? ' / ' + fmtBytes(provisionedBytes) : ''}
        </span>
      </span>
    {/if}

  </div>
{/if}

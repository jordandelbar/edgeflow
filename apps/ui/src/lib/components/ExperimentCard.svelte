<script lang="ts">
  import type { Experiment } from '$lib/api';

  export let experiment: Experiment;

  function fmt(ms: number) {
    return new Date(ms).toLocaleDateString('en-GB', { day: 'numeric', month: 'short', year: 'numeric' });
  }
</script>

<a
  href="/experiments/{experiment.experiment_id}"
  class="block bg-white rounded-xl border border-gray-100 p-5 shadow-sm hover:shadow-md hover:-translate-y-0.5 transition-all duration-150 group"
>
  <div class="flex items-start justify-between gap-2 mb-3">
    <div class="w-9 h-9 rounded-lg flex items-center justify-center flex-shrink-0" style="background:#edf4f1">
      <i class="fa-solid fa-flask text-sage text-sm"></i>
    </div>
    <span class="text-xs text-gray-400 font-mono mt-1">#{experiment.experiment_id}</span>
  </div>
  <p class="font-semibold text-gray-800 group-hover:text-peach-dark transition-colors truncate">{experiment.name}</p>
  <p class="text-xs text-gray-400 mt-1">{fmt(experiment.creation_time)}</p>
</a>

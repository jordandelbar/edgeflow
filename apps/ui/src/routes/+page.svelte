<script lang="ts">
  const sections = [
    {
      href:        '/experiments',
      label:       'Experiments',
      icon:        'fa-solid fa-flask',
      description: 'Browse MLflow runs, inspect parameters, metrics, and artifacts.',
    },
    {
      href:        '/models',
      label:       'Models',
      icon:        'fa-solid fa-brain',
      description: 'Promote finished runs to models and deploy them to edge targets.',
    },
    {
      href:        '/deployments',
      label:       'Deployments',
      icon:        'fa-solid fa-rocket',
      description: 'Monitor active targets, check pod health, and run inference tests.',
    },
  ];
</script>

<div class="max-w-2xl mx-auto py-12">
  <div class="mb-10">
    <h1 class="text-2xl font-bold text-gray-900 mb-2">
      <i class="fa-solid fa-hexagon text-peach mr-2 text-xl"></i>edgeflow
    </h1>
    <p class="text-sm text-gray-500">
      ML experiment tracking and edge model deployment - all in one place.
    </p>
  </div>

  <div class="space-y-3">
    {#each sections as s}
      <a
        href={s.href}
        class="flex items-center gap-5 bg-white rounded-xl border border-gray-100 px-6 py-5 shadow-sm
               hover:shadow-md hover:-translate-y-0.5 transition-all duration-150 group"
      >
        <div class="w-11 h-11 rounded-xl flex items-center justify-center shrink-0" style="background:#edf4f1">
          <i class="{s.icon} text-sage text-base"></i>
        </div>
        <div class="flex-1 min-w-0">
          <p class="font-semibold text-gray-800 group-hover:text-peach-dark transition-colors">{s.label}</p>
          <p class="text-xs text-gray-400 mt-0.5">{s.description}</p>
        </div>
        <i class="fa-solid fa-chevron-right text-gray-300 group-hover:text-peach transition-colors text-xs shrink-0"></i>
      </a>
    {/each}
  </div>
</div>

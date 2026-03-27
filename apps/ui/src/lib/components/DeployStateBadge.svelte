<script lang="ts">
  export let state: string;

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
</script>

<span class="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium {stateStyle[state] ?? 'bg-gray-100 text-gray-600'}">
  <i class="{stateIcon[state] ?? 'fa-solid fa-circle'} text-xs"></i>
  {state}
</span>

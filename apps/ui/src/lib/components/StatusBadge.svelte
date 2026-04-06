<script lang="ts">
  let { status }: { status: string } = $props();

  const styles: Record<string, string> = {
    FINISHED: 'bg-sage-light/40 text-sage-dark',
    RUNNING:  'bg-peach-light/60 text-peach-dark',
    FAILED:   'bg-red-100 text-red-700',
    KILLED:   'bg-gray-100 text-gray-600',
  };
</script>

<span class="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium {styles[status] ?? 'bg-gray-100 text-gray-600'}">
  {status}
</span>

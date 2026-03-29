<script lang="ts">
  export let items: Array<{ label: string; href?: string }>;
</script>

<div class="flex items-center gap-2 text-sm text-gray-400 mb-5 flex-wrap">
  {#each items as item, i (i)}
    {#if i > 0}
      <i class="fa-solid fa-chevron-right text-xs"></i>
    {/if}
    {#if item.href}
      <a href={item.href} class="hover:text-gray-700 transition-colors">{item.label}</a>
    {:else}
      <span class="text-gray-700 font-medium">{item.label}</span>
    {/if}
  {/each}
</div>

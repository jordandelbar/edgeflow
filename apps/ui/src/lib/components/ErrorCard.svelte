<script lang="ts">
  let { message }: { message: string } = $props();
</script>

<div class="flex items-center gap-2 text-red-600 bg-red-50 border border-red-200 rounded-lg px-4 py-3 text-sm">
  <i class="fa-solid fa-circle-exclamation"></i>{message}
</div>

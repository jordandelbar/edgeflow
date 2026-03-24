<script lang="ts">
  import { page } from '$app/stores';
</script>

<nav>
  <a href="/">edgeflow</a>
</nav>

<main>
  <slot />
</main>

<style>
  nav {
    padding: 0.75rem 1.5rem;
    background: #1a1a2e;
    color: #e0e0e0;
    font-weight: 600;
    font-size: 1.1rem;
  }
  nav a { color: inherit; text-decoration: none; }
  main { padding: 1.5rem; }
</style>

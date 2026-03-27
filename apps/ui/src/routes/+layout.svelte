<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';

  const nav = [
    { href: '/',            label: 'Experiments', icon: 'fa-solid fa-flask'  },
    { href: '/models',      label: 'Models',       icon: 'fa-solid fa-brain'  },
    { href: '/deployments', label: 'Deployments',  icon: 'fa-solid fa-rocket' },
  ];

  // Must be a $: declaration so Svelte sees $page as a direct dependency
  // and re-evaluates whenever the route changes.
  $: pathname = $page.url.pathname;
  $: current = nav.find(n =>
    n.href === '/' ? (pathname === '/' || pathname.startsWith('/experiments')) : pathname.startsWith(n.href)
  ) ?? nav[0];
</script>

<div class="flex h-screen overflow-hidden bg-cream text-gray-900 font-sans">

  <!-- ── Sidebar ─────────────────────────────────────────── -->
  <aside class="w-52 shrink-0 flex flex-col" style="background:#1e2d28">
    <div class="px-5 py-5 select-none">
      <span class="text-peach font-bold text-lg tracking-tight">
        <i class="fa-solid fa-hexagon mr-1.5 text-sm"></i>edgeflow
      </span>
    </div>

    <nav class="flex-1 px-2 space-y-0.5">
      {#each nav as item}
        {@const isActive = item.href === '/'
          ? (pathname === '/' || pathname.startsWith('/experiments'))
          : pathname.startsWith(item.href)}
        <a
          href={item.href}
          class="flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm transition-colors duration-100
            {isActive
              ? 'bg-white/10 text-peach font-medium'
              : 'text-sage-light/80 hover:bg-white/5 hover:text-white'}"
        >
          <i class="{item.icon} w-4 text-center opacity-80"></i>
          {item.label}
        </a>
      {/each}
    </nav>

    <div class="px-5 py-4 text-xs" style="color:#4a6a5a">v0.1.0</div>
  </aside>

  <!-- ── Right pane ──────────────────────────────────────── -->
  <div class="flex-1 flex flex-col overflow-hidden">

    <!-- Top bar -->
    <header class="h-12 shrink-0 bg-white border-b border-gray-100 flex items-center px-6 gap-2">
      <i class="{current.icon} text-sage text-sm"></i>
      <span class="text-sm font-semibold text-gray-700">{current.label}</span>
    </header>

    <!-- Page content -->
    <main class="flex-1 overflow-y-auto p-6 bg-cream">
      <slot />
    </main>

  </div>
</div>

import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  // Svelte exports `browser` condition for its runtime (src/runtime/index.js
  // with real onMount/onDestroy). Without this, Vitest resolves to ssr.js
  // where onMount is a no-op, making async component tests impossible.
  resolve: { conditions: ['browser'] },
  server: {
    proxy: {
      '/api': 'http://localhost:5000',
    },
  },
  test: {
    include: ['src/**/*.{test,spec}.{js,ts}'],
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test-setup.ts'],
  },
});

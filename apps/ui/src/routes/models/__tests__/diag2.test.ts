import { render, waitFor, screen } from '@testing-library/svelte';
import { it, expect, beforeEach, afterEach, vi } from 'vitest';

// Minimal Svelte component with onMount + fetch call
import { onMount } from 'svelte';

it('fetch mock works in component onMount', async () => {
  // Override fetch before render
  const originalFetch = globalThis.fetch;
  globalThis.fetch = vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ ok: true }), { status: 200, headers: { 'content-type': 'application/json' } })
  ) as typeof fetch;

  // Check the fetch reference is replaced
  console.log('globalThis.fetch is mock?', (globalThis.fetch as any).mock !== undefined);

  // Just verify the mock works in this test context
  const res = await globalThis.fetch('http://test');
  const data = await res.json();
  console.log('direct fetch result:', data);
  expect(data).toEqual({ ok: true });

  globalThis.fetch = originalFetch;
});

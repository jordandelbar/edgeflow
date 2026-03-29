import { it, expect, vi } from 'vitest';

it('fetch mock works in component onMount', async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = vi.fn().mockResolvedValue(
    new Response(JSON.stringify({ ok: true }), { status: 200, headers: { 'content-type': 'application/json' } })
  ) as typeof fetch;

  const res = await globalThis.fetch('http://test');
  const data = await res.json();
  expect(data).toEqual({ ok: true });

  globalThis.fetch = originalFetch;
});

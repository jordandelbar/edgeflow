import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/svelte';
import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import Page from '../+page.svelte';

// models.list() calls experiments.list() first (mget → /experiments/list),
// then /runs/search with a promoted-tag filter.  Both must return data or
// models.list() short-circuits with { runs: [] }.
const originalFetch = globalThis.fetch;

function stubFetch() {
  globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
    const url = input.toString();
    const json = (body: unknown) =>
      new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

    if (url.includes('/experiments/list')) {
      return json({ experiments: [{ experiment_id: '1', name: 'test', artifact_location: '', lifecycle_stage: 'active', creation_time: 0, last_update_time: 0, tags: [] }] });
    }
    if (url.includes('/runs/search')) {
      return json({ runs: [{ info: { run_id: 'run-abc', experiment_id: '1', run_name: 'My Model', status: 'FINISHED', start_time: 0, end_time: null, artifact_uri: '', lifecycle_stage: 'active' }, data: { metrics: [], params: [], tags: [] } }] });
    }
    if (url.includes('/deployments')) {
      return json({ deployments: [{ deployment_id: 'dep-1', run_id: 'run-abc', target: 'prod', state: 'deployed', created_at: 0 }] });
    }
    if (url.includes('/targets')) {
      return json({ targets: [{ target: 'prod', address: 'http://pod', pod_name: null, node: null, registered_at: 0 }] });
    }
    return json({});
  }) as typeof fetch;
}

beforeEach(() => stubFetch());

afterEach(() => {
  cleanup();
  globalThis.fetch = originalFetch;
  vi.restoreAllMocks();
});

describe('models page — interval cleanup', () => {
  it('clears all polling intervals when the component is destroyed', async () => {
    render(Page);

    // Wait for onMount data load before installing the setInterval spy — the spy
    // must not interfere with Svelte's own initialisation.
    await waitFor(() => screen.getByText('My Model'));

    // Track every interval registered from here on.
    // pollOne is only triggered by user interaction, so nothing is missed.
    const registeredIds: ReturnType<typeof setInterval>[] = [];
    const origSetInterval = globalThis.setInterval.bind(globalThis);
    vi.spyOn(globalThis, 'setInterval').mockImplementation((fn: TimerHandler, delay?: number) => {
      const id = origSetInterval(fn, delay);
      registeredIds.push(id);
      return id;
    });
    const clearSpy = vi.spyOn(globalThis, 'clearInterval');

    // Open the deploy panel on the model card.
    fireEvent.click(screen.getByText('Deploy'));

    // Click the existing 'prod' target — triggers deployToExisting → pollOne → setInterval.
    await waitFor(() => screen.getByRole('button', { name: /prod/ }));
    fireEvent.click(screen.getByRole('button', { name: /prod/ }));

    // Wait until pollOne has registered its interval.
    await waitFor(() => expect(registeredIds.length).toBeGreaterThan(0));

    const idsBeforeDestroy = [...registeredIds];

    // Destroy the component — onDestroy must call clearInterval for every active interval.
    cleanup();

    for (const id of idsBeforeDestroy) {
      expect(clearSpy).toHaveBeenCalledWith(id);
    }
  });
});

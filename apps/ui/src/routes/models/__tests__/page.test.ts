import { render, screen, fireEvent, cleanup, waitFor } from '@testing-library/svelte';
import { vi, describe, it, expect, beforeEach, afterEach } from 'vitest';
import Page from '../+page.svelte';

const originalFetch = globalThis.fetch;

function stubFetch() {
  globalThis.fetch = vi.fn(async (input: RequestInfo | URL) => {
    const url = input.toString();
    const json = (body: unknown) =>
      new Response(JSON.stringify(body), { status: 200, headers: { 'content-type': 'application/json' } });

    if (url.includes('/registered-models/list')) {
      return json({
        registered_models: [{
          name: 'My Model',
          creation_time: 0,
          last_updated_time: 0,
          description: null,
          latest_versions: [{
            name: 'My Model', version: '1', creation_time: 0, last_updated_time: 0,
            current_stage: 'None', description: null, source: null,
            run_id: 'run-abc', status: 'READY',
          }],
        }],
      });
    }
    if (url.includes('/deployments')) {
      return json({ deployments: [{ deployment_id: 'dep-1', run_id: 'run-abc', model_name: 'My Model', model_version: '1', target: 'prod', state: 'deployed', created_at: 0 }] });
    }
    if (url.includes('/targets')) {
      return json({ targets: [{ target: 'prod', address: 'http://pod', pod_name: null, node: null, registered_at: 0, last_seen: null, health: 'unknown' }] });
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

    // Open the deploy modal.
    fireEvent.click(screen.getByText('Deploy'));

    // The modal now shows a version picker first — select v1.
    await waitFor(() => screen.getByRole('button', { name: /v1/ }));
    fireEvent.click(screen.getByRole('button', { name: /v1/ }));

    // Now the target picker is shown — click the existing 'prod' target.
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

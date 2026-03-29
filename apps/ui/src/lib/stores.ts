import { readable } from 'svelte/store';
import { deployments, targets, type Deployment, type Target } from '$lib/api';

export type LiveData = {
  deployments: Deployment[];
  targets: Target[];
  error: string;
  loaded: boolean;
};

let _refresh: (() => void) | null = null;

export const liveData = readable<LiveData>(
  { deployments: [], targets: [], error: '', loaded: false },
  (set) => {
    async function load() {
      try {
        const [depsRes, tgtsRes] = await Promise.all([deployments.list(), targets.list()]);
        set({
          deployments: depsRes.deployments ?? [],
          targets:     tgtsRes.targets ?? [],
          error:       '',
          loaded:      true,
        });
      } catch (e) {
        set({ deployments: [], targets: [], error: String(e), loaded: true });
      }
    }

    _refresh = load;
    load();
    const timer = setInterval(load, 5000);
    return () => { clearInterval(timer); _refresh = null; };
  }
);

export function refreshLiveData(): void {
  _refresh?.();
}

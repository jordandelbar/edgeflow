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
    let prev: LiveData = { deployments: [], targets: [], error: '', loaded: false };

    async function load() {
      try {
        const [depsRes, tgtsRes] = await Promise.all([deployments.list(), targets.list()]);
        // Preserve previous pod list when k8s returns empty — avoids flickering
        // pods away during transient k8s query failures or rolling restarts.
        const prevByTarget = new Map(prev.targets.map(t => [t.target, t]));
        const mergedTargets = (tgtsRes.targets ?? []).map(t => {
          if (t.pods.length > 0) return t;
          const p = prevByTarget.get(t.target);
          return p ? { ...t, pods: p.pods, health: p.health } : t;
        });
        prev = { deployments: depsRes.deployments ?? [], targets: mergedTargets, error: '', loaded: true };
        set(prev);
      } catch (e) {
        // On error keep previous data visible — only surface the message.
        prev = { ...prev, error: String(e), loaded: true };
        set(prev);
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

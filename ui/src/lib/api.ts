const MLFLOW = '/api/2.0/mlflow';
const V1     = '/api/v1';

async function mpost<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${MLFLOW}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

async function mget<T>(path: string, params?: Record<string, string>): Promise<T> {
  const url = new URL(`${MLFLOW}${path}`, window.location.origin);
  if (params) Object.entries(params).forEach(([k, v]) => url.searchParams.set(k, v));
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

async function v1post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${V1}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

async function v1get<T>(path: string, params?: Record<string, string>): Promise<T> {
  const url = new URL(`${V1}${path}`, window.location.origin);
  if (params) Object.entries(params).forEach(([k, v]) => url.searchParams.set(k, v));
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

// ── Types ──────────────────────────────────────────────────────────────────

export type Experiment = {
  experiment_id: string;
  name: string;
  artifact_location: string;
  lifecycle_stage: string;
  creation_time: number;
  last_update_time: number;
  tags: { key: string; value: string }[];
};

export type Run = {
  info: {
    run_id: string;
    experiment_id: string;
    run_name: string | null;
    status: string;
    start_time: number;
    end_time: number | null;
    artifact_uri: string;
    lifecycle_stage: string;
  };
  data: {
    metrics: Metric[];
    params: { key: string; value: string }[];
    tags: { key: string; value: string }[];
  };
};

export type Metric = {
  key: string;
  value: number;
  timestamp: number;
  step: number;
};

export type FileInfo = {
  path: string;
  is_dir: boolean;
  file_size: number | null;
};

export type Deployment = {
  deployment_id: string;
  run_id: string;
  target: string;
  state: string;
  created_at: number;
};

export type Target = {
  target: string;
  address: string;
  pod_name: string | null;
  registered_at: number;
};

// ── Helpers ────────────────────────────────────────────────────────────────

export function runTag(run: Run, key: string): string | undefined {
  return run.data.tags.find(t => t.key === key)?.value;
}

export function modelName(run: Run): string {
  return runTag(run, 'edgeflow.model_name')
    ?? run.info.run_name
    ?? run.info.run_id.slice(0, 8);
}

// ── Experiments ────────────────────────────────────────────────────────────

export const experiments = {
  list: () => mget<{ experiments: Experiment[] }>('/experiments/list'),
  get:  (id: string) => mget<{ experiment: Experiment }>('/experiments/get', { experiment_id: id }),
};

// ── Runs ───────────────────────────────────────────────────────────────────

export const runs = {
  search: (experiment_ids: string[]) =>
    mpost<{ runs: Run[] }>('/runs/search', { experiment_ids }),
  get:    (run_id: string) => mget<{ run: Run }>('/runs/get', { run_id }),
};

// ── Metrics / Artifacts ────────────────────────────────────────────────────

export const metrics = {
  getHistory: (run_id: string, metric_key: string) =>
    mget<{ metrics: Metric[] }>('/metrics/get-history', { run_id, metric_key }),
};

export const artifacts = {
  list: (run_id: string, path?: string) =>
    mget<{ root_uri: string; files: FileInfo[] }>(
      '/artifacts/list',
      path ? { run_id, path } : { run_id },
    ),
};

// ── Models (promoted runs) ─────────────────────────────────────────────────

export const models = {
  /** List all runs tagged edgeflow.promoted = true, across all experiments. */
  list: async (): Promise<{ runs: Run[] }> => {
    const { experiments: exps } = await experiments.list();
    const ids = (exps ?? []).map(e => e.experiment_id);
    if (ids.length === 0) return { runs: [] };
    return mpost<{ runs: Run[] }>('/runs/search', {
      experiment_ids: ids,
      filter: "tag.`edgeflow.promoted` = 'true'",
      max_results: 200,
    });
  },
  promote: (run_id: string) =>
    mpost('/runs/set-tag', { run_id, key: 'edgeflow.promoted', value: 'true' }),
};

// ── Deployments ────────────────────────────────────────────────────────────

export const deployments = {
  create:  (run_id: string, target: string) =>
    v1post<{ deployment: Deployment }>('/deployments', { run_id, target }),
  list:    () =>
    v1get<{ deployments: Deployment[] }>('/deployments'),
  listForTarget: (target: string) =>
    v1get<{ deployments: Deployment[] }>('/deployments', { target }),
  getById: (id: string) =>
    v1get<{ deployment: Deployment }>(`/deployments/${id}`),
  latest:  (target: string) =>
    v1get<{ deployment: Deployment }>('/deployments/latest', { target }),
};

const BASE = '/api/2.0/mlflow';

async function post<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

async function get<T>(path: string, params?: Record<string, string>): Promise<T> {
  const url = new URL(`${BASE}${path}`, window.location.origin);
  if (params) Object.entries(params).forEach(([k, v]) => url.searchParams.set(k, v));
  const res = await fetch(url.toString());
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

// --- Experiments ---

export type Experiment = {
  experiment_id: string;
  name: string;
  artifact_location: string;
  lifecycle_stage: string;
  creation_time: number;
  last_update_time: number;
  tags: { key: string; value: string }[];
};

export const experiments = {
  list: () => get<{ experiments: Experiment[] }>('/experiments/list'),
  get: (id: string) => get<{ experiment: Experiment }>('/experiments/get', { experiment_id: id }),
  create: (name: string) => post<{ experiment_id: string }>('/experiments/create', { name }),
  delete: (id: string) => post('/experiments/delete', { experiment_id: id }),
};

// --- Runs ---

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

export const runs = {
  search: (experiment_ids: string[]) =>
    post<{ runs: Run[] }>('/runs/search', { experiment_ids }),
  get: (run_id: string) => get<{ run: Run }>('/runs/get', { run_id }),
  create: (experiment_id: string, run_name?: string) =>
    post<{ run: Run }>('/runs/create', { experiment_id, run_name }),
  finish: (run_id: string) =>
    post('/runs/update', { run_id, status: 'FINISHED', end_time: Date.now() }),
};

// --- Metrics ---

export type Metric = {
  key: string;
  value: number;
  timestamp: number;
  step: number;
};

export const metrics = {
  getHistory: (run_id: string, metric_key: string) =>
    get<{ metrics: Metric[] }>('/metrics/get-history', { run_id, metric_key }),
};

// --- Artifacts ---

export type FileInfo = {
  path: string;
  is_dir: boolean;
  file_size: number | null;
};

export const artifacts = {
  list: (run_id: string, path?: string) =>
    get<{ root_uri: string; files: FileInfo[] }>('/artifacts/list', path ? { run_id, path } : { run_id }),
};

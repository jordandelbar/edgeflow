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

async function mdelete<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${MLFLOW}${path}`, {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
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

async function v1patch<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(`${V1}${path}`, {
    method: 'PATCH',
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
  model_name: string | null;
  model_version: string | null;
  target: string;
  state: string;
  created_at: number;
};

export type TargetHealth = 'healthy' | 'stale' | 'unhealthy' | 'unknown';

export type Target = {
  target: string;
  address: string;
  pod_name: string | null;
  node: string | null;
  registered_at: number;
  last_seen: number | null;
  health: TargetHealth;
  resources: ResourceSettings | null;
};

export type ResourceSettings = {
  cpu_request:    string | null;
  memory_request: string | null;
  memory_limit:   string | null;
  sessions:       number | null;
  max_concurrent: number | null;
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

// ── Model Registry ─────────────────────────────────────────────────────────────

export type ModelVersion = {
  name: string;
  version: string;
  creation_time: number;
  last_updated_time: number;
  current_stage: string;
  description: string | null;
  source: string | null;
  run_id: string | null;
  status: string;
};

export type RegisteredModel = {
  name: string;
  creation_time: number;
  last_updated_time: number;
  description: string | null;
  latest_versions: ModelVersion[];
};

export const registeredModels = {
  list: () =>
    mget<{ registered_models: RegisteredModel[] }>('/registered-models/list'),
  get: (name: string) =>
    mget<{ registered_model: RegisteredModel }>('/registered-models/get', { name }),
  create: (name: string, description?: string) =>
    mpost<{ registered_model: RegisteredModel }>('/registered-models/create', { name, description }),
  delete: (name: string) =>
    mdelete('/registered-models/delete', { name }),
  createVersion: (name: string, run_id: string, source?: string) =>
    mpost<{ model_version: ModelVersion }>('/model-versions/create', { name, run_id, source }),
  listVersions: (name: string) =>
    mpost<{ model_versions: ModelVersion[] }>('/model-versions/search', { filter: `name = '${name}'` }),
  getVersionsByRunId: (run_id: string) =>
    mpost<{ model_versions: ModelVersion[] }>('/model-versions/search', { filter: `run_id = '${run_id}'` }),
  transitionStage: (name: string, version: string, stage: string) =>
    mpost<{ model_version: ModelVersion }>('/model-versions/transition-stage', { name, version, stage }),
  deleteVersion: (name: string, version: string) =>
    mdelete('/model-versions/delete', { name, version }),
};

// ── Deployments ────────────────────────────────────────────────────────────

export type ModelStatus = {
  run_id: string;
  deployment_id: string;
  target: string;
  loaded_at: string;
};

export const targets = {
  list:    () => v1get<{ targets: Target[] }>('/targets'),
  get:     (target: string) => v1get<{ target: Target }>(`/targets/${target}`),
  updateResources: (target: string, resources: Partial<ResourceSettings>) =>
    v1patch<{ target: Target; pod_restarted: boolean }>(`/targets/${target}/resources`, resources),
  model:   (target: string) =>
    v1get<ModelStatus>(`/targets/${target}/model`),
  health:  (target: string) =>
    v1get<{ status: string }>(`/targets/${target}/health`),
  playground: (target: string, data: number[]) =>
    v1post<{ shape: number[]; data: number[] }>(`/targets/${target}/infer/playground`, { data }),
  teardown: async (target: string): Promise<void> => {
    const res = await fetch(`${V1}/targets/${target}`, { method: 'DELETE' });
    if (!res.ok) throw new Error(await res.text());
  },
};

export const nodes = {
  list: () => v1get<{ nodes: string[] }>('/nodes'),
};

export const deployments = {
  create:  (model_name: string, model_version: string, target: string, node?: string | null, resources?: Partial<ResourceSettings>) =>
    v1post<{ deployment: Deployment }>('/deployments', { model_name, model_version, target, node, resources }),
  list:    () =>
    v1get<{ deployments: Deployment[] }>('/deployments'),
  listForTarget: (target: string) =>
    v1get<{ deployments: Deployment[] }>('/deployments', { target }),
  getById: (id: string) =>
    v1get<{ deployment: Deployment }>(`/deployments/${id}`),
  latest:  (target: string) =>
    v1get<{ deployment: Deployment }>('/deployments/latest', { target }),
};

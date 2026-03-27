CREATE TABLE IF NOT EXISTS deployments (
    deployment_id TEXT PRIMARY KEY,
    target        TEXT NOT NULL,
    run_id        TEXT NOT NULL REFERENCES runs(run_id),
    created_at    INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_deployments_target_created ON deployments(target, created_at DESC);

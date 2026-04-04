CREATE TABLE IF NOT EXISTS experiments (
    experiment_id     TEXT    PRIMARY KEY,
    name              TEXT    NOT NULL UNIQUE,
    artifact_location TEXT    NOT NULL,
    lifecycle_stage   TEXT    NOT NULL DEFAULT 'active',
    creation_time     INTEGER NOT NULL,
    last_update_time  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS experiment_tags (
    experiment_id TEXT NOT NULL REFERENCES experiments(experiment_id),
    key           TEXT NOT NULL,
    value         TEXT NOT NULL,
    PRIMARY KEY (experiment_id, key)
);

CREATE TABLE IF NOT EXISTS runs (
    run_id          TEXT    PRIMARY KEY,
    experiment_id   TEXT    NOT NULL REFERENCES experiments(experiment_id),
    run_name        TEXT,
    status          TEXT    NOT NULL DEFAULT 'RUNNING',
    start_time      INTEGER NOT NULL,
    end_time        INTEGER,
    artifact_uri    TEXT    NOT NULL,
    lifecycle_stage TEXT    NOT NULL DEFAULT 'active'
);

CREATE TABLE IF NOT EXISTS metrics (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id    TEXT    NOT NULL REFERENCES runs(run_id),
    key       TEXT    NOT NULL,
    value     REAL    NOT NULL,
    timestamp INTEGER NOT NULL,
    step      INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_metrics_run_key ON metrics(run_id, key);

CREATE TABLE IF NOT EXISTS params (
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    key    TEXT NOT NULL,
    value  TEXT NOT NULL,
    PRIMARY KEY (run_id, key)
);

CREATE TABLE IF NOT EXISTS run_tags (
    run_id TEXT NOT NULL REFERENCES runs(run_id),
    key    TEXT NOT NULL,
    value  TEXT NOT NULL,
    PRIMARY KEY (run_id, key)
);

CREATE TABLE IF NOT EXISTS deployments (
    deployment_id TEXT    PRIMARY KEY,
    target        TEXT    NOT NULL,
    run_id        TEXT    NOT NULL REFERENCES runs(run_id),
    created_at    INTEGER NOT NULL,
    state         TEXT    NOT NULL DEFAULT 'pending',
    model_name    TEXT,
    model_version TEXT
);

CREATE INDEX IF NOT EXISTS idx_deployments_target_created ON deployments(target, created_at DESC);

CREATE TABLE IF NOT EXISTS targets (
    target          TEXT    PRIMARY KEY,
    registered_at   INTEGER NOT NULL,
    sessions        INTEGER,
    max_concurrent  INTEGER,
    current_run_id  TEXT,
    model_loaded_at TEXT
);


CREATE TABLE IF NOT EXISTS registered_models (
    name              TEXT    PRIMARY KEY,
    description       TEXT,
    creation_time     INTEGER NOT NULL,
    last_updated_time INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS model_versions (
    name              TEXT    NOT NULL REFERENCES registered_models(name) ON DELETE CASCADE,
    version           INTEGER NOT NULL,
    run_id            TEXT,
    source            TEXT,
    description       TEXT,
    current_stage     TEXT    NOT NULL DEFAULT 'None',
    status            TEXT    NOT NULL DEFAULT 'READY',
    creation_time     INTEGER NOT NULL,
    last_updated_time INTEGER NOT NULL,
    PRIMARY KEY (name, version)
);

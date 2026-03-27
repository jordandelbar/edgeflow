CREATE TABLE IF NOT EXISTS experiments (
    experiment_id     TEXT PRIMARY KEY,
    name              TEXT NOT NULL UNIQUE,
    artifact_location TEXT NOT NULL,
    lifecycle_stage   TEXT NOT NULL DEFAULT 'active',
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
    run_id          TEXT PRIMARY KEY,
    experiment_id   TEXT NOT NULL REFERENCES experiments(experiment_id),
    run_name        TEXT,
    status          TEXT NOT NULL DEFAULT 'RUNNING',
    start_time      INTEGER NOT NULL,
    end_time        INTEGER,
    artifact_uri    TEXT NOT NULL,
    lifecycle_stage TEXT NOT NULL DEFAULT 'active'
);

CREATE TABLE IF NOT EXISTS metrics (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id    TEXT NOT NULL REFERENCES runs(run_id),
    key       TEXT NOT NULL,
    value     REAL NOT NULL,
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

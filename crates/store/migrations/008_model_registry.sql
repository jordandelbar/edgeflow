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

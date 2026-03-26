ALTER TABLE deployments ADD COLUMN state TEXT NOT NULL DEFAULT 'pending';

CREATE TABLE IF NOT EXISTS targets (
    target        TEXT PRIMARY KEY,
    address       TEXT NOT NULL,
    pod_name      TEXT,
    registered_at INTEGER NOT NULL
);

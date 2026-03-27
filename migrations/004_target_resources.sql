ALTER TABLE targets ADD COLUMN cpu_request    TEXT;
ALTER TABLE targets ADD COLUMN memory_request TEXT;
ALTER TABLE targets ADD COLUMN memory_limit   TEXT;
ALTER TABLE targets ADD COLUMN max_concurrent INTEGER;

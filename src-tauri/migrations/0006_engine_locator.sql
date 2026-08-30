ALTER TABLE projects
ADD COLUMN engine_id TEXT NOT NULL DEFAULT 'duckdb';

ALTER TABLE projects
ADD COLUMN locator_json TEXT NOT NULL DEFAULT '{}';

CREATE INDEX projects_engine_idx ON projects(engine_id, last_opened_at DESC);

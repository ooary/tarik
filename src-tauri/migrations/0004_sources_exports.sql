CREATE TABLE sources (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('duckdb_table', 'linked_parquet', 'linked_csv')),
    state TEXT NOT NULL CHECK(state IN ('ready', 'missing', 'invalid_schema')),
    source_path TEXT,
    duckdb_name TEXT NOT NULL,
    options_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, duckdb_name)
) STRICT;

CREATE INDEX sources_project_idx ON sources(project_id, display_name COLLATE NOCASE);

CREATE TABLE export_history (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK(status IN ('queued', 'running', 'succeeded', 'failed', 'cancelled')),
    format TEXT NOT NULL CHECK(format IN ('csv', 'parquet')),
    output_directory TEXT NOT NULL,
    base_name TEXT NOT NULL,
    rows_per_part INTEGER NOT NULL CHECK(rows_per_part > 0),
    completed_parts_json TEXT NOT NULL DEFAULT '[]',
    error_message TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX export_history_project_time_idx ON export_history(project_id, created_at DESC);

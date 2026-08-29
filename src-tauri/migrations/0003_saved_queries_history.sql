CREATE TABLE query_folders (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE(project_id, name)
) STRICT;

CREATE TABLE saved_queries (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    folder_id TEXT REFERENCES query_folders(id) ON DELETE SET NULL,
    name TEXT NOT NULL,
    sql_text TEXT NOT NULL,
    tags_json TEXT NOT NULL DEFAULT '[]',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE INDEX saved_queries_project_name_idx ON saved_queries(project_id, name COLLATE NOCASE);

CREATE TABLE query_history (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    sql_text TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('succeeded', 'failed', 'cancelled')),
    duration_ms INTEGER CHECK(duration_ms IS NULL OR duration_ms >= 0),
    returned_rows INTEGER CHECK(returned_rows IS NULL OR returned_rows >= 0),
    error_code TEXT,
    error_message TEXT,
    executed_at TEXT NOT NULL
) STRICT;

CREATE INDEX query_history_project_time_idx ON query_history(project_id, executed_at DESC);
CREATE INDEX query_history_status_idx ON query_history(status, executed_at DESC);

CREATE TABLE query_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE query_tabs (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES query_sessions(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    sql_text TEXT NOT NULL,
    tab_position INTEGER NOT NULL CHECK(tab_position >= 0),
    is_active INTEGER NOT NULL CHECK(is_active IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(session_id, tab_position)
) STRICT;

CREATE UNIQUE INDEX query_tabs_one_active_idx
ON query_tabs(session_id)
WHERE is_active = 1;

CREATE INDEX query_tabs_session_order_idx
ON query_tabs(session_id, tab_position);

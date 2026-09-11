CREATE TABLE agent_audit (
  id TEXT PRIMARY KEY NOT NULL,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  client_id TEXT NOT NULL,
  connection_id TEXT NOT NULL,
  approval_id TEXT NOT NULL,
  tool TEXT NOT NULL,
  risk TEXT NOT NULL,
  snapshot_hash TEXT NOT NULL,
  decision TEXT NOT NULL,
  outcome TEXT NOT NULL,
  affected_objects_json TEXT NOT NULL,
  rows_affected INTEGER,
  rollback_state TEXT NOT NULL,
  error_code TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
) STRICT;

CREATE INDEX idx_agent_audit_project_created
  ON agent_audit(project_id, created_at DESC, id DESC);

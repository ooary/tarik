CREATE TABLE agent_export_destinations (
  id TEXT PRIMARY KEY NOT NULL,
  client_id TEXT NOT NULL REFERENCES agent_clients(id) ON DELETE CASCADE,
  project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  canonical_path TEXT NOT NULL,
  directory_identity TEXT NOT NULL,
  display_label TEXT NOT NULL CHECK(length(trim(display_label)) BETWEEN 1 AND 80),
  allow_csv INTEGER NOT NULL CHECK(allow_csv IN (0, 1)),
  allow_parquet INTEGER NOT NULL CHECK(allow_parquet IN (0, 1)),
  maximum_rows_per_part INTEGER NOT NULL CHECK(maximum_rows_per_part BETWEEN 1 AND 1000000),
  maximum_total_bytes INTEGER NOT NULL CHECK(maximum_total_bytes BETWEEN 1 AND 107374182400),
  create_new_only INTEGER NOT NULL DEFAULT 1 CHECK(create_new_only = 1),
  enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
  revision INTEGER NOT NULL CHECK(revision >= 1),
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(client_id, project_id, canonical_path)
) STRICT;

CREATE INDEX idx_agent_export_destinations_owner
  ON agent_export_destinations(client_id, project_id, enabled, updated_at DESC, id);

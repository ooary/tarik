CREATE TABLE quality_checks (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name TEXT NOT NULL COLLATE NOCASE,
    check_type TEXT NOT NULL CHECK(check_type IN (
        'not_empty', 'not_null', 'unique', 'accepted_values',
        'range', 'relationship', 'freshness', 'custom_sql'
    )),
    target_json TEXT NOT NULL CHECK(json_valid(target_json)),
    null_policy TEXT NOT NULL CHECK(null_policy IN ('fail_on_null', 'pass_on_null')),
    severity TEXT NOT NULL CHECK(severity IN ('info', 'warning', 'critical')),
    enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE(project_id, name)
) STRICT;

CREATE INDEX quality_checks_project_updated_idx
    ON quality_checks(project_id, updated_at DESC, id);

CREATE TABLE quality_check_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    check_id TEXT NOT NULL REFERENCES quality_checks(id) ON DELETE CASCADE,
    revision_number INTEGER NOT NULL CHECK(revision_number > 0),
    definition_json TEXT NOT NULL CHECK(json_valid(definition_json)),
    created_at TEXT NOT NULL,
    UNIQUE(check_id, revision_number)
) STRICT;

CREATE INDEX quality_revisions_check_number_idx
    ON quality_check_revisions(check_id, revision_number DESC);

CREATE TABLE quality_check_runs (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    check_id TEXT NOT NULL REFERENCES quality_checks(id) ON DELETE CASCADE,
    revision_id TEXT NOT NULL REFERENCES quality_check_revisions(id) ON DELETE CASCADE,
    outcome TEXT NOT NULL CHECK(outcome IN ('pass', 'fail', 'error', 'cancelled')),
    failure_count INTEGER CHECK(failure_count IS NULL OR failure_count >= 0),
    duration_ms INTEGER NOT NULL CHECK(duration_ms >= 0),
    observed_at TEXT NOT NULL,
    error_code TEXT,
    created_at TEXT NOT NULL
) STRICT;

CREATE INDEX quality_runs_check_time_idx
    ON quality_check_runs(check_id, observed_at DESC, id DESC);
CREATE INDEX quality_runs_project_time_idx
    ON quality_check_runs(project_id, observed_at DESC, id DESC);

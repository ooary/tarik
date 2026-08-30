ALTER TABLE projects
ADD COLUMN ownership TEXT NOT NULL DEFAULT 'external'
CHECK(ownership IN ('managed', 'external'));

CREATE INDEX projects_ownership_idx ON projects(ownership, last_opened_at DESC);

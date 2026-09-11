CREATE TABLE agent_clients (
    id TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL CHECK(length(trim(display_name)) > 0),
    secret_salt BLOB NOT NULL CHECK(length(secret_salt) = 32),
    secret_verifier BLOB NOT NULL CHECK(length(secret_verifier) = 32),
    state TEXT NOT NULL CHECK(state IN ('paired', 'revoked')),
    created_at TEXT NOT NULL,
    last_connected_at TEXT,
    revoked_at TEXT
) STRICT;

CREATE INDEX agent_clients_state_connected_idx
    ON agent_clients(state, last_connected_at DESC, id);

CREATE TABLE agent_project_grants (
    client_id TEXT NOT NULL REFERENCES agent_clients(id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    can_inspect INTEGER NOT NULL CHECK(can_inspect IN (0, 1)),
    can_analyze INTEGER NOT NULL CHECK(can_analyze IN (0, 1)),
    can_modify_workspace INTEGER NOT NULL CHECK(can_modify_workspace IN (0, 1)),
    can_modify_data INTEGER NOT NULL CHECK(can_modify_data IN (0, 1)),
    updated_at TEXT NOT NULL,
    PRIMARY KEY(client_id, project_id)
) STRICT, WITHOUT ROWID;

CREATE INDEX agent_project_grants_project_idx
    ON agent_project_grants(project_id, client_id);

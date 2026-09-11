use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use tarik_agent_protocol::ProjectGrant;

use super::{MetadataDb, MetadataError};

pub const MAX_AGENT_CLIENTS: usize = 8;
pub const MAX_AGENT_PROJECT_GRANTS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentClientState {
    Paired,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentClientRecord {
    pub id: String,
    pub display_name: String,
    #[serde(skip)]
    pub secret_salt: Vec<u8>,
    #[serde(skip)]
    pub secret_verifier: Vec<u8>,
    pub state: AgentClientState,
    pub created_at: String,
    pub last_connected_at: Option<String>,
    pub revoked_at: Option<String>,
}

#[derive(Clone)]
pub struct AgentRepository {
    database: MetadataDb,
}

impl AgentRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn pair(
        &self,
        id: &str,
        display_name: &str,
        secret_salt: &[u8; 32],
        secret_verifier: &[u8; 32],
    ) -> Result<AgentClientRecord, MetadataError> {
        let display_name = display_name.trim();
        if id.trim().is_empty() || display_name.is_empty() {
            return Err(MetadataError::Invariant("agent client identity is empty"));
        }
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let count: i64 = transaction.query_row(
            "SELECT count(*) FROM agent_clients WHERE state = 'paired' AND id != ?1",
            [id],
            |row| row.get(0),
        )?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_AGENT_CLIENTS {
            return Err(MetadataError::Invariant("agent client limit reached"));
        }
        transaction.execute(
            "INSERT INTO agent_clients(
                id, display_name, secret_salt, secret_verifier, state,
                created_at, last_connected_at, revoked_at
             ) VALUES (?1, ?2, ?3, ?4, 'paired', datetime('now'), NULL, NULL)
             ON CONFLICT(id) DO UPDATE SET
                display_name = excluded.display_name,
                secret_salt = excluded.secret_salt,
                secret_verifier = excluded.secret_verifier,
                state = 'paired',
                last_connected_at = NULL,
                revoked_at = NULL",
            params![
                id,
                display_name,
                secret_salt.as_slice(),
                secret_verifier.as_slice()
            ],
        )?;
        let record = find_client(&transaction, id)?
            .ok_or(MetadataError::Invariant("paired client was not persisted"))?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn find_client(&self, id: &str) -> Result<Option<AgentClientRecord>, MetadataError> {
        let connection = self.database.connection()?;
        find_client(&connection, id)
    }

    pub fn list_clients(&self) -> Result<Vec<AgentClientRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, display_name, secret_salt, secret_verifier, state,
                    created_at, last_connected_at, revoked_at
             FROM agent_clients
             ORDER BY state ASC, coalesce(last_connected_at, created_at) DESC, id",
        )?;
        let records = statement
            .query_map([], read_client)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn mark_connected(&self, id: &str) -> Result<bool, MetadataError> {
        let connection = self.database.connection()?;
        Ok(connection.execute(
            "UPDATE agent_clients SET last_connected_at = datetime('now')
             WHERE id = ?1 AND state = 'paired'",
            [id],
        )? > 0)
    }

    pub fn revoke(&self, id: &str) -> Result<bool, MetadataError> {
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM agent_project_grants WHERE client_id = ?1",
            [id],
        )?;
        let changed = transaction.execute(
            "UPDATE agent_clients SET state = 'revoked', revoked_at = datetime('now')
             WHERE id = ?1 AND state = 'paired'",
            [id],
        )? > 0;
        transaction.commit()?;
        Ok(changed)
    }

    pub fn set_grant(&self, client_id: &str, grant: &ProjectGrant) -> Result<(), MetadataError> {
        if !grant.has_any() {
            return self.remove_grant(client_id, &grant.project_id).map(|_| ());
        }
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let paired: bool = transaction.query_row(
            "SELECT EXISTS(SELECT 1 FROM agent_clients WHERE id = ?1 AND state = 'paired')",
            [client_id],
            |row| row.get(0),
        )?;
        if !paired {
            return Err(MetadataError::Invariant("agent client is not paired"));
        }
        let count: i64 = transaction.query_row(
            "SELECT count(*) FROM agent_project_grants WHERE client_id = ?1 AND project_id != ?2",
            params![client_id, grant.project_id],
            |row| row.get(0),
        )?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_AGENT_PROJECT_GRANTS {
            return Err(MetadataError::Invariant(
                "agent project grant limit reached",
            ));
        }
        transaction.execute(
            "INSERT INTO agent_project_grants(
                client_id, project_id, can_inspect, can_analyze,
                can_modify_workspace, can_modify_data, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(client_id, project_id) DO UPDATE SET
                can_inspect = excluded.can_inspect,
                can_analyze = excluded.can_analyze,
                can_modify_workspace = excluded.can_modify_workspace,
                can_modify_data = excluded.can_modify_data,
                updated_at = excluded.updated_at",
            params![
                client_id,
                grant.project_id,
                grant.inspect,
                grant.analyze,
                grant.modify_workspace,
                grant.modify_data
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_grant(&self, client_id: &str, project_id: &str) -> Result<bool, MetadataError> {
        Ok(self.database.connection()?.execute(
            "DELETE FROM agent_project_grants WHERE client_id = ?1 AND project_id = ?2",
            params![client_id, project_id],
        )? > 0)
    }

    pub fn list_grants(&self, client_id: &str) -> Result<Vec<ProjectGrant>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT project_id, can_inspect, can_analyze, can_modify_workspace, can_modify_data
             FROM agent_project_grants WHERE client_id = ?1 ORDER BY project_id",
        )?;
        let grants = statement
            .query_map([client_id], |row| {
                Ok(ProjectGrant {
                    project_id: row.get(0)?,
                    inspect: row.get(1)?,
                    analyze: row.get(2)?,
                    modify_workspace: row.get(3)?,
                    modify_data: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(grants)
    }
}

fn find_client(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<Option<AgentClientRecord>, MetadataError> {
    Ok(connection
        .query_row(
            "SELECT id, display_name, secret_salt, secret_verifier, state,
                    created_at, last_connected_at, revoked_at
             FROM agent_clients WHERE id = ?1",
            [id],
            read_client,
        )
        .optional()?)
}

fn read_client(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentClientRecord> {
    let state: String = row.get(4)?;
    Ok(AgentClientRecord {
        id: row.get(0)?,
        display_name: row.get(1)?,
        secret_salt: row.get(2)?,
        secret_verifier: row.get(3)?,
        state: match state.as_str() {
            "paired" => AgentClientState::Paired,
            "revoked" => AgentClientState::Revoked,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        created_at: row.get(5)?,
        last_connected_at: row.get(6)?,
        revoked_at: row.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::metadata::projects::{ProjectOwnership, ProjectsRepository};

    use super::*;

    fn fixture() -> (AgentRepository, String) {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                Path::new("/tmp/agent-test.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        (AgentRepository::new(database), project.id)
    }

    #[test]
    fn pairing_rotates_verifier_and_revocation_cascades_grants() {
        let (repository, project_id) = fixture();
        repository.pair("client", "Pi", &[1; 32], &[2; 32]).unwrap();
        repository
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id,
                    inspect: true,
                    analyze: true,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        assert_eq!(repository.list_grants("client").unwrap().len(), 1);
        repository
            .pair("client", "Pi renamed", &[3; 32], &[4; 32])
            .unwrap();
        let rotated = repository.find_client("client").unwrap().unwrap();
        assert_eq!(rotated.secret_salt, vec![3; 32]);
        assert_eq!(rotated.secret_verifier, vec![4; 32]);
        assert!(repository.revoke("client").unwrap());
        assert!(repository.list_grants("client").unwrap().is_empty());
    }

    #[test]
    fn empty_grant_removes_existing_grant() {
        let (repository, project_id) = fixture();
        repository.pair("client", "Pi", &[1; 32], &[2; 32]).unwrap();
        repository
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id: project_id.clone(),
                    inspect: true,
                    analyze: false,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        repository
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id,
                    inspect: false,
                    analyze: false,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        assert!(repository.list_grants("client").unwrap().is_empty());
    }

    #[test]
    fn refuses_grants_for_unpaired_clients() {
        let (repository, project_id) = fixture();
        let error = repository
            .set_grant(
                "missing",
                &ProjectGrant {
                    project_id,
                    inspect: true,
                    analyze: false,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("not paired"));
    }
}

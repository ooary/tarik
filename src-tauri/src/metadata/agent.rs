use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use tarik_agent_protocol::ProjectGrant;

use super::{MetadataDb, MetadataError};

pub const MAX_AGENT_CLIENTS: usize = 8;
pub const MAX_AGENT_PROJECT_GRANTS: usize = 16;
pub const MAX_AGENT_AUDIT_PER_PROJECT: usize = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuditRecord {
    pub id: String,
    pub project_id: String,
    pub client_id: String,
    pub connection_id: String,
    pub approval_id: String,
    pub tool: String,
    pub risk: String,
    pub snapshot_hash: String,
    pub decision: String,
    pub outcome: String,
    pub affected_objects: Vec<String>,
    pub rows_affected: Option<u64>,
    pub rollback_state: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

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
            "DELETE FROM agent_export_destinations WHERE client_id = ?1",
            [id],
        )?;
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
        if !grant.analyze {
            transaction.execute(
                "DELETE FROM agent_export_destinations
                 WHERE client_id = ?1 AND project_id = ?2",
                params![client_id, grant.project_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn remove_grant(&self, client_id: &str, project_id: &str) -> Result<bool, MetadataError> {
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "DELETE FROM agent_export_destinations
             WHERE client_id = ?1 AND project_id = ?2",
            params![client_id, project_id],
        )?;
        let changed = transaction.execute(
            "DELETE FROM agent_project_grants WHERE client_id = ?1 AND project_id = ?2",
            params![client_id, project_id],
        )? > 0;
        transaction.commit()?;
        Ok(changed)
    }

    pub fn add_audit(&self, audit: &AgentAuditRecord) -> Result<(), MetadataError> {
        let affected_objects =
            serde_json::to_string(&audit.affected_objects).map_err(|source| {
                MetadataError::InvalidJson {
                    key: format!("agent-audit:{}:objects", audit.id),
                    source,
                }
            })?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO agent_audit(
                id, project_id, client_id, connection_id, approval_id, tool, risk,
                snapshot_hash, decision, outcome, affected_objects_json, rows_affected,
                rollback_state, error_code, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                audit.id,
                audit.project_id,
                audit.client_id,
                audit.connection_id,
                audit.approval_id,
                audit.tool,
                audit.risk,
                audit.snapshot_hash,
                audit.decision,
                audit.outcome,
                affected_objects,
                audit.rows_affected,
                audit.rollback_state,
                audit.error_code,
                audit.created_at,
            ],
        )?;
        transaction.execute(
            "DELETE FROM agent_audit
             WHERE project_id = ?1 AND id NOT IN (
               SELECT id FROM agent_audit WHERE project_id = ?1
               ORDER BY created_at DESC, id DESC LIMIT ?2
             )",
            params![audit.project_id, MAX_AGENT_AUDIT_PER_PROJECT as u64],
        )?;
        transaction.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn list_audit(
        &self,
        project_id: &str,
        limit: u32,
    ) -> Result<Vec<AgentAuditRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, client_id, connection_id, approval_id, tool, risk,
                    snapshot_hash, decision, outcome, affected_objects_json, rows_affected,
                    rollback_state, error_code, created_at
             FROM agent_audit WHERE project_id = ?1
             ORDER BY created_at DESC, id DESC LIMIT ?2",
        )?;
        let records = statement
            .query_map(params![project_id, limit.min(500)], |row| {
                let objects: String = row.get(10)?;
                Ok(AgentAuditRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    client_id: row.get(2)?,
                    connection_id: row.get(3)?,
                    approval_id: row.get(4)?,
                    tool: row.get(5)?,
                    risk: row.get(6)?,
                    snapshot_hash: row.get(7)?,
                    decision: row.get(8)?,
                    outcome: row.get(9)?,
                    affected_objects: serde_json::from_str(&objects).map_err(|error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            10,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    })?,
                    rows_affected: row.get(11)?,
                    rollback_state: row.get(12)?,
                    error_code: row.get(13)?,
                    created_at: row.get(14)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
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
    fn audit_is_project_scoped_and_excludes_sql() {
        let (repository, project_id) = fixture();
        repository
            .add_audit(&AgentAuditRecord {
                id: "audit-1".into(),
                project_id: project_id.clone(),
                client_id: "client".into(),
                connection_id: "connection".into(),
                approval_id: "approval".into(),
                tool: "tarik_execute_approved".into(),
                risk: "approvalrequired".into(),
                snapshot_hash: "hash".into(),
                decision: "approved".into(),
                outcome: "succeeded".into(),
                affected_objects: vec!["main.orders".into()],
                rows_affected: Some(1),
                rollback_state: "not_needed".into(),
                error_code: None,
                created_at: "2026-09-11T00:00:00Z".into(),
            })
            .unwrap();
        let audit = repository.list_audit(&project_id, 10).unwrap();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].snapshot_hash, "hash");
        assert_eq!(audit[0].affected_objects, ["main.orders"]);
        let json = serde_json::to_string(&audit[0]).unwrap();
        assert!(!json.contains("INSERT INTO"));
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

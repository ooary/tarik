use rusqlite::{params, OptionalExtension, TransactionBehavior};

use super::{MetadataDb, MetadataError};

pub const MAX_DESTINATIONS_PER_CLIENT_PROJECT: usize = 8;
pub const MAX_DESTINATION_BYTES: u64 = 100 * 1024 * 1024 * 1024;
pub const MAX_DESTINATION_ROWS_PER_PART: u64 = 1_000_000;

/// Private persistence shape. It intentionally does not implement Serialize.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportDestinationRecord {
    pub id: String,
    pub client_id: String,
    pub project_id: String,
    pub canonical_path: String,
    pub directory_identity: String,
    pub display_label: String,
    pub allow_csv: bool,
    pub allow_parquet: bool,
    pub maximum_rows_per_part: u64,
    pub maximum_total_bytes: u64,
    pub enabled: bool,
    pub revision: u64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DestinationPolicy {
    pub display_label: String,
    pub allow_csv: bool,
    pub allow_parquet: bool,
    pub maximum_rows_per_part: u64,
    pub maximum_total_bytes: u64,
}

impl DestinationPolicy {
    pub fn validate(&self) -> Result<(), MetadataError> {
        let label = self.display_label.trim();
        if label.is_empty() || label.len() > 80 {
            return Err(MetadataError::Invariant(
                "export destination label must contain 1-80 bytes",
            ));
        }
        if !self.allow_csv && !self.allow_parquet {
            return Err(MetadataError::Invariant(
                "export destination must allow CSV or Parquet",
            ));
        }
        if self.maximum_rows_per_part == 0
            || self.maximum_rows_per_part > MAX_DESTINATION_ROWS_PER_PART
        {
            return Err(MetadataError::Invariant(
                "export destination rows per part are outside 1-1000000",
            ));
        }
        if self.maximum_total_bytes == 0 || self.maximum_total_bytes > MAX_DESTINATION_BYTES {
            return Err(MetadataError::Invariant(
                "export destination byte quota is outside 1-100 GiB",
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct AgentDestinationRepository {
    database: MetadataDb,
}

impl AgentDestinationRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn create(
        &self,
        client_id: &str,
        project_id: &str,
        canonical_path: &str,
        directory_identity: &str,
        policy: &DestinationPolicy,
    ) -> Result<ExportDestinationRecord, MetadataError> {
        policy.validate()?;
        if client_id.trim().is_empty()
            || project_id.trim().is_empty()
            || canonical_path.is_empty()
            || directory_identity.is_empty()
        {
            return Err(MetadataError::Invariant(
                "export destination identity is incomplete",
            ));
        }
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let authorized: bool = transaction.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM agent_clients c
               JOIN agent_project_grants g ON g.client_id = c.id
               WHERE c.id = ?1 AND c.state = 'paired' AND g.project_id = ?2 AND g.can_analyze = 1
             )",
            params![client_id, project_id],
            |row| row.get(0),
        )?;
        if !authorized {
            return Err(MetadataError::Invariant(
                "paired client lacks Analyze access for the project",
            ));
        }
        let count: i64 = transaction.query_row(
            "SELECT count(*) FROM agent_export_destinations
             WHERE client_id = ?1 AND project_id = ?2",
            params![client_id, project_id],
            |row| row.get(0),
        )?;
        if usize::try_from(count).unwrap_or(usize::MAX) >= MAX_DESTINATIONS_PER_CLIENT_PROJECT {
            return Err(MetadataError::Invariant(
                "export destination grant limit reached",
            ));
        }
        let id = uuid::Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO agent_export_destinations(
               id, client_id, project_id, canonical_path, directory_identity,
               display_label, allow_csv, allow_parquet, maximum_rows_per_part,
               maximum_total_bytes, create_new_only, enabled, revision, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 1, 1,
                       datetime('now'), datetime('now'))",
            params![
                id,
                client_id,
                project_id,
                canonical_path,
                directory_identity,
                policy.display_label.trim(),
                policy.allow_csv,
                policy.allow_parquet,
                policy.maximum_rows_per_part,
                policy.maximum_total_bytes,
            ],
        )?;
        let record = find(&transaction, &id)?.ok_or(MetadataError::Invariant(
            "export destination was not persisted",
        ))?;
        transaction.commit()?;
        Ok(record)
    }

    pub fn find(&self, id: &str) -> Result<Option<ExportDestinationRecord>, MetadataError> {
        let connection = self.database.connection()?;
        find(&connection, id)
    }

    pub fn list_for_owner(
        &self,
        client_id: &str,
        project_id: &str,
    ) -> Result<Vec<ExportDestinationRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, client_id, project_id, canonical_path, directory_identity,
                    display_label, allow_csv, allow_parquet, maximum_rows_per_part,
                    maximum_total_bytes, enabled, revision, created_at, updated_at
             FROM agent_export_destinations
             WHERE client_id = ?1 AND project_id = ?2
             ORDER BY enabled DESC, updated_at DESC, id",
        )?;
        let records = statement
            .query_map(params![client_id, project_id], read_record)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub fn update_policy(
        &self,
        id: &str,
        client_id: &str,
        project_id: &str,
        policy: &DestinationPolicy,
    ) -> Result<Option<ExportDestinationRecord>, MetadataError> {
        policy.validate()?;
        let connection = self.database.connection()?;
        let updated = connection.execute(
            "UPDATE agent_export_destinations SET
               display_label = ?4, allow_csv = ?5, allow_parquet = ?6,
               maximum_rows_per_part = ?7, maximum_total_bytes = ?8,
               revision = revision + 1, updated_at = datetime('now')
             WHERE id = ?1 AND client_id = ?2 AND project_id = ?3",
            params![
                id,
                client_id,
                project_id,
                policy.display_label.trim(),
                policy.allow_csv,
                policy.allow_parquet,
                policy.maximum_rows_per_part,
                policy.maximum_total_bytes,
            ],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        find(&connection, id)
    }

    pub fn replace_directory(
        &self,
        id: &str,
        client_id: &str,
        project_id: &str,
        canonical_path: &str,
        directory_identity: &str,
    ) -> Result<Option<ExportDestinationRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let updated = connection.execute(
            "UPDATE agent_export_destinations SET
               canonical_path = ?4, directory_identity = ?5, enabled = 1,
               revision = revision + 1, updated_at = datetime('now')
             WHERE id = ?1 AND client_id = ?2 AND project_id = ?3",
            params![
                id,
                client_id,
                project_id,
                canonical_path,
                directory_identity
            ],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        find(&connection, id)
    }

    pub fn set_enabled(
        &self,
        id: &str,
        client_id: &str,
        project_id: &str,
        enabled: bool,
    ) -> Result<Option<ExportDestinationRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let updated = connection.execute(
            "UPDATE agent_export_destinations SET enabled = ?4,
               revision = revision + 1, updated_at = datetime('now')
             WHERE id = ?1 AND client_id = ?2 AND project_id = ?3",
            params![id, client_id, project_id, enabled],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        find(&connection, id)
    }

    pub fn revoke(
        &self,
        id: &str,
        client_id: &str,
        project_id: &str,
    ) -> Result<bool, MetadataError> {
        Ok(self.database.connection()?.execute(
            "DELETE FROM agent_export_destinations
             WHERE id = ?1 AND client_id = ?2 AND project_id = ?3",
            params![id, client_id, project_id],
        )? > 0)
    }
}

fn find(
    connection: &rusqlite::Connection,
    id: &str,
) -> Result<Option<ExportDestinationRecord>, MetadataError> {
    Ok(connection
        .query_row(
            "SELECT id, client_id, project_id, canonical_path, directory_identity,
                    display_label, allow_csv, allow_parquet, maximum_rows_per_part,
                    maximum_total_bytes, enabled, revision, created_at, updated_at
             FROM agent_export_destinations WHERE id = ?1",
            [id],
            read_record,
        )
        .optional()?)
}

fn read_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExportDestinationRecord> {
    Ok(ExportDestinationRecord {
        id: row.get(0)?,
        client_id: row.get(1)?,
        project_id: row.get(2)?,
        canonical_path: row.get(3)?,
        directory_identity: row.get(4)?,
        display_label: row.get(5)?,
        allow_csv: row.get(6)?,
        allow_parquet: row.get(7)?,
        maximum_rows_per_part: row.get(8)?,
        maximum_total_bytes: row.get(9)?,
        enabled: row.get(10)?,
        revision: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tarik_agent_protocol::ProjectGrant;

    use crate::metadata::{
        agent::AgentRepository,
        projects::{ProjectOwnership, ProjectsRepository},
    };

    use super::*;

    fn fixture() -> (AgentDestinationRepository, AgentRepository, String) {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                Path::new("/tmp/agent-destination.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        let agents = AgentRepository::new(database.clone());
        agents.pair("client", "Pi", &[1; 32], &[2; 32]).unwrap();
        agents
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id: project.id.clone(),
                    inspect: true,
                    analyze: true,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        (
            AgentDestinationRepository::new(database),
            agents,
            project.id,
        )
    }

    fn policy(label: &str) -> DestinationPolicy {
        DestinationPolicy {
            display_label: label.into(),
            allow_csv: true,
            allow_parquet: true,
            maximum_rows_per_part: 1_000_000,
            maximum_total_bytes: 1024 * 1024,
        }
    }

    #[test]
    fn destination_is_owner_scoped_revisioned_and_cascades_on_revoke() {
        let (repository, agents, project_id) = fixture();
        let created = repository
            .create(
                "client",
                &project_id,
                "/tmp/export",
                "unix:1:2",
                &policy("Exports"),
            )
            .unwrap();
        assert_eq!(created.revision, 1);
        assert_eq!(
            repository
                .list_for_owner("client", &project_id)
                .unwrap()
                .len(),
            1
        );
        assert!(repository
            .list_for_owner("other", &project_id)
            .unwrap()
            .is_empty());
        let updated = repository
            .update_policy(&created.id, "client", &project_id, &policy("Daily exports"))
            .unwrap()
            .unwrap();
        assert_eq!(updated.revision, 2);
        assert_eq!(updated.display_label, "Daily exports");
        agents
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
        assert!(repository.find(&created.id).unwrap().is_none());

        agents
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id: project_id.clone(),
                    inspect: true,
                    analyze: true,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        let recreated = repository
            .create(
                "client",
                &project_id,
                "/tmp/export",
                "unix:1:2",
                &policy("Exports"),
            )
            .unwrap();
        agents.revoke("client").unwrap();
        assert!(repository.find(&recreated.id).unwrap().is_none());
    }

    #[test]
    fn destination_limit_is_bounded_and_records_survive_reopen() {
        let root =
            std::env::temp_dir().join(format!("tarik-destination-reopen-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let database_path = root.join("metadata.sqlite");
        let project_id;
        {
            let database = MetadataDb::open(&database_path).unwrap();
            let project = ProjectsRepository::new(database.clone())
                .upsert(
                    "Test",
                    Path::new("/tmp/agent-destination-reopen.duckdb"),
                    ProjectOwnership::External,
                )
                .unwrap();
            project_id = project.id;
            let agents = AgentRepository::new(database.clone());
            agents.pair("client", "Pi", &[1; 32], &[2; 32]).unwrap();
            agents
                .set_grant(
                    "client",
                    &ProjectGrant {
                        project_id: project_id.clone(),
                        inspect: true,
                        analyze: true,
                        modify_workspace: false,
                        modify_data: false,
                    },
                )
                .unwrap();
            let destinations = AgentDestinationRepository::new(database);
            for index in 0..MAX_DESTINATIONS_PER_CLIENT_PROJECT {
                destinations
                    .create(
                        "client",
                        &project_id,
                        &format!("/tmp/export-{index}"),
                        &format!("unix:1:{index}"),
                        &policy(&format!("Export {index}")),
                    )
                    .unwrap();
            }
            assert!(destinations
                .create(
                    "client",
                    &project_id,
                    "/tmp/export-overflow",
                    "unix:1:99",
                    &policy("Overflow"),
                )
                .is_err());
        }
        let reopened = MetadataDb::open(&database_path).unwrap();
        assert_eq!(
            AgentDestinationRepository::new(reopened)
                .list_for_owner("client", &project_id)
                .unwrap()
                .len(),
            MAX_DESTINATIONS_PER_CLIENT_PROJECT
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn destination_requires_analyze_and_closed_bounded_policy() {
        let (repository, agents, project_id) = fixture();
        agents
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
        assert!(repository
            .create(
                "client",
                &project_id,
                "/tmp/export",
                "unix:1:2",
                &policy("Exports")
            )
            .is_err());
        let mut invalid = policy("Exports");
        invalid.allow_csv = false;
        invalid.allow_parquet = false;
        assert!(invalid.validate().is_err());
        invalid.allow_csv = true;
        invalid.maximum_total_bytes = MAX_DESTINATION_BYTES + 1;
        assert!(invalid.validate().is_err());
    }
}

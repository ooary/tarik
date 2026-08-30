use rusqlite::TransactionBehavior;
use serde::{Deserialize, Serialize};

use super::{MetadataDb, MetadataError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQuery {
    pub id: String,
    pub project_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub sql_text: String,
    pub tags: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryEntry {
    pub id: String,
    pub project_id: String,
    pub sql_text: String,
    pub status: ExecutionStatus,
    pub duration_ms: Option<u64>,
    pub returned_rows: Option<u64>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub executed_at: String,
}

#[derive(Clone)]
pub struct QueriesRepository {
    database: MetadataDb,
}

impl QueriesRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn upsert_saved(&self, query: &SavedQuery) -> Result<(), MetadataError> {
        let tags =
            serde_json::to_string(&query.tags).map_err(|source| MetadataError::InvalidJson {
                key: format!("saved-query:{}:tags", query.id),
                source,
            })?;
        let connection = self.database.connection()?;
        connection.execute(
            "INSERT INTO saved_queries(id, project_id, folder_id, name, sql_text, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(id) DO UPDATE SET folder_id = excluded.folder_id, name = excluded.name,
             sql_text = excluded.sql_text, tags_json = excluded.tags_json, updated_at = excluded.updated_at",
            (
                &query.id,
                &query.project_id,
                &query.folder_id,
                &query.name,
                &query.sql_text,
                tags,
                &query.created_at,
                &query.updated_at,
            ),
        )?;
        Ok(())
    }

    pub fn list_saved(
        &self,
        project_id: &str,
        search: Option<&str>,
    ) -> Result<Vec<SavedQuery>, MetadataError> {
        let pattern = format!("%{}%", search.unwrap_or_default());
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, folder_id, name, sql_text, tags_json, created_at, updated_at
             FROM saved_queries WHERE project_id = ?1 AND (?2 = '%%' OR name LIKE ?2 OR sql_text LIKE ?2)
             ORDER BY updated_at DESC, name COLLATE NOCASE",
        )?;
        let queries = statement
            .query_map((project_id, pattern), read_saved_query)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(queries)
    }

    pub fn delete_saved(&self, id: &str) -> Result<bool, MetadataError> {
        Ok(self
            .database
            .connection()?
            .execute("DELETE FROM saved_queries WHERE id = ?1", [id])?
            > 0)
    }

    pub fn add_history(&self, entry: &QueryHistoryEntry) -> Result<(), MetadataError> {
        let connection = self.database.connection()?;
        connection.execute(
            "INSERT INTO query_history(id, project_id, sql_text, status, duration_ms, returned_rows,
             error_code, error_message, executed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            (
                &entry.id,
                &entry.project_id,
                &entry.sql_text,
                entry.status.as_str(),
                entry.duration_ms,
                entry.returned_rows,
                &entry.error_code,
                &entry.error_message,
                &entry.executed_at,
            ),
        )?;
        Ok(())
    }

    pub fn list_history(
        &self,
        project_id: &str,
        status: Option<ExecutionStatus>,
        limit: u32,
    ) -> Result<Vec<QueryHistoryEntry>, MetadataError> {
        let status = status.map(|value| value.as_str().to_owned());
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, sql_text, status, duration_ms, returned_rows,
             error_code, error_message, executed_at FROM query_history
             WHERE project_id = ?1 AND (?2 IS NULL OR status = ?2)
             ORDER BY executed_at DESC LIMIT ?3",
        )?;
        let history = statement
            .query_map((project_id, status, limit), read_history)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(history)
    }

    pub fn prune_history(&self, project_id: &str, keep: u32) -> Result<usize, MetadataError> {
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = transaction.execute(
            "DELETE FROM query_history WHERE project_id = ?1 AND id NOT IN (
                SELECT id FROM query_history WHERE project_id = ?1 ORDER BY executed_at DESC LIMIT ?2
             )",
            (project_id, keep),
        )?;
        transaction.commit()?;
        Ok(deleted)
    }
}

fn read_saved_query(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedQuery> {
    let id: String = row.get(0)?;
    let tags_json: String = row.get(5)?;
    let tags = serde_json::from_str(&tags_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok(SavedQuery {
        id,
        project_id: row.get(1)?,
        folder_id: row.get(2)?,
        name: row.get(3)?,
        sql_text: row.get(4)?,
        tags,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

fn read_history(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryHistoryEntry> {
    let status: String = row.get(3)?;
    let status = match status.as_str() {
        "succeeded" => ExecutionStatus::Succeeded,
        "failed" => ExecutionStatus::Failed,
        "cancelled" => ExecutionStatus::Cancelled,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    Ok(QueryHistoryEntry {
        id: row.get(0)?,
        project_id: row.get(1)?,
        sql_text: row.get(2)?,
        status,
        duration_ms: row.get(4)?,
        returned_rows: row.get(5)?,
        error_code: row.get(6)?,
        error_message: row.get(7)?,
        executed_at: row.get(8)?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::metadata::projects::{ProjectOwnership, ProjectsRepository};

    use super::*;

    fn setup() -> (QueriesRepository, MetadataDb, String) {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Retail",
                Path::new("/data/retail.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        (
            QueriesRepository::new(database.clone()),
            database,
            project.id,
        )
    }

    #[test]
    fn saved_query_crud_search_and_project_cascade() {
        let (repository, database, project_id) = setup();
        let query = SavedQuery {
            id: "saved-1".into(),
            project_id: project_id.clone(),
            folder_id: None,
            name: "Monthly revenue".into(),
            sql_text: "select sum(revenue) from orders".into(),
            tags: vec!["finance".into()],
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        repository.upsert_saved(&query).unwrap();
        assert_eq!(
            repository.list_saved(&project_id, Some("revenue")).unwrap(),
            vec![query]
        );
        assert!(repository.delete_saved("saved-1").unwrap());

        database
            .connection()
            .unwrap()
            .execute("DELETE FROM projects WHERE id = ?1", [&project_id])
            .unwrap();
        assert!(repository.list_saved(&project_id, None).unwrap().is_empty());
    }

    #[test]
    fn history_represents_terminal_states_and_prunes() {
        let (repository, _, project_id) = setup();
        for (index, status) in [
            ExecutionStatus::Succeeded,
            ExecutionStatus::Failed,
            ExecutionStatus::Cancelled,
        ]
        .into_iter()
        .enumerate()
        {
            repository
                .add_history(&QueryHistoryEntry {
                    id: format!("history-{index}"),
                    project_id: project_id.clone(),
                    sql_text: "select 1".into(),
                    status,
                    duration_ms: Some(index as u64),
                    returned_rows: Some(1),
                    error_code: None,
                    error_message: None,
                    executed_at: format!("2026-01-01T00:00:0{index}Z"),
                })
                .unwrap();
        }

        assert_eq!(
            repository
                .list_history(&project_id, None, 10)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(repository.prune_history(&project_id, 2).unwrap(), 1);
        assert_eq!(
            repository
                .list_history(&project_id, None, 10)
                .unwrap()
                .len(),
            2
        );
    }
}

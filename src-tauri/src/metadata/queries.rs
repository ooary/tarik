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
#[serde(rename_all = "camelCase")]
pub struct QueryFolder {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQueryDraft {
    pub project_id: String,
    pub folder_id: Option<String>,
    pub name: String,
    pub sql_text: String,
    pub tags: Vec<String>,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryFilter {
    pub status: Option<ExecutionStatus>,
    pub search: Option<String>,
    pub executed_from: Option<String>,
    pub executed_to: Option<String>,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryPage {
    pub entries: Vec<QueryHistoryEntry>,
    pub offset: u32,
    pub next_offset: Option<u32>,
}

#[derive(Clone)]
pub struct QueriesRepository {
    database: MetadataDb,
}

impl QueriesRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn create_saved(&self, draft: &SavedQueryDraft) -> Result<SavedQuery, MetadataError> {
        let draft = normalize_saved_draft(draft)?;
        let now = chrono::Utc::now().to_rfc3339();
        let query = SavedQuery {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: draft.project_id,
            folder_id: draft.folder_id,
            name: draft.name,
            sql_text: draft.sql_text,
            tags: draft.tags,
            created_at: now.clone(),
            updated_at: now,
        };
        self.insert_saved(&query)?;
        Ok(query)
    }

    pub fn update_saved(
        &self,
        id: &str,
        draft: &SavedQueryDraft,
    ) -> Result<SavedQuery, MetadataError> {
        let draft = normalize_saved_draft(draft)?;
        let tags = encode_tags(id, &draft.tags)?;
        let now = chrono::Utc::now().to_rfc3339();
        let connection = self.database.connection()?;
        ensure_folder(&connection, &draft.project_id, draft.folder_id.as_deref())?;
        if saved_name_exists(&connection, &draft.project_id, &draft.name, Some(id))? {
            return Err(MetadataError::SavedQueryConflict(draft.name));
        }
        let updated = connection.execute(
            "UPDATE saved_queries SET folder_id = ?1, name = ?2, sql_text = ?3, tags_json = ?4,
             updated_at = ?5 WHERE id = ?6 AND project_id = ?7",
            (
                &draft.folder_id,
                &draft.name,
                &draft.sql_text,
                tags,
                &now,
                id,
                &draft.project_id,
            ),
        )?;
        if updated == 0 {
            return Err(MetadataError::SavedQueryMissing);
        }
        connection.query_row(
            "SELECT id, project_id, folder_id, name, sql_text, tags_json, created_at, updated_at
             FROM saved_queries WHERE id = ?1",
            [id],
            read_saved_query,
        ).map_err(MetadataError::from)
    }

    fn insert_saved(&self, query: &SavedQuery) -> Result<(), MetadataError> {
        let tags = encode_tags(&query.id, &query.tags)?;
        let connection = self.database.connection()?;
        ensure_folder(&connection, &query.project_id, query.folder_id.as_deref())?;
        if saved_name_exists(&connection, &query.project_id, &query.name, None)? {
            return Err(MetadataError::SavedQueryConflict(query.name.clone()));
        }
        connection.execute(
            "INSERT INTO saved_queries(id, project_id, folder_id, name, sql_text, tags_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
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
             FROM saved_queries WHERE project_id = ?1
             AND (?2 = '%%' OR name LIKE ?2 OR sql_text LIKE ?2 OR tags_json LIKE ?2)
             ORDER BY updated_at DESC, name COLLATE NOCASE",
        )?;
        let queries = statement
            .query_map((project_id, pattern), read_saved_query)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(queries)
    }

    pub fn create_folder(
        &self,
        project_id: &str,
        name: &str,
    ) -> Result<QueryFolder, MetadataError> {
        let name = normalize_name(name, "folder name is empty")?;
        let folder = QueryFolder {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project_id.to_string(),
            name: name.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let connection = self.database.connection()?;
        if folder_name_exists(&connection, project_id, &name, None)? {
            return Err(MetadataError::QueryFolderConflict(name));
        }
        connection.execute(
            "INSERT INTO query_folders(id, project_id, name, created_at) VALUES (?1, ?2, ?3, ?4)",
            (
                &folder.id,
                &folder.project_id,
                &folder.name,
                &folder.created_at,
            ),
        )?;
        Ok(folder)
    }

    pub fn rename_folder(
        &self,
        project_id: &str,
        id: &str,
        name: &str,
    ) -> Result<QueryFolder, MetadataError> {
        let name = normalize_name(name, "folder name is empty")?;
        let connection = self.database.connection()?;
        if folder_name_exists(&connection, project_id, &name, Some(id))? {
            return Err(MetadataError::QueryFolderConflict(name));
        }
        let updated = connection.execute(
            "UPDATE query_folders SET name = ?1 WHERE id = ?2 AND project_id = ?3",
            (&name, id, project_id),
        )?;
        if updated == 0 {
            return Err(MetadataError::QueryFolderMissing);
        }
        connection
            .query_row(
                "SELECT id, project_id, name, created_at FROM query_folders WHERE id = ?1",
                [id],
                read_folder,
            )
            .map_err(MetadataError::from)
    }

    pub fn list_folders(&self, project_id: &str) -> Result<Vec<QueryFolder>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, name, created_at FROM query_folders
             WHERE project_id = ?1 ORDER BY name COLLATE NOCASE, id",
        )?;
        let folders = statement
            .query_map([project_id], read_folder)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(folders)
    }

    pub fn delete_folder(&self, project_id: &str, id: &str) -> Result<bool, MetadataError> {
        Ok(self.database.connection()?.execute(
            "DELETE FROM query_folders WHERE id = ?1 AND project_id = ?2",
            (id, project_id),
        )? > 0)
    }

    pub fn delete_saved(&self, project_id: &str, id: &str) -> Result<bool, MetadataError> {
        Ok(self.database.connection()?.execute(
            "DELETE FROM saved_queries WHERE id = ?1 AND project_id = ?2",
            (id, project_id),
        )? > 0)
    }

    pub fn add_history(&self, entry: &QueryHistoryEntry) -> Result<(), MetadataError> {
        let connection = self.database.connection()?;
        connection.execute(
            "INSERT INTO query_history(id, project_id, sql_text, status, duration_ms, returned_rows,
             error_code, error_message, executed_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO NOTHING",
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

    pub fn list_history_page(
        &self,
        project_id: &str,
        filter: &QueryHistoryFilter,
    ) -> Result<QueryHistoryPage, MetadataError> {
        let limit = filter.limit.clamp(1, 100);
        let fetch_limit = limit + 1;
        let status = filter.status.clone().map(|value| value.as_str().to_owned());
        let pattern = format!("%{}%", filter.search.as_deref().unwrap_or_default().trim());
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, sql_text, status, duration_ms, returned_rows,
             error_code, error_message, executed_at FROM query_history
             WHERE project_id = ?1
               AND (?2 IS NULL OR status = ?2)
               AND (?3 = '%%' OR sql_text LIKE ?3 OR error_code LIKE ?3 OR error_message LIKE ?3)
               AND (?4 IS NULL OR executed_at >= ?4)
               AND (?5 IS NULL OR executed_at <= ?5)
             ORDER BY executed_at DESC, id DESC LIMIT ?6 OFFSET ?7",
        )?;
        let mut entries = statement
            .query_map(
                (
                    project_id,
                    status,
                    pattern,
                    &filter.executed_from,
                    &filter.executed_to,
                    fetch_limit,
                    filter.offset,
                ),
                read_history,
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = entries.len() > limit as usize;
        entries.truncate(limit as usize);
        Ok(QueryHistoryPage {
            entries,
            offset: filter.offset,
            next_offset: has_more.then_some(filter.offset.saturating_add(limit)),
        })
    }

    pub fn list_history(
        &self,
        project_id: &str,
        status: Option<ExecutionStatus>,
        limit: u32,
    ) -> Result<Vec<QueryHistoryEntry>, MetadataError> {
        Ok(self
            .list_history_page(
                project_id,
                &QueryHistoryFilter {
                    status,
                    search: None,
                    executed_from: None,
                    executed_to: None,
                    offset: 0,
                    limit,
                },
            )?
            .entries)
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

fn normalize_saved_draft(draft: &SavedQueryDraft) -> Result<SavedQueryDraft, MetadataError> {
    let project_id = normalize_name(&draft.project_id, "project id is empty")?;
    let name = normalize_name(&draft.name, "saved query name is empty")?;
    let sql_text = draft.sql_text.trim().to_string();
    if sql_text.is_empty() {
        return Err(MetadataError::InvalidSavedQuery("SQL text is empty"));
    }
    let mut tags = draft
        .tags
        .iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    tags.sort_by_key(|tag| tag.to_lowercase());
    tags.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    Ok(SavedQueryDraft {
        project_id,
        folder_id: draft.folder_id.clone(),
        name,
        sql_text,
        tags,
    })
}

fn normalize_name(value: &str, message: &'static str) -> Result<String, MetadataError> {
    let value = value.trim();
    if value.is_empty() {
        Err(MetadataError::InvalidSavedQuery(message))
    } else {
        Ok(value.to_string())
    }
}

fn encode_tags(id: &str, tags: &[String]) -> Result<String, MetadataError> {
    serde_json::to_string(tags).map_err(|source| MetadataError::InvalidJson {
        key: format!("saved-query:{id}:tags"),
        source,
    })
}

fn saved_name_exists(
    connection: &rusqlite::Connection,
    project_id: &str,
    name: &str,
    except_id: Option<&str>,
) -> Result<bool, MetadataError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM saved_queries WHERE project_id = ?1
         AND name = ?2 COLLATE NOCASE AND (?3 IS NULL OR id <> ?3))",
        (project_id, name, except_id),
        |row| row.get(0),
    )?)
}

fn folder_name_exists(
    connection: &rusqlite::Connection,
    project_id: &str,
    name: &str,
    except_id: Option<&str>,
) -> Result<bool, MetadataError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM query_folders WHERE project_id = ?1
         AND name = ?2 COLLATE NOCASE AND (?3 IS NULL OR id <> ?3))",
        (project_id, name, except_id),
        |row| row.get(0),
    )?)
}

fn ensure_folder(
    connection: &rusqlite::Connection,
    project_id: &str,
    folder_id: Option<&str>,
) -> Result<(), MetadataError> {
    let Some(folder_id) = folder_id else {
        return Ok(());
    };
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM query_folders WHERE id = ?1 AND project_id = ?2)",
        (folder_id, project_id),
        |row| row.get(0),
    )?;
    if exists {
        Ok(())
    } else {
        Err(MetadataError::QueryFolderMissing)
    }
}

fn read_folder(row: &rusqlite::Row<'_>) -> rusqlite::Result<QueryFolder> {
    Ok(QueryFolder {
        id: row.get(0)?,
        project_id: row.get(1)?,
        name: row.get(2)?,
        created_at: row.get(3)?,
    })
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
    fn saved_query_crud_search_folders_and_project_isolation() {
        let (repository, database, project_id) = setup();
        let folder = repository
            .create_folder(&project_id, " Reporting ")
            .unwrap();
        assert_eq!(folder.name, "Reporting");
        let created = repository
            .create_saved(&SavedQueryDraft {
                project_id: project_id.clone(),
                folder_id: Some(folder.id.clone()),
                name: " Monthly revenue ".into(),
                sql_text: " select sum(revenue) from orders ".into(),
                tags: vec![" Finance ".into(), "finance".into(), "monthly".into()],
            })
            .unwrap();
        assert_eq!(created.name, "Monthly revenue");
        assert_eq!(created.sql_text, "select sum(revenue) from orders");
        assert_eq!(created.tags, ["Finance", "monthly"]);
        assert!(matches!(
            repository.create_saved(&SavedQueryDraft {
                project_id: project_id.clone(),
                folder_id: None,
                name: "monthly REVENUE".into(),
                sql_text: "select 2".into(),
                tags: vec![],
            }),
            Err(MetadataError::SavedQueryConflict(_))
        ));
        assert_eq!(
            repository.list_saved(&project_id, Some("finance")).unwrap(),
            vec![created.clone()]
        );

        let renamed_folder = repository
            .rename_folder(&project_id, &folder.id, "Finance")
            .unwrap();
        let updated = repository
            .update_saved(
                &created.id,
                &SavedQueryDraft {
                    project_id: project_id.clone(),
                    folder_id: Some(renamed_folder.id.clone()),
                    name: "Quarterly revenue".into(),
                    sql_text: "select quarter, sum(revenue) from orders group by quarter".into(),
                    tags: vec!["finance".into()],
                },
            )
            .unwrap();
        assert_eq!(updated.created_at, created.created_at);
        assert!(updated.updated_at >= created.updated_at);
        assert_eq!(
            repository.list_folders(&project_id).unwrap(),
            [renamed_folder]
        );

        assert!(repository.delete_folder(&project_id, &folder.id).unwrap());
        assert_eq!(
            repository.list_saved(&project_id, None).unwrap()[0].folder_id,
            None
        );

        let other_project = ProjectsRepository::new(database.clone())
            .upsert(
                "Other",
                Path::new("/data/other.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        assert!(repository
            .list_saved(&other_project.id, None)
            .unwrap()
            .is_empty());
        assert!(matches!(
            repository.update_saved(
                &created.id,
                &SavedQueryDraft {
                    project_id: other_project.id,
                    folder_id: None,
                    name: "Stolen".into(),
                    sql_text: "select 1".into(),
                    tags: vec![],
                },
            ),
            Err(MetadataError::SavedQueryMissing)
        ));

        assert!(repository.delete_saved(&project_id, &created.id).unwrap());
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

        // Replaying a terminal persistence attempt is idempotent at the
        // storage boundary; the original row remains exactly once.
        let duplicate = QueryHistoryEntry {
            id: "history-0".into(),
            project_id: project_id.clone(),
            sql_text: "SELECT changed".into(),
            status: ExecutionStatus::Failed,
            duration_ms: Some(999),
            returned_rows: None,
            error_code: Some("late.duplicate".into()),
            error_message: Some("must not overwrite the first terminal".into()),
            executed_at: "2026-01-02T00:00:00Z".into(),
        };
        repository.add_history(&duplicate).unwrap();
        let history = repository.list_history(&project_id, None, 10).unwrap();
        assert_eq!(history.len(), 3);
        let original = history
            .iter()
            .find(|entry| entry.id == "history-0")
            .unwrap();
        assert_eq!(original.status, ExecutionStatus::Succeeded);
        assert_eq!(original.sql_text, "select 1");

        let failed_page = repository
            .list_history_page(
                &project_id,
                &QueryHistoryFilter {
                    status: Some(ExecutionStatus::Failed),
                    search: Some("select".into()),
                    executed_from: Some("2026-01-01T00:00:00Z".into()),
                    executed_to: Some("2026-01-01T00:00:02Z".into()),
                    offset: 0,
                    limit: 1,
                },
            )
            .unwrap();
        assert_eq!(failed_page.entries.len(), 1);
        assert_eq!(failed_page.entries[0].status, ExecutionStatus::Failed);
        assert_eq!(failed_page.next_offset, None);

        let first_page = repository
            .list_history_page(
                &project_id,
                &QueryHistoryFilter {
                    status: None,
                    search: None,
                    executed_from: None,
                    executed_to: None,
                    offset: 0,
                    limit: 2,
                },
            )
            .unwrap();
        assert_eq!(first_page.entries.len(), 2);
        assert_eq!(first_page.entries[0].id, "history-2");
        assert_eq!(first_page.next_offset, Some(2));
        let second_page = repository
            .list_history_page(
                &project_id,
                &QueryHistoryFilter {
                    offset: first_page.next_offset.unwrap(),
                    limit: 2,
                    status: None,
                    search: None,
                    executed_from: None,
                    executed_to: None,
                },
            )
            .unwrap();
        assert_eq!(second_page.entries.len(), 1);
        assert_eq!(second_page.entries[0].id, "history-0");
        assert_eq!(second_page.next_offset, None);

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

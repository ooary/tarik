use rusqlite::TransactionBehavior;
use serde::{Deserialize, Serialize};

use super::{MetadataDb, MetadataError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryTabSnapshot {
    pub id: String,
    pub title: String,
    pub sql_text: String,
    pub position: u32,
    pub is_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuerySessionSnapshot {
    pub id: String,
    pub project_id: String,
    pub tabs: Vec<QueryTabSnapshot>,
}

#[derive(Clone)]
pub struct SessionsRepository {
    database: MetadataDb,
}

impl SessionsRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn save_snapshot(&self, snapshot: &QuerySessionSnapshot) -> Result<(), MetadataError> {
        validate(snapshot)?;
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute(
            "INSERT INTO query_sessions(id, project_id, created_at, updated_at)
             VALUES (?1, ?2, datetime('now'), datetime('now'))
             ON CONFLICT(id) DO UPDATE SET project_id = excluded.project_id, updated_at = excluded.updated_at",
            (&snapshot.id, &snapshot.project_id),
        )?;
        transaction.execute(
            "DELETE FROM query_tabs WHERE session_id = ?1",
            [&snapshot.id],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO query_tabs(id, session_id, title, sql_text, tab_position, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), datetime('now'))",
            )?;
            for tab in &snapshot.tabs {
                insert.execute((
                    &tab.id,
                    &snapshot.id,
                    &tab.title,
                    &tab.sql_text,
                    tab.position,
                    tab.is_active,
                ))?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn load(&self, session_id: &str) -> Result<Option<QuerySessionSnapshot>, MetadataError> {
        let connection = self.database.connection()?;
        let project_id = connection.query_row(
            "SELECT project_id FROM query_sessions WHERE id = ?1",
            [session_id],
            |row| row.get::<_, String>(0),
        );
        let project_id = match project_id {
            Ok(project_id) => project_id,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let mut statement = connection.prepare(
            "SELECT id, title, sql_text, tab_position, is_active
             FROM query_tabs WHERE session_id = ?1 ORDER BY tab_position",
        )?;
        let tabs = statement
            .query_map([session_id], |row| {
                Ok(QueryTabSnapshot {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    sql_text: row.get(2)?,
                    position: row.get(3)?,
                    is_active: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(QuerySessionSnapshot {
            id: session_id.to_owned(),
            project_id,
            tabs,
        }))
    }
}

fn validate(snapshot: &QuerySessionSnapshot) -> Result<(), MetadataError> {
    if snapshot.tabs.is_empty() {
        return Err(MetadataError::InvalidSession(
            "a session requires at least one tab",
        ));
    }
    if snapshot.tabs.iter().filter(|tab| tab.is_active).count() != 1 {
        return Err(MetadataError::InvalidSession(
            "a session requires exactly one active tab",
        ));
    }
    for (expected, tab) in snapshot.tabs.iter().enumerate() {
        if tab.position as usize != expected {
            return Err(MetadataError::InvalidSession(
                "tab positions must be contiguous and ordered",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::metadata::projects::ProjectsRepository;

    use super::*;

    fn session(project_id: String) -> QuerySessionSnapshot {
        QuerySessionSnapshot {
            id: "session-1".into(),
            project_id,
            tabs: vec![
                QueryTabSnapshot {
                    id: "tab-a".into(),
                    title: "First".into(),
                    sql_text: "select 1".into(),
                    position: 0,
                    is_active: false,
                },
                QueryTabSnapshot {
                    id: "tab-b".into(),
                    title: "Second".into(),
                    sql_text: "select 2".into(),
                    position: 1,
                    is_active: true,
                },
            ],
        }
    }

    #[test]
    fn snapshot_round_trip_preserves_order_and_active_tab() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert("Retail", Path::new("/data/retail.duckdb"))
            .unwrap();
        let repository = SessionsRepository::new(database);
        let expected = session(project.id);

        repository.save_snapshot(&expected).unwrap();

        assert_eq!(repository.load("session-1").unwrap(), Some(expected));
    }

    #[test]
    fn replacement_is_atomic_when_new_tabs_violate_database_constraint() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert("Retail", Path::new("/data/retail.duckdb"))
            .unwrap();
        let repository = SessionsRepository::new(database.clone());
        let original = session(project.id);
        repository.save_snapshot(&original).unwrap();

        let mut invalid = original.clone();
        invalid.tabs[1].id = invalid.tabs[0].id.clone();
        assert!(repository.save_snapshot(&invalid).is_err());

        assert_eq!(repository.load("session-1").unwrap(), Some(original));
    }

    #[test]
    fn invalid_active_state_is_rejected_before_write() {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert("Retail", Path::new("/data/retail.duckdb"))
            .unwrap();
        let repository = SessionsRepository::new(database);
        let mut invalid = session(project.id);
        invalid.tabs[0].is_active = true;

        assert!(matches!(
            repository.save_snapshot(&invalid),
            Err(MetadataError::InvalidSession(_))
        ));
    }
}

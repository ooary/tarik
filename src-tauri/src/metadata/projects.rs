use std::path::Path;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{MetadataDb, MetadataError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub id: String,
    pub name: String,
    pub duckdb_path: String,
    pub created_at: String,
    pub last_opened_at: String,
}

#[derive(Clone)]
pub struct ProjectsRepository {
    database: MetadataDb,
}

impl ProjectsRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn upsert(&self, name: &str, path: &Path) -> Result<RecentProject, MetadataError> {
        let path = path.to_string_lossy().into_owned();
        let id = Uuid::new_v4().to_string();
        let connection = self.database.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO projects(id, name, duckdb_path, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, datetime('now'), datetime('now'))
             ON CONFLICT(duckdb_path) DO UPDATE SET name = excluded.name, last_opened_at = excluded.last_opened_at",
            (&id, name, &path),
        )?;
        let project = find_by_path(&transaction, &path)?.ok_or(MetadataError::Invariant(
            "project upsert did not return a project",
        ))?;
        transaction.commit()?;
        Ok(project)
    }

    pub fn touch(&self, id: &str) -> Result<bool, MetadataError> {
        let connection = self.database.connection()?;
        Ok(connection.execute(
            "UPDATE projects SET last_opened_at = datetime('now') WHERE id = ?1",
            [id],
        )? > 0)
    }

    pub fn find(&self, id: &str) -> Result<Option<RecentProject>, MetadataError> {
        let connection = self.database.connection()?;
        Ok(connection
            .query_row(
                "SELECT id, name, duckdb_path, created_at, last_opened_at FROM projects WHERE id = ?1",
                [id],
                read_project,
            )
            .optional()?)
    }

    pub fn list(&self) -> Result<Vec<RecentProject>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, duckdb_path, created_at, last_opened_at
             FROM projects ORDER BY last_opened_at DESC, name COLLATE NOCASE ASC",
        )?;
        let projects = statement
            .query_map([], read_project)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(projects)
    }

    pub fn remove(&self, id: &str) -> Result<bool, MetadataError> {
        let connection = self.database.connection()?;
        Ok(connection.execute("DELETE FROM projects WHERE id = ?1", [id])? > 0)
    }
}

fn find_by_path(
    connection: &rusqlite::Connection,
    path: &str,
) -> Result<Option<RecentProject>, MetadataError> {
    Ok(connection
        .query_row(
            "SELECT id, name, duckdb_path, created_at, last_opened_at FROM projects WHERE duckdb_path = ?1",
            [path],
            read_project,
        )
        .optional()?)
}

fn read_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecentProject> {
    Ok(RecentProject {
        id: row.get(0)?,
        name: row.get(1)?,
        duckdb_path: row.get(2)?,
        created_at: row.get(3)?,
        last_opened_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, thread, time::Duration};

    use super::*;

    #[test]
    fn upsert_keeps_stable_identity_for_path() {
        let database = MetadataDb::open_in_memory().unwrap();
        let repository = ProjectsRepository::new(database);
        let path = PathBuf::from("/data/retail.duckdb");

        let first = repository.upsert("Retail", &path).unwrap();
        let second = repository.upsert("Retail analysis", &path).unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(second.name, "Retail analysis");
        assert_eq!(repository.list().unwrap().len(), 1);
    }

    #[test]
    fn list_is_recent_first_and_remove_is_explicit() {
        let database = MetadataDb::open_in_memory().unwrap();
        let repository = ProjectsRepository::new(database);
        let first = repository
            .upsert("First", Path::new("/data/first.duckdb"))
            .unwrap();
        thread::sleep(Duration::from_millis(1_100));
        let second = repository
            .upsert("Second", Path::new("/data/second.duckdb"))
            .unwrap();

        let projects = repository.list().unwrap();
        assert_eq!(projects[0].id, second.id);
        assert!(repository.touch(&first.id).unwrap());
        assert!(repository.remove(&second.id).unwrap());
        assert!(!repository.remove(&second.id).unwrap());
    }
}

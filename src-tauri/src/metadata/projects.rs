use std::path::Path;

use rusqlite::{OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{MetadataDb, MetadataError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectOwnership {
    Managed,
    External,
}

impl ProjectOwnership {
    fn as_str(self) -> &'static str {
        match self {
            Self::Managed => "managed",
            Self::External => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub id: String,
    pub name: String,
    pub duckdb_path: String,
    pub ownership: ProjectOwnership,
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

    pub fn upsert(
        &self,
        name: &str,
        path: &Path,
        ownership: ProjectOwnership,
    ) -> Result<RecentProject, MetadataError> {
        let path = path.to_string_lossy().into_owned();
        let id = Uuid::new_v4().to_string();
        let connection = self.database.connection()?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO projects(id, name, duckdb_path, ownership, created_at, last_opened_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))
             ON CONFLICT(duckdb_path) DO UPDATE SET
               name = excluded.name,
               ownership = CASE WHEN projects.ownership = 'managed' THEN 'managed' ELSE excluded.ownership END,
               last_opened_at = excluded.last_opened_at",
            (&id, name, &path, ownership.as_str()),
        )?;
        let project = find_by_path(&transaction, &path)?.ok_or(MetadataError::Invariant(
            "project upsert did not return a project",
        ))?;
        transaction.commit()?;
        Ok(project)
    }

    pub fn update_name_and_path(
        &self,
        id: &str,
        name: &str,
        path: &Path,
    ) -> Result<bool, MetadataError> {
        let path = path.to_string_lossy().into_owned();
        let mut connection = self.database.connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated = transaction.execute(
            "UPDATE projects SET name = ?2, duckdb_path = ?3, last_opened_at = datetime('now') WHERE id = ?1",
            (id, name, path),
        )? > 0;
        transaction.commit()?;
        Ok(updated)
    }

    pub fn rename_display(&self, id: &str, name: &str) -> Result<bool, MetadataError> {
        let connection = self.database.connection()?;
        Ok(connection.execute("UPDATE projects SET name = ?2 WHERE id = ?1", (id, name))? > 0)
    }

    #[cfg(test)]
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
                "SELECT id, name, duckdb_path, ownership, created_at, last_opened_at FROM projects WHERE id = ?1",
                [id],
                read_project,
            )
            .optional()?)
    }

    pub fn list(&self) -> Result<Vec<RecentProject>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, name, duckdb_path, ownership, created_at, last_opened_at
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
            "SELECT id, name, duckdb_path, ownership, created_at, last_opened_at FROM projects WHERE duckdb_path = ?1",
            [path],
            read_project,
        )
        .optional()?)
}

fn read_project(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecentProject> {
    let ownership: String = row.get(3)?;
    Ok(RecentProject {
        id: row.get(0)?,
        name: row.get(1)?,
        duckdb_path: row.get(2)?,
        ownership: match ownership.as_str() {
            "managed" => ProjectOwnership::Managed,
            "external" => ProjectOwnership::External,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        created_at: row.get(4)?,
        last_opened_at: row.get(5)?,
    })
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, thread, time::Duration};

    use super::*;

    #[test]
    fn upsert_keeps_stable_identity_and_managed_ownership() {
        let database = MetadataDb::open_in_memory().unwrap();
        let repository = ProjectsRepository::new(database);
        let path = PathBuf::from("/data/retail.duckdb");

        let first = repository
            .upsert("Retail", &path, ProjectOwnership::Managed)
            .unwrap();
        let second = repository
            .upsert("Retail analysis", &path, ProjectOwnership::External)
            .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(second.name, "Retail analysis");
        assert_eq!(second.ownership, ProjectOwnership::Managed);
        assert_eq!(repository.list().unwrap().len(), 1);
    }

    #[test]
    fn update_and_remove_are_explicit() {
        let database = MetadataDb::open_in_memory().unwrap();
        let repository = ProjectsRepository::new(database);
        let project = repository
            .upsert(
                "Retail",
                Path::new("/data/retail.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();

        assert!(repository.rename_display(&project.id, "Finance").unwrap());
        assert!(repository
            .update_name_and_path(&project.id, "Moved", Path::new("/data/moved.duckdb"))
            .unwrap());
        let updated = repository.find(&project.id).unwrap().unwrap();
        assert_eq!(updated.name, "Moved");
        assert_eq!(updated.duckdb_path, "/data/moved.duckdb");
        assert!(repository.remove(&project.id).unwrap());
    }

    #[test]
    fn list_is_recent_first() {
        let database = MetadataDb::open_in_memory().unwrap();
        let repository = ProjectsRepository::new(database);
        let first = repository
            .upsert(
                "First",
                Path::new("/data/first.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        thread::sleep(Duration::from_millis(1_100));
        let second = repository
            .upsert(
                "Second",
                Path::new("/data/second.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();

        let projects = repository.list().unwrap();
        assert_eq!(projects[0].id, second.id);
        assert!(repository.touch(&first.id).unwrap());
    }
}

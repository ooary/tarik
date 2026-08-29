pub mod commands;
mod migrations;
pub mod projects;
pub mod queries;
pub mod sessions;
pub mod settings;
pub mod sources;

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use rusqlite::{Connection, OpenFlags};

pub const LATEST_SCHEMA_VERSION: u32 = 4;

#[derive(Debug, thiserror::Error)]
pub enum MetadataError {
    #[error("could not open metadata database at {path}: {source}")]
    Open {
        path: PathBuf,
        source: rusqlite::Error,
    },
    #[error("metadata database lock is unavailable")]
    Lock,
    #[error("metadata database version {found} is newer than supported version {supported}")]
    IncompatibleVersion { found: u32, supported: u32 },
    #[error("metadata migration {version} ({name}) failed: {source}")]
    Migration {
        version: u32,
        name: &'static str,
        source: rusqlite::Error,
    },
    #[error("invalid JSON stored for setting {key}: {source}")]
    InvalidJson {
        key: String,
        source: serde_json::Error,
    },
    #[error("metadata invariant failed: {0}")]
    Invariant(&'static str),
    #[error("invalid query session: {0}")]
    InvalidSession(&'static str),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
}

#[derive(Clone)]
pub struct MetadataDb {
    connection: Arc<Mutex<Connection>>,
    path: PathBuf,
}

impl MetadataDb {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MetadataError> {
        let path = path.as_ref().to_path_buf();
        let mut connection = Connection::open_with_flags(
            &path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|source| MetadataError::Open {
            path: path.clone(),
            source,
        })?;

        configure(&connection)?;
        migrations::migrate(&mut connection)?;

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path,
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self, MetadataError> {
        let mut connection = Connection::open_in_memory()?;
        configure(&connection)?;
        migrations::migrate(&mut connection)?;
        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            path: PathBuf::from(":memory:"),
        })
    }

    #[allow(dead_code)]
    pub fn connection(&self) -> Result<MutexGuard<'_, Connection>, MetadataError> {
        self.connection.lock().map_err(|_| MetadataError::Lock)
    }

    #[allow(dead_code)]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn configure(connection: &Connection) -> Result<(), MetadataError> {
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "busy_timeout", 5_000)?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_configured_metadata_store() {
        let database = MetadataDb::open_in_memory().expect("open metadata store");
        let connection = database.connection().unwrap();

        let foreign_keys: i64 = connection
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();

        assert_eq!(foreign_keys, 1);
        assert_eq!(version, LATEST_SCHEMA_VERSION);
        assert_eq!(database.path(), Path::new(":memory:"));
    }

    #[test]
    fn reports_invalid_parent_path() {
        let error = MetadataDb::open("/definitely/missing/tarik.sqlite")
            .err()
            .expect("invalid parent should fail");

        assert!(matches!(error, MetadataError::Open { .. }));
    }
}

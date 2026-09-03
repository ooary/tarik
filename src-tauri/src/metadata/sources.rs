use serde::{Deserialize, Serialize};

use super::{MetadataDb, MetadataError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    DuckdbTable,
    LinkedParquet,
    LinkedCsv,
}

impl SourceKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::DuckdbTable => "duckdb_table",
            Self::LinkedParquet => "linked_parquet",
            Self::LinkedCsv => "linked_csv",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Ready,
    Missing,
    InvalidSchema,
}

impl SourceState {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Missing => "missing",
            Self::InvalidSchema => "invalid_schema",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub id: String,
    pub project_id: String,
    pub display_name: String,
    pub kind: SourceKind,
    pub state: SourceState,
    pub source_path: Option<String>,
    pub duckdb_name: String,
    pub options: serde_json::Value,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExportStatus {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPartSummary {
    pub part_number: u64,
    pub path: String,
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportHistoryRecord {
    pub id: String,
    pub project_id: String,
    pub status: ExportStatus,
    pub format: String,
    pub output_directory: String,
    pub base_name: String,
    pub rows_per_part: u64,
    pub sql_text: String,
    pub options: serde_json::Value,
    pub duration_ms: Option<u64>,
    pub rows_written: u64,
    pub files_written: u64,
    pub bytes_written: u64,
    pub completed_parts: Vec<ExportPartSummary>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone)]
pub struct SourcesRepository {
    database: MetadataDb,
}

impl SourcesRepository {
    pub fn new(database: MetadataDb) -> Self {
        Self { database }
    }

    pub fn upsert_source(&self, source: &SourceRecord) -> Result<(), MetadataError> {
        let options = serde_json::to_string(&source.options).map_err(|source_error| {
            MetadataError::InvalidJson {
                key: format!("source:{}:options", source.id),
                source: source_error,
            }
        })?;
        self.database.connection()?.execute(
            "INSERT INTO sources(id, project_id, display_name, kind, state, source_path, duckdb_name,
             options_json, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, state = excluded.state,
             source_path = excluded.source_path, duckdb_name = excluded.duckdb_name,
             options_json = excluded.options_json, updated_at = excluded.updated_at",
            (
                &source.id,
                &source.project_id,
                &source.display_name,
                source.kind.as_str(),
                source.state.as_str(),
                &source.source_path,
                &source.duckdb_name,
                options,
                &source.created_at,
                &source.updated_at,
            ),
        )?;
        Ok(())
    }

    pub fn set_source_state(&self, id: &str, state: SourceState) -> Result<bool, MetadataError> {
        Ok(self.database.connection()?.execute(
            "UPDATE sources SET state = ?2, updated_at = datetime('now') WHERE id = ?1",
            (id, state.as_str()),
        )? > 0)
    }

    pub fn list_sources(&self, project_id: &str) -> Result<Vec<SourceRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, project_id, display_name, kind, state, source_path, duckdb_name,
             options_json, created_at, updated_at FROM sources
             WHERE project_id = ?1 ORDER BY display_name COLLATE NOCASE",
        )?;
        let sources = statement
            .query_map([project_id], read_source)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sources)
    }

    pub fn remove_source(&self, id: &str) -> Result<bool, MetadataError> {
        Ok(self
            .database
            .connection()?
            .execute("DELETE FROM sources WHERE id = ?1", [id])?
            > 0)
    }

    pub fn remove_by_object_name(
        &self,
        project_id: &str,
        duckdb_name: &str,
    ) -> Result<usize, MetadataError> {
        Ok(self.database.connection()?.execute(
            "DELETE FROM sources WHERE project_id = ?1 AND duckdb_name = ?2",
            (project_id, duckdb_name),
        )?)
    }

    pub fn get_source(&self, id: &str) -> Result<Option<SourceRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let result = connection.query_row(
            "SELECT id, project_id, display_name, kind, state, source_path, duckdb_name,
             options_json, created_at, updated_at FROM sources WHERE id = ?1",
            [id],
            read_source,
        );
        match result {
            Ok(source) => Ok(Some(source)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    pub fn add_terminal_export(&self, export: &ExportHistoryRecord) -> Result<bool, MetadataError> {
        let parts = serde_json::to_string(&export.completed_parts).map_err(|source| {
            MetadataError::InvalidJson {
                key: format!("export:{}:parts", export.id),
                source,
            }
        })?;
        let options = serde_json::to_string(&export.options).map_err(|source| {
            MetadataError::InvalidJson {
                key: format!("export:{}:options", export.id),
                source,
            }
        })?;
        self.database.connection()?.execute(
            "INSERT INTO export_history(id, project_id, status, format, output_directory, base_name,
             rows_per_part, completed_parts_json, error_message, created_at, updated_at, sql_text,
             options_json, duration_ms, rows_written, files_written, bytes_written, error_code)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
             ON CONFLICT(id) DO NOTHING",
            rusqlite::params![
                &export.id,
                &export.project_id,
                export.status.as_str(),
                &export.format,
                &export.output_directory,
                &export.base_name,
                export.rows_per_part,
                parts,
                &export.error_message,
                &export.created_at,
                &export.updated_at,
                &export.sql_text,
                options,
                export.duration_ms,
                export.rows_written,
                export.files_written,
                export.bytes_written,
                &export.error_code,
            ],
        )
        .map(|inserted| inserted > 0)
        .map_err(Into::into)
    }

    #[cfg(test)]
    pub fn get_export(&self, id: &str) -> Result<Option<ExportHistoryRecord>, MetadataError> {
        let connection = self.database.connection()?;
        let result = connection.query_row(
            "SELECT id, project_id, status, format, output_directory, base_name, rows_per_part,
             completed_parts_json, error_message, created_at, updated_at, sql_text, options_json,
             duration_ms, rows_written, files_written, bytes_written, error_code
             FROM export_history WHERE id = ?1",
            [id],
            read_export,
        );
        match result {
            Ok(export) => Ok(Some(export)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }
}

fn read_source(row: &rusqlite::Row<'_>) -> rusqlite::Result<SourceRecord> {
    let kind: String = row.get(3)?;
    let state: String = row.get(4)?;
    let options_json: String = row.get(7)?;
    Ok(SourceRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        display_name: row.get(2)?,
        kind: match kind.as_str() {
            "duckdb_table" => SourceKind::DuckdbTable,
            "linked_parquet" => SourceKind::LinkedParquet,
            "linked_csv" => SourceKind::LinkedCsv,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        state: match state.as_str() {
            "ready" => SourceState::Ready,
            "missing" => SourceState::Missing,
            "invalid_schema" => SourceState::InvalidSchema,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        source_path: row.get(5)?,
        duckdb_name: row.get(6)?,
        options: serde_json::from_str(&options_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

#[cfg(test)]
fn read_export(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExportHistoryRecord> {
    let status: String = row.get(2)?;
    let parts: String = row.get(7)?;
    let options: String = row.get(12)?;
    Ok(ExportHistoryRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        status: match status.as_str() {
            "queued" => ExportStatus::Queued,
            "running" => ExportStatus::Running,
            "succeeded" => ExportStatus::Succeeded,
            "failed" => ExportStatus::Failed,
            "cancelled" => ExportStatus::Cancelled,
            _ => return Err(rusqlite::Error::InvalidQuery),
        },
        format: row.get(3)?,
        output_directory: row.get(4)?,
        base_name: row.get(5)?,
        rows_per_part: row.get(6)?,
        completed_parts: serde_json::from_str(&parts).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                7,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        error_message: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
        sql_text: row.get(11)?,
        options: serde_json::from_str(&options).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        duration_ms: row.get(13)?,
        rows_written: row.get(14)?,
        files_written: row.get(15)?,
        bytes_written: row.get(16)?,
        error_code: row.get(17)?,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::metadata::projects::{ProjectOwnership, ProjectsRepository};

    use super::*;

    fn setup() -> (SourcesRepository, String) {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Retail",
                Path::new("/data/retail.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        (SourcesRepository::new(database), project.id)
    }

    #[test]
    fn source_state_transitions_keep_only_metadata() {
        let (repository, project_id) = setup();
        let source = SourceRecord {
            id: "source-1".into(),
            project_id,
            display_name: "orders".into(),
            kind: SourceKind::LinkedParquet,
            state: SourceState::Ready,
            source_path: Some("/data/orders.parquet".into()),
            duckdb_name: "orders".into(),
            options: serde_json::json!({"glob": false}),
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        repository.upsert_source(&source).unwrap();
        repository
            .set_source_state("source-1", SourceState::Missing)
            .unwrap();

        let loaded = repository.get_source("source-1").unwrap().unwrap();
        assert_eq!(loaded.state, SourceState::Missing);
        assert_eq!(loaded.source_path.as_deref(), Some("/data/orders.parquet"));
        assert_eq!(
            repository
                .remove_by_object_name(&loaded.project_id, "orders")
                .unwrap(),
            1
        );
        assert!(repository.get_source("source-1").unwrap().is_none());
    }

    #[test]
    fn export_history_tracks_completed_parts() {
        let (repository, project_id) = setup();
        let export = ExportHistoryRecord {
            id: "export-1".into(),
            project_id,
            status: ExportStatus::Succeeded,
            format: "parquet".into(),
            output_directory: "/exports".into(),
            base_name: "orders".into(),
            rows_per_part: 1_000_000,
            sql_text: "SELECT * FROM orders".into(),
            options: serde_json::json!({ "compression": "snappy" }),
            duration_ms: Some(125),
            rows_written: 12,
            files_written: 1,
            bytes_written: 400,
            completed_parts: vec![ExportPartSummary {
                part_number: 1,
                path: "/exports/orders-part-00001.parquet".into(),
                rows: 12,
                bytes: 400,
            }],
            error_code: None,
            error_message: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:01Z".into(),
        };
        assert!(repository.add_terminal_export(&export).unwrap());
        let mut replay = export.clone();
        replay.status = ExportStatus::Failed;
        replay.error_code = Some("late".into());
        replay.error_message = Some("must not replace terminal state".into());
        assert!(!repository.add_terminal_export(&replay).unwrap());

        assert_eq!(repository.get_export("export-1").unwrap(), Some(export));
    }
}

//! Versioned engine protocol types shared by the Tarik desktop app and engine
//! adapters. This crate intentionally depends only on serde and uuid so the
//! desktop build never pulls in database or Arrow crates.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub schemas: bool,
    pub views: bool,
    pub cancel_query: bool,
    pub explain: bool,
    pub profile: bool,
    pub link_parquet: bool,
    pub import_csv: bool,
    pub import_parquet: bool,
    pub transactions: bool,
    pub bounded_pages: bool,
    pub export_csv: bool,
    pub export_parquet: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            schemas: true,
            views: true,
            cancel_query: true,
            explain: true,
            profile: true,
            link_parquet: false,
            import_csv: false,
            import_parquet: false,
            transactions: true,
            bounded_pages: true,
            export_csv: true,
            export_parquet: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub engine_id: String,
    pub engine_name: String,
    pub engine_version: String,
    pub protocol_version: u32,
    pub capabilities: Capabilities,
    /// Engine-specific free-form metadata.
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLocator {
    pub engine_id: String,
    /// Engine-specific locator payload, e.g. `{ "path": "/data/retail.duckdb" }`.
    pub payload: serde_json::Map<String, serde_json::Value>,
}

impl ProjectLocator {
    pub fn duckdb_path(locator: &ProjectLocator) -> Option<&str> {
        if locator.engine_id != "duckdb" {
            return None;
        }
        locator
            .payload
            .get("path")
            .and_then(serde_json::Value::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    /// Generic logical type used for frontend formatting, e.g. `decimal`.
    pub logical_type: String,
    /// Engine-native type, e.g. `DECIMAL(18,2)`.
    pub native_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultInfo {
    pub result_id: String,
    pub columns: Vec<ColumnInfo>,
    pub row_count: u64,
    pub row_count_exact: bool,
    /// Location of the bounded page artifact directory.
    pub page_dir: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    pub offset: u64,
    pub rows: u64,
    /// Path to an Arrow IPC or Parquet page artifact.
    pub artifact: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogObject {
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub estimated_row_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogColumn {
    pub database: String,
    pub schema: String,
    pub object: String,
    pub name: String,
    pub data_type: String,
    pub position: u32,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub objects: Vec<CatalogObject>,
    pub columns: Vec<CatalogColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    DuckdbTable,
    LinkedParquet,
    LinkedCsv,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Ready,
    Missing,
    InvalidSchema,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInspection {
    pub path: String,
    pub format: String,
    pub suggested_name: String,
    pub file_size_bytes: u64,
    pub row_count: u64,
    pub row_count_exact: bool,
    pub columns: Vec<SourceColumn>,
    pub preview_rows: Vec<Vec<serde_json::Value>>,
    pub csv_options: Option<CsvOptions>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvOptions {
    pub delimiter: String,
    pub has_header: bool,
    pub null_value: Option<String>,
    pub all_varchar: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: ",".into(),
            has_header: true,
            null_value: None,
            all_varchar: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnOverride {
    pub column: String,
    pub data_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub table_name: String,
    pub csv: Option<CsvOptions>,
    pub column_overrides: Vec<ColumnOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub id: String,
    pub project_id: String,
    pub display_name: String,
    pub kind: SourceKind,
    pub state: SourceState,
    pub source_path: Option<String>,
    pub duckdb_name: String,
    pub options: serde_json::Map<String, serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// Lifecycle of one query execution submitted to an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Observed status of one engine execution. The engine owns the transition
/// from queued to a single terminal state; the desktop persists history from
/// the terminal snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStatus {
    pub execution_id: String,
    pub state: ExecutionState,
    pub duration_ms: u64,
    /// Rows produced so far while running, or the final row count once
    /// terminal. `None` for statements without a row set.
    pub rows_produced: Option<u64>,
    /// Rows changed by DML statements when no result set was produced.
    pub rows_affected: Option<u64>,
    pub error: Option<ErrorEnvelope>,
    /// Published bounded result metadata once the execution succeeded with
    /// a row set.
    #[serde(default)]
    pub result: Option<ResultInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineManifest {
    pub id: String,
    pub executable: String,
    pub protocol_version: u32,
    pub display_name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    /// Optional engine-specific detail, e.g. DuckDB error position.
    #[serde(default)]
    pub details: serde_json::Map<String, serde_json::Value>,
}

impl ErrorEnvelope {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            request_id: None,
            details: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub id: String,
    pub method: String,
    /// Method parameters; schemas are defined per method.
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub id: String,
    pub ok: bool,
    pub error: Option<ErrorEnvelope>,
    /// Successful method result.
    pub result: Option<serde_json::Value>,
}

impl ResponseEnvelope {
    pub fn ok(id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            error: None,
            result: Some(result),
        }
    }

    pub fn err(id: impl Into<String>, error: ErrorEnvelope) -> Self {
        Self {
            id: id.into(),
            ok: false,
            error: Some(error),
            result: None,
        }
    }
}

/// Framed wire messages. `EngineFrame::Request` flows desktop → engine;
/// `EngineFrame::Response` flows engine → desktop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum EngineFrame {
    Request(RequestEnvelope),
    Response(ResponseEnvelope),
}

pub fn new_request_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duckdb_capabilities() -> Capabilities {
        Capabilities {
            link_parquet: true,
            import_csv: true,
            import_parquet: true,
            ..Default::default()
        }
    }

    #[test]
    fn request_response_round_trip_keeps_ids_and_unknown_fields() {
        let request = RequestEnvelope {
            id: "req-1".into(),
            method: "catalog.inspect".into(),
            params: serde_json::json!({ "sessionId": "s1", "futureField": 1 })
                .as_object()
                .unwrap()
                .clone(),
        };
        let frame = EngineFrame::Request(request);
        let wire = serde_json::to_string(&frame).unwrap();
        let decoded: EngineFrame = serde_json::from_str(&wire).unwrap();
        match decoded {
            EngineFrame::Request(decoded) => {
                assert_eq!(decoded.id, "req-1");
                assert_eq!(decoded.params["futureField"], 1);
            }
            EngineFrame::Response(_) => panic!("expected request"),
        }
    }

    #[test]
    fn duckdb_locator_extracts_path_only_for_duckdb() {
        let locator = ProjectLocator {
            engine_id: "duckdb".into(),
            payload: serde_json::json!({ "path": "/data/retail.duckdb" })
                .as_object()
                .unwrap()
                .clone(),
        };
        assert_eq!(
            ProjectLocator::duckdb_path(&locator),
            Some("/data/retail.duckdb")
        );

        let postgres = ProjectLocator {
            engine_id: "postgres".into(),
            payload: serde_json::json!({ "host": "localhost" })
                .as_object()
                .unwrap()
                .clone(),
        };
        assert_eq!(ProjectLocator::duckdb_path(&postgres), None);
    }

    #[test]
    fn capabilities_are_negotiable_per_engine() {
        let duckdb = duckdb_capabilities();
        let postgres = Capabilities::default();
        assert!(duckdb.link_parquet);
        assert!(!postgres.link_parquet);
    }

    #[test]
    fn error_envelope_carries_code_message_and_request_id() {
        let response = ResponseEnvelope::err(
            "req-9",
            ErrorEnvelope {
                request_id: Some("req-9".into()),
                ..ErrorEnvelope::new("sql.parse", "near line 2: syntax error")
            },
        );
        assert!(!response.ok);
        assert_eq!(response.error.as_ref().unwrap().code, "sql.parse");
        assert_eq!(response.id, "req-9");
    }
}

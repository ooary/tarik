//! Private, versioned protocol between the Tarik desktop and `tarik-mcp`.
//!
//! This is intentionally not the MCP protocol. It is the narrow authenticated
//! bridge contract behind the public MCP tools and never mirrors Tauri or
//! engine methods dynamically.

use serde::{Deserialize, Serialize};

pub const BRIDGE_PROTOCOL_VERSION: u32 = 4;
pub const MAX_BRIDGE_MESSAGE_BYTES: usize = 1024 * 1024;
pub const MAX_CLIENT_LABEL_BYTES: usize = 80;
pub const MAX_PROFILE_ID_BYTES: usize = 128;
pub const CHALLENGE_BYTES: usize = 32;
pub const PROOF_BYTES: usize = 32;
pub const MAX_DISCOVERY_PAGE_ITEMS: u32 = 100;
pub const MAX_DISCOVERY_SEARCH_BYTES: usize = 128;
pub const MAX_DISCOVERY_RESPONSE_BYTES: usize = 256 * 1024;
pub const MAX_AGENT_RESULT_ROWS: u64 = 5_000;
pub const MAX_AGENT_RESULT_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_AGENT_RESULTS_PER_PROFILE: u32 = 8;
pub const MAX_AGENT_QUERIES_PER_PROFILE: u32 = 4;
pub const MAX_AGENT_QUERIES_GLOBAL: u32 = 16;
pub const MAX_AGENT_RESULTS_GLOBAL: u32 = 32;
pub const MAX_AGENT_CACHE_BYTES_PER_PROFILE: u64 = 128 * 1024 * 1024;
pub const MAX_AGENT_CACHE_BYTES_GLOBAL: u64 = 512 * 1024 * 1024;
pub const MAX_AGENT_PAGE_ROWS: u32 = 500;
pub const MAX_AGENT_PAGE_RESPONSE_BYTES: usize = 1024 * 1024;
pub const MAX_AGENT_EXPORT_BASE_NAME_BYTES: usize = 64;
pub const MAX_AGENT_EXPORT_ROWS_PER_PART: u64 = 1_000_000;
pub const MAX_AGENT_EXPORT_MANIFEST_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRequest {
    pub id: String,
    pub protocol_version: u32,
    #[serde(flatten)]
    pub action: BridgeAction,
}

impl BridgeRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.protocol_version != BRIDGE_PROTOCOL_VERSION {
            return Err(ProtocolError::UnsupportedVersion);
        }
        if self.id.trim().is_empty() || self.id.len() > 128 {
            return Err(ProtocolError::InvalidRequestId);
        }
        match &self.action {
            BridgeAction::Hello {
                profile_id,
                label,
                pairing_key,
            } => {
                if profile_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProfileId);
                }
                if label.trim().is_empty() || label.len() > MAX_CLIENT_LABEL_BYTES {
                    return Err(ProtocolError::InvalidClientLabel);
                }
                match (profile_id.is_empty(), pairing_key) {
                    (true, Some(key)) if valid_proof_hex(key) => {}
                    (true, _) => return Err(ProtocolError::PairingKeyRequired),
                    (false, None) => {}
                    (false, Some(_)) => return Err(ProtocolError::UnexpectedPairingKey),
                }
            }
            BridgeAction::Authenticate {
                connection_id,
                profile_id,
                proof,
            } => {
                if connection_id.trim().is_empty() {
                    return Err(ProtocolError::InvalidConnectionId);
                }
                if profile_id.trim().is_empty() || profile_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProfileId);
                }
                if !valid_proof_hex(proof) {
                    return Err(ProtocolError::InvalidProof);
                }
            }
            BridgeAction::Status { connection_id }
            | BridgeAction::ListProjects { connection_id }
            | BridgeAction::ListActive { connection_id }
            | BridgeAction::QueryStatus { connection_id, .. }
            | BridgeAction::CancelQuery { connection_id, .. }
            | BridgeAction::ReleaseResult { connection_id, .. }
            | BridgeAction::ProposeSql { connection_id, .. }
            | BridgeAction::ApprovalStatus { connection_id, .. }
            | BridgeAction::ExecuteApproved { connection_id, .. }
            | BridgeAction::ProfileStatus { connection_id, .. }
            | BridgeAction::CancelProfile { connection_id, .. }
            | BridgeAction::ExplainSql { connection_id, .. }
            | BridgeAction::Disconnect { connection_id } => {
                validate_connection_id(connection_id)?;
            }
            BridgeAction::ListCatalog {
                connection_id,
                project_id,
                search,
                cursor,
                limit,
            } => {
                validate_discovery_request(connection_id, project_id, cursor.as_deref(), *limit)?;
                if search
                    .as_ref()
                    .is_some_and(|value| value.len() > MAX_DISCOVERY_SEARCH_BYTES)
                {
                    return Err(ProtocolError::InvalidSearch);
                }
            }
            BridgeAction::DescribeRelation {
                connection_id,
                project_id,
                database,
                schema,
                name,
                cursor,
                limit,
            } => {
                validate_discovery_request(connection_id, project_id, cursor.as_deref(), *limit)?;
                if [database, schema, name]
                    .iter()
                    .any(|value| value.trim().is_empty() || value.len() > 1024)
                {
                    return Err(ProtocolError::InvalidRelationIdentity);
                }
            }
            BridgeAction::ClassifySql {
                connection_id,
                project_id,
                sql,
            } => {
                validate_connection_id(connection_id)?;
                if project_id.trim().is_empty() || project_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProjectId);
                }
                if sql.trim().is_empty() || sql.len() > 256 * 1024 {
                    return Err(ProtocolError::InvalidSql);
                }
            }
            BridgeAction::StartQuery {
                connection_id,
                snapshot_id,
            } => {
                validate_connection_id(connection_id)?;
                if snapshot_id.trim().is_empty() || snapshot_id.len() > 128 {
                    return Err(ProtocolError::InvalidSnapshotId);
                }
            }
            BridgeAction::GetResultPage {
                connection_id,
                result_id,
                max_rows,
                ..
            } => {
                validate_connection_id(connection_id)?;
                if result_id.trim().is_empty() || result_id.len() > 128 {
                    return Err(ProtocolError::InvalidResultId);
                }
                if *max_rows == 0 || *max_rows > MAX_AGENT_PAGE_ROWS {
                    return Err(ProtocolError::InvalidPageLimit);
                }
            }
            BridgeAction::StartProfile {
                connection_id,
                request,
            } => {
                validate_connection_id(connection_id)?;
                request
                    .validate()
                    .map_err(|_| ProtocolError::InvalidProfileRequest)?;
            }
            BridgeAction::ListExportDestinations {
                connection_id,
                project_id,
            } => {
                validate_connection_id(connection_id)?;
                if project_id.trim().is_empty() || project_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProjectId);
                }
            }
            BridgeAction::ProposeExport {
                connection_id,
                intent,
            } => {
                validate_connection_id(connection_id)?;
                intent.validate()?;
            }
            BridgeAction::ExportStatus {
                connection_id,
                export_id,
            }
            | BridgeAction::ExportCancel {
                connection_id,
                export_id,
            }
            | BridgeAction::ExportRelease {
                connection_id,
                export_id,
            } => {
                validate_connection_id(connection_id)?;
                if export_id.trim().is_empty() || export_id.len() > 128 {
                    return Err(ProtocolError::InvalidExportId);
                }
            }
            BridgeAction::ListQuality {
                connection_id,
                project_id,
                offset,
                limit,
            }
            | BridgeAction::ListSavedQueries {
                connection_id,
                project_id,
                offset,
                limit,
            } => {
                validate_connection_id(connection_id)?;
                if project_id.trim().is_empty() || project_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProjectId);
                }
                if *limit == 0 || *limit > 100 || *offset > 5_000 {
                    return Err(ProtocolError::InvalidPageLimit);
                }
            }
            BridgeAction::ListQualityRuns {
                connection_id,
                project_id,
                offset,
                limit,
                ..
            } => {
                validate_connection_id(connection_id)?;
                if project_id.trim().is_empty() || project_id.len() > MAX_PROFILE_ID_BYTES {
                    return Err(ProtocolError::InvalidProjectId);
                }
                if *limit == 0 || *limit > 100 || *offset > 5_000 {
                    return Err(ProtocolError::InvalidPageLimit);
                }
            }
            BridgeAction::Ping => {}
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum BridgeAction {
    Ping,
    Hello {
        #[serde(default)]
        profile_id: String,
        label: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pairing_key: Option<String>,
    },
    Authenticate {
        connection_id: String,
        profile_id: String,
        proof: String,
    },
    Status {
        connection_id: String,
    },
    ListProjects {
        connection_id: String,
    },
    ListActive {
        connection_id: String,
    },
    ListExportDestinations {
        connection_id: String,
        project_id: String,
    },
    ProposeExport {
        connection_id: String,
        intent: AgentExportIntent,
    },
    ExportStatus {
        connection_id: String,
        export_id: String,
    },
    ExportCancel {
        connection_id: String,
        export_id: String,
    },
    ExportRelease {
        connection_id: String,
        export_id: String,
    },
    ListCatalog {
        connection_id: String,
        project_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        search: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        limit: u32,
    },
    DescribeRelation {
        connection_id: String,
        project_id: String,
        database: String,
        schema: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        limit: u32,
    },
    ClassifySql {
        connection_id: String,
        project_id: String,
        sql: String,
    },
    StartQuery {
        connection_id: String,
        snapshot_id: String,
    },
    QueryStatus {
        connection_id: String,
        execution_id: String,
    },
    CancelQuery {
        connection_id: String,
        execution_id: String,
    },
    GetResultPage {
        connection_id: String,
        result_id: String,
        offset: u64,
        max_rows: u32,
    },
    ReleaseResult {
        connection_id: String,
        result_id: String,
    },
    ProposeSql {
        connection_id: String,
        snapshot_id: String,
    },
    ApprovalStatus {
        connection_id: String,
        approval_id: String,
    },
    ExecuteApproved {
        connection_id: String,
        approval_id: String,
    },
    StartProfile {
        connection_id: String,
        request: tarik_engine_protocol::ProfileRequest,
    },
    ProfileStatus {
        connection_id: String,
        profile_id: String,
    },
    CancelProfile {
        connection_id: String,
        profile_id: String,
    },
    ExplainSql {
        connection_id: String,
        snapshot_id: String,
        actual: bool,
    },
    ListQuality {
        connection_id: String,
        project_id: String,
        offset: u32,
        limit: u32,
    },
    ListQualityRuns {
        connection_id: String,
        project_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        check_id: Option<String>,
        offset: u32,
        limit: u32,
    },
    ListSavedQueries {
        connection_id: String,
        project_id: String,
        offset: u32,
        limit: u32,
    },
    Disconnect {
        connection_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeResponse {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<BridgeError>,
}

impl BridgeResponse {
    pub fn success<T: Serialize>(id: impl Into<String>, value: T) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: serde_json::to_value(value).ok(),
            error: None,
        }
    }

    pub fn failure(
        id: impl Into<String>,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(BridgeError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResult {
    pub state: HelloState,
    pub connection_id: String,
    pub profile_id: String,
    pub challenge: String,
    pub salt: String,
    pub pairing_request_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HelloState {
    PairingRequired,
    AuthenticationRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationResult {
    pub client_profile_id: String,
    pub connection_id: String,
    pub grants: Vec<ProjectGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionStatusResult {
    pub authenticated: bool,
    pub client_profile_id: Option<String>,
    pub grants: Vec<ProjectGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGrant {
    pub project_id: String,
    pub inspect: bool,
    pub analyze: bool,
    pub modify_workspace: bool,
    pub modify_data: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantedProject {
    pub project_id: String,
    pub name: String,
    pub active: bool,
    pub grant: ProjectGrant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrantedProjectsResult {
    pub projects: Vec<GrantedProject>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Parquet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDestinationView {
    pub destination_id: String,
    pub label: String,
    pub formats: Vec<ExportFormat>,
    pub maximum_rows_per_part: u64,
    pub maximum_total_bytes: u64,
    pub create_new_only: bool,
    pub enabled: bool,
    pub ready: bool,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDestinationList {
    pub project_id: String,
    pub destinations: Vec<ExportDestinationView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCsvExportOptions {
    pub delimiter: String,
    pub include_header: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentParquetCompression {
    Uncompressed,
    Snappy,
    Gzip,
    Zstd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentParquetExportOptions {
    pub compression: AgentParquetCompression,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentExportIntent {
    pub snapshot_id: String,
    pub destination_id: String,
    pub format: ExportFormat,
    pub base_name: String,
    pub rows_per_part: u64,
    pub csv: Option<AgentCsvExportOptions>,
    pub parquet: Option<AgentParquetExportOptions>,
}

impl AgentExportIntent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.snapshot_id.trim().is_empty() || self.snapshot_id.len() > 128 {
            return Err(ProtocolError::InvalidSnapshotId);
        }
        if self.destination_id.trim().is_empty() || self.destination_id.len() > 128 {
            return Err(ProtocolError::InvalidDestinationId);
        }
        if self.base_name.is_empty()
            || self.base_name.len() > MAX_AGENT_EXPORT_BASE_NAME_BYTES
            || !self
                .base_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ProtocolError::InvalidExportBaseName);
        }
        if self.rows_per_part == 0 || self.rows_per_part > MAX_AGENT_EXPORT_ROWS_PER_PART {
            return Err(ProtocolError::InvalidExportRowsPerPart);
        }
        match self.format {
            ExportFormat::Csv => {
                let csv = self
                    .csv
                    .as_ref()
                    .ok_or(ProtocolError::InvalidExportOptions)?;
                if self.parquet.is_some() {
                    return Err(ProtocolError::InvalidExportOptions);
                }
                let delimiter = csv.delimiter.as_bytes();
                if delimiter.len() != 1
                    || !delimiter[0].is_ascii()
                    || matches!(delimiter[0], 0 | b'"' | b'\r' | b'\n')
                {
                    return Err(ProtocolError::InvalidExportOptions);
                }
            }
            ExportFormat::Parquet => {
                if self.csv.is_some() || self.parquet.is_none() {
                    return Err(ProtocolError::InvalidExportOptions);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentExportDecision {
    Delegated,
    ApprovalRequired,
    CriticalConfirmation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentExportState {
    AwaitingApproval,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    RecoveryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExportPartSummary {
    pub part_number: u64,
    pub file_name: String,
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExportView {
    pub export_id: String,
    pub project_id: String,
    pub destination_id: String,
    pub decision: AgentExportDecision,
    pub approval_id: Option<String>,
    pub state: AgentExportState,
    pub complete_query: bool,
    pub duration_ms: u64,
    pub rows_written: u64,
    pub files_written: u64,
    pub bytes_written: u64,
    pub current_part: Option<u64>,
    pub completed_parts: Vec<AgentExportPartSummary>,
    pub error: Option<tarik_engine_protocol::ErrorEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExportReleaseResult {
    pub export_id: String,
    pub released: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRelationSummary {
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub estimated_row_count: Option<u64>,
    pub column_count: u32,
    pub registered_source_id: Option<String>,
    pub source_kind: Option<String>,
    pub source_state: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPageResult {
    pub project_id: String,
    pub catalog_revision: String,
    pub relations: Vec<CatalogRelationSummary>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationColumn {
    pub name: String,
    pub data_type: String,
    pub position: u32,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationDescriptionResult {
    pub project_id: String,
    pub catalog_revision: String,
    pub relation: CatalogRelationSummary,
    pub columns: Vec<RelationColumn>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlSnapshotResult {
    pub snapshot_id: String,
    pub project_id: String,
    pub classification: tarik_engine_protocol::AgentSqlClassification,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentExecutionResult {
    pub execution_id: String,
    pub project_id: String,
    pub state: String,
    pub duration_ms: u64,
    pub rows_produced: Option<u64>,
    pub rows_affected: Option<u64>,
    pub result_id: Option<String>,
    pub row_total: Option<u64>,
    pub row_total_exact: Option<bool>,
    pub browse_limit_reached: bool,
    pub complete_result_available: bool,
    pub limit_reason: Option<String>,
    pub browse_row_cap: u64,
    pub cache_bytes: Option<u64>,
    pub slot_held: bool,
    pub slot_available: bool,
    pub cancellation_requested: bool,
    pub cleanup_pending: bool,
    pub error: Option<tarik_engine_protocol::ErrorEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAnalysisLimits {
    pub browse_row_cap: u64,
    pub maximum_result_bytes: u64,
    pub retained_result_limit: u32,
    pub outstanding_query_limit: u32,
    pub profile_cache_bytes: u64,
    pub global_cache_bytes: u64,
    pub queue_deadline_seconds: u64,
    pub execution_deadline_seconds: u64,
}

impl Default for AgentAnalysisLimits {
    fn default() -> Self {
        Self {
            browse_row_cap: MAX_AGENT_RESULT_ROWS,
            maximum_result_bytes: MAX_AGENT_RESULT_BYTES,
            retained_result_limit: MAX_AGENT_RESULTS_PER_PROFILE,
            outstanding_query_limit: MAX_AGENT_QUERIES_PER_PROFILE,
            profile_cache_bytes: MAX_AGENT_CACHE_BYTES_PER_PROFILE,
            global_cache_bytes: MAX_AGENT_CACHE_BYTES_GLOBAL,
            queue_deadline_seconds: 60,
            execution_deadline_seconds: 60,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActiveQuery {
    pub execution_id: String,
    pub project_id: String,
    pub origin_connection_id: String,
    pub state: String,
    pub queue_wait_ms: u64,
    pub running_ms: u64,
    pub result_id: Option<String>,
    pub rows: Option<u64>,
    pub row_total_exact: Option<bool>,
    pub browse_limit_reached: bool,
    pub cache_bytes: Option<u64>,
    pub slot_held: bool,
    pub cancellation_requested: bool,
    pub cleanup_pending: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActiveConnection {
    pub connection_id: String,
    pub authenticated: bool,
    pub connected_for_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActiveList {
    pub client_profile_id: String,
    pub limits: AgentAnalysisLimits,
    pub connections: Vec<AgentActiveConnection>,
    pub queries: Vec<AgentActiveQuery>,
    pub retained_result_count: u32,
    pub retained_cache_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResultPage {
    pub result_id: String,
    pub offset: u64,
    pub row_total: u64,
    pub row_total_exact: bool,
    pub columns: serde_json::Value,
    pub rows: serde_json::Value,
    pub truncated_cells: serde_json::Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalState {
    Pending,
    Approved,
    Denied,
    Expired,
    Used,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalResult {
    pub approval_id: String,
    pub project_id: String,
    pub state: ApprovalState,
    pub decision: tarik_engine_protocol::AgentSqlDecision,
    pub reason_code: String,
    pub affected_objects: Vec<String>,
    pub has_top_level_filter: Option<bool>,
    pub snapshot_hash: String,
    pub expires_in_seconds: u64,
}

impl ProjectGrant {
    pub fn has_any(&self) -> bool {
        self.inspect || self.analyze || self.modify_workspace || self.modify_data
    }
}

fn validate_connection_id(value: &str) -> Result<(), ProtocolError> {
    if value.trim().is_empty() || value.len() > 128 {
        Err(ProtocolError::InvalidConnectionId)
    } else {
        Ok(())
    }
}

fn validate_discovery_request(
    connection_id: &str,
    project_id: &str,
    cursor: Option<&str>,
    limit: u32,
) -> Result<(), ProtocolError> {
    validate_connection_id(connection_id)?;
    if project_id.trim().is_empty() || project_id.len() > MAX_PROFILE_ID_BYTES {
        return Err(ProtocolError::InvalidProjectId);
    }
    if limit == 0 || limit > MAX_DISCOVERY_PAGE_ITEMS {
        return Err(ProtocolError::InvalidPageLimit);
    }
    if cursor.is_some_and(|value| value.is_empty() || value.len() > 1024) {
        return Err(ProtocolError::InvalidCursor);
    }
    Ok(())
}

fn valid_proof_hex(value: &str) -> bool {
    value.len() == PROOF_BYTES * 2 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("unsupported bridge protocol version")]
    UnsupportedVersion,
    #[error("invalid bridge request id")]
    InvalidRequestId,
    #[error("invalid client profile id")]
    InvalidProfileId,
    #[error("invalid client display label")]
    InvalidClientLabel,
    #[error("a new client must provide a 32-byte pairing key")]
    PairingKeyRequired,
    #[error("an existing client must not replace its pairing key")]
    UnexpectedPairingKey,
    #[error("invalid bridge connection id")]
    InvalidConnectionId,
    #[error("invalid project id")]
    InvalidProjectId,
    #[error("discovery page limit must be between 1 and 100")]
    InvalidPageLimit,
    #[error("invalid discovery cursor")]
    InvalidCursor,
    #[error("catalog search text is too long")]
    InvalidSearch,
    #[error("invalid relation identity")]
    InvalidRelationIdentity,
    #[error("SQL must contain 1-262144 bytes")]
    InvalidSql,
    #[error("invalid immutable SQL snapshot id")]
    InvalidSnapshotId,
    #[error("invalid result id")]
    InvalidResultId,
    #[error("invalid profile request")]
    InvalidProfileRequest,
    #[error("invalid export ID")]
    InvalidExportId,
    #[error("invalid export destination ID")]
    InvalidDestinationId,
    #[error("invalid export base name")]
    InvalidExportBaseName,
    #[error("invalid export rows per part")]
    InvalidExportRowsPerPart,
    #[error("invalid export options")]
    InvalidExportOptions,
    #[error("invalid authentication proof")]
    InvalidProof,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(action: BridgeAction) -> BridgeRequest {
        BridgeRequest {
            id: "request-1".into(),
            protocol_version: BRIDGE_PROTOCOL_VERSION,
            action,
        }
    }

    #[test]
    fn validates_hello_and_authentication_boundaries() {
        assert!(request(BridgeAction::Hello {
            profile_id: String::new(),
            label: "Claude Desktop".into(),
            pairing_key: Some("01".repeat(PROOF_BYTES)),
        })
        .validate()
        .is_ok());
        assert_eq!(
            request(BridgeAction::Authenticate {
                connection_id: "connection-1".into(),
                profile_id: "profile-1".into(),
                proof: "not-hex".into(),
            })
            .validate(),
            Err(ProtocolError::InvalidProof)
        );
    }

    #[test]
    fn rejects_wrong_protocol_and_oversized_labels() {
        let mut wrong = request(BridgeAction::Ping);
        wrong.protocol_version += 1;
        assert_eq!(wrong.validate(), Err(ProtocolError::UnsupportedVersion));
        assert_eq!(
            request(BridgeAction::Hello {
                profile_id: String::new(),
                label: "x".repeat(MAX_CLIENT_LABEL_BYTES + 1),
                pairing_key: Some("01".repeat(PROOF_BYTES)),
            })
            .validate(),
            Err(ProtocolError::InvalidClientLabel)
        );
    }

    #[test]
    fn validates_discovery_bounds() {
        assert!(request(BridgeAction::ListCatalog {
            connection_id: "connection-1".into(),
            project_id: "project-1".into(),
            search: None,
            cursor: None,
            limit: 100,
        })
        .validate()
        .is_ok());
        assert_eq!(
            request(BridgeAction::ListCatalog {
                connection_id: "connection-1".into(),
                project_id: "project-1".into(),
                search: None,
                cursor: None,
                limit: 101,
            })
            .validate(),
            Err(ProtocolError::InvalidPageLimit)
        );
    }

    #[test]
    fn guarded_export_intent_is_closed_and_bounded() {
        let csv = AgentExportIntent {
            snapshot_id: "snapshot-1".into(),
            destination_id: "destination-1".into(),
            format: ExportFormat::Csv,
            base_name: "daily_orders".into(),
            rows_per_part: MAX_AGENT_EXPORT_ROWS_PER_PART,
            csv: Some(AgentCsvExportOptions {
                delimiter: ",".into(),
                include_header: true,
            }),
            parquet: None,
        };
        assert!(request(BridgeAction::ProposeExport {
            connection_id: "connection-1".into(),
            intent: csv.clone(),
        })
        .validate()
        .is_ok());

        let mut invalid = csv.clone();
        invalid.base_name = "../escape".into();
        assert_eq!(
            invalid.validate(),
            Err(ProtocolError::InvalidExportBaseName)
        );
        invalid = csv.clone();
        invalid.rows_per_part = MAX_AGENT_EXPORT_ROWS_PER_PART + 1;
        assert_eq!(
            invalid.validate(),
            Err(ProtocolError::InvalidExportRowsPerPart)
        );
        invalid = csv;
        invalid.parquet = Some(AgentParquetExportOptions {
            compression: AgentParquetCompression::Snappy,
        });
        assert_eq!(invalid.validate(), Err(ProtocolError::InvalidExportOptions));

        let injected = serde_json::json!({
            "id": "request-1",
            "protocolVersion": BRIDGE_PROTOCOL_VERSION,
            "method": "propose_export",
            "params": {
                "connectionId": "connection-1",
                "intent": {
                    "snapshotId": "snapshot-1",
                    "destinationId": "destination-1",
                    "format": "csv",
                    "baseName": "orders",
                    "rowsPerPart": 100,
                    "csv": {"delimiter": ",", "includeHeader": true},
                    "parquet": null,
                    "path": "/tmp/escape",
                    "sql": "COPY secrets TO '/tmp/escape'",
                    "overwrite": true
                }
            }
        });
        assert!(serde_json::from_value::<BridgeRequest>(injected).is_err());
    }

    #[test]
    fn grants_report_only_real_capabilities() {
        assert!(!ProjectGrant {
            project_id: "p".into(),
            inspect: false,
            analyze: false,
            modify_workspace: false,
            modify_data: false,
        }
        .has_any());
    }
}

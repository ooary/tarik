use std::{
    borrow::Cow,
    future::Future,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{
        GetPromptRequestParams, GetPromptResponse, Implementation, ListPromptsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    schemars,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{Deserialize, Serialize};
use tarik_agent_protocol::{
    AgentCsvExportOptions, AgentExportIntent, AgentParquetCompression, AgentParquetExportOptions,
    AuthenticationResult, ExportFormat, HelloResult, HelloState, MAX_AGENT_EXPORT_BASE_NAME_BYTES,
    MAX_AGENT_EXPORT_ROWS_PER_PART, MAX_AGENT_PAGE_ROWS, MAX_DISCOVERY_PAGE_ITEMS,
};
use zeroize::Zeroizing;

use crate::{
    bridge::{generate_pairing_key, BridgeClient},
    guidance,
    profile::{self, ClientProfile},
};

const SUPPORTED_PROTOCOLS: &[ProtocolVersion] =
    &[ProtocolVersion::V_2025_06_18, ProtocolVersion::V_2025_11_25];

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfoRequest {
    #[schemars(description = "Refresh the local Tarik connection before returning status")]
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EmptyRequest {}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRequest {
    pub project_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormatInput {
    Csv,
    Parquet,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ParquetCompressionInput {
    Uncompressed,
    Snappy,
    Gzip,
    Zstd,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CsvExportInput {
    #[schemars(description = "One ASCII delimiter byte other than NUL, quote, CR, or LF")]
    pub delimiter: String,
    pub include_header: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ParquetExportInput {
    pub compression: ParquetCompressionInput,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportIntentRequest {
    pub snapshot_id: String,
    pub destination_id: String,
    pub format: ExportFormatInput,
    #[schemars(length(min = 1, max = 64), pattern(r"^[A-Za-z0-9_-]+$"))]
    pub base_name: String,
    #[schemars(range(min = 1, max = 1_000_000))]
    pub rows_per_part: u64,
    #[serde(default)]
    pub csv: Option<CsvExportInput>,
    #[serde(default)]
    pub parquet: Option<ParquetExportInput>,
}

impl From<ExportIntentRequest> for AgentExportIntent {
    fn from(value: ExportIntentRequest) -> Self {
        Self {
            snapshot_id: value.snapshot_id,
            destination_id: value.destination_id,
            format: match value.format {
                ExportFormatInput::Csv => ExportFormat::Csv,
                ExportFormatInput::Parquet => ExportFormat::Parquet,
            },
            base_name: value.base_name,
            rows_per_part: value.rows_per_part,
            csv: value.csv.map(|csv| AgentCsvExportOptions {
                delimiter: csv.delimiter,
                include_header: csv.include_header,
            }),
            parquet: value.parquet.map(|parquet| AgentParquetExportOptions {
                compression: match parquet.compression {
                    ParquetCompressionInput::Uncompressed => AgentParquetCompression::Uncompressed,
                    ParquetCompressionInput::Snappy => AgentParquetCompression::Snappy,
                    ParquetCompressionInput::Gzip => AgentParquetCompression::Gzip,
                    ParquetCompressionInput::Zstd => AgentParquetCompression::Zstd,
                },
            }),
        }
    }
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportIdRequest {
    pub export_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CatalogRequest {
    pub project_id: String,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DescribeRelationRequest {
    pub project_id: String,
    pub database: String,
    pub schema: String,
    pub name: String,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default = "default_page_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: u32,
}

fn default_page_limit() -> u32 {
    50
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClassifySqlRequest {
    pub project_id: String,
    pub sql: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRequest {
    pub snapshot_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExplainRequest {
    pub snapshot_id: String,
    #[serde(default)]
    pub actual: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRequest {
    pub execution_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResultPageRequest {
    pub result_id: String,
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_page_limit")]
    #[schemars(range(min = 1, max = 500))]
    pub max_rows: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResultRequest {
    pub result_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub approval_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRequestInput {
    pub project_id: String,
    pub database: String,
    pub schema: String,
    pub relation: String,
    pub relation_kind: String,
    pub catalog_revision: String,
    pub columns: Vec<ProfileColumnInput>,
    #[serde(default)]
    pub exact: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileColumnInput {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileIdRequest {
    pub profile_id: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectPageRequest {
    pub project_id: String,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_page_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: u32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct QualityRunsRequest {
    pub project_id: String,
    #[serde(default)]
    pub check_id: Option<String>,
    #[serde(default)]
    pub offset: u32,
    #[serde(default = "default_page_limit")]
    #[schemars(range(min = 1, max = 100))]
    pub limit: u32,
}

#[derive(Debug, Clone, Serialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TarikServerStatus {
    pub available: bool,
    pub paired: bool,
    pub authenticated: bool,
    pub client_profile_id: Option<String>,
    pub granted_projects: usize,
    pub guidance: String,
}

#[derive(Clone)]
pub struct TarikMcpServer {
    state: Arc<Mutex<ServerState>>,
    tool_router: ToolRouter<Self>,
}

struct PendingPairing {
    hello: HelloResult,
    pairing_key: Zeroizing<String>,
    profile_dir: PathBuf,
    label: String,
}

struct ServerState {
    profile_name: String,
    label: String,
    status: TarikServerStatus,
    bridge: Option<BridgeClient>,
    pending_pairing: Option<PendingPairing>,
}

impl TarikMcpServer {
    pub fn new(profile_name: String, label: String) -> Self {
        Self {
            state: Arc::new(Mutex::new(ServerState {
                profile_name,
                label,
                status: unavailable_status("Connect Tarik and enable Agent Access."),
                bridge: None,
                pending_pairing: None,
            })),
            tool_router: Self::tool_router(),
        }
    }

    pub fn connect(&self) -> TarikServerStatus {
        let status = match self.state.lock() {
            Ok(mut state) => {
                connect_state(&mut state).unwrap_or_else(|error| unavailable_status(&error))
            }
            Err(_) => unavailable_status("The local MCP state is unavailable."),
        };
        if let Ok(mut state) = self.state.lock() {
            state.status = status.clone();
        }
        status
    }

    pub fn heartbeat(&self) {
        if let Ok(mut state) = self.state.lock() {
            if ensure_authenticated_bridge(&mut state).is_ok() {
                if let Some(bridge) = state.bridge.as_mut() {
                    if bridge.heartbeat().is_err() {
                        state.bridge = None;
                        state.status = unavailable_status(
                            "The Tarik desktop connection closed. Retry a tool to reconnect.",
                        );
                    }
                }
            }
        }
    }
}

#[tool_router]
impl TarikMcpServer {
    #[tool(
        name = "tarik_server_info",
        description = "Report whether the local Tarik desktop is available, paired, authenticated, and granted to projects. This tool never opens a project or runs SQL."
    )]
    fn server_info(
        &self,
        Parameters(request): Parameters<ServerInfoRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let status = if request.refresh {
            self.connect()
        } else {
            self.state
                .lock()
                .map(|state| state.status.clone())
                .unwrap_or_else(|_| unavailable_status("The local MCP state is unavailable."))
        };
        Ok(structured(status))
    }

    #[tool(
        name = "tarik_list_projects",
        description = "List only Tarik projects explicitly granted to this paired client. Paths are never returned. A project must be active in visible Tarik before catalog or analysis tools can use it."
    )]
    fn list_projects(
        &self,
        Parameters(_request): Parameters<EmptyRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.list_projects()))
    }

    #[tool(
        name = "tarik_list_active",
        description = "List bounded queries, retained results, active connections, slot state, cache bytes, and effective analysis limits owned by this paired client profile. Use it to recover IDs after reconnecting. It never returns SQL text, rows, paths, credentials, or another profile's activity."
    )]
    fn list_active(
        &self,
        Parameters(_request): Parameters<EmptyRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.list_active()))
    }

    #[tool(
        name = "tarik_list_export_destinations",
        description = "List only redacted export destination grants for this authenticated client and explicitly granted active project. Returns opaque destination IDs, labels, allowed CSV/Parquet formats, quotas, create-new-only policy, and readiness. Absolute paths are never returned. This tool cannot create, edit, repair, enable, disable, revoke, or select a destination."
    )]
    fn list_export_destinations(
        &self,
        Parameters(request): Parameters<ProjectRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.list_export_destinations(request.project_id)))
    }

    #[tool(
        name = "tarik_propose_export",
        description = "Consume one owned immutable SafeRead snapshot and propose a complete CSV or Parquet export to one opaque Tarik destination. Accepts no SQL, path, URL, COPY, overwrite flag, or arbitrary option string. Within-policy create-new work starts delegated; policy exceptions wait for visible Tarik approval; collisions require fresh critical typed confirmation in Tarik."
    )]
    fn propose_export(
        &self,
        Parameters(request): Parameters<ExportIntentRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        if request.base_name.len() > MAX_AGENT_EXPORT_BASE_NAME_BYTES
            || request.rows_per_part == 0
            || request.rows_per_part > MAX_AGENT_EXPORT_ROWS_PER_PART
        {
            return Ok(tool_error(
                "agent.invalid_export_intent",
                "baseName and rowsPerPart are outside Tarik's guarded export bounds",
            ));
        }
        Ok(self.bridge_call(|bridge| bridge.propose_export(request.into())))
    }

    #[tool(
        name = "tarik_export_status",
        description = "Poll one complete-query export owned by this authenticated MCP connection. Returns exact aggregate counters, bounded relative part names, decision/state, and path-free errors only."
    )]
    fn export_status(
        &self,
        Parameters(request): Parameters<ExportIdRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.export_status(request.export_id)))
    }

    #[tool(
        name = "tarik_export_cancel",
        description = "Request cancellation of one complete-query export owned by this authenticated MCP connection. Poll status until Tarik reports a terminal state."
    )]
    fn export_cancel(
        &self,
        Parameters(request): Parameters<ExportIdRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.export_cancel(request.export_id)))
    }

    #[tool(
        name = "tarik_export_release",
        description = "Release one terminal export ownership record. Completed user files and persisted aggregate history are preserved. Active exports must be cancelled first."
    )]
    fn export_release(
        &self,
        Parameters(request): Parameters<ExportIdRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.export_release(request.export_id)))
    }

    #[tool(
        name = "tarik_list_catalog",
        description = "List a bounded page of relation metadata from an explicitly granted active Tarik project. This performs catalog inspection only: it returns no data rows and runs no user query. Continue with the opaque nextCursor when present."
    )]
    fn list_catalog(
        &self,
        Parameters(request): Parameters<CatalogRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        if request.limit == 0 || request.limit > MAX_DISCOVERY_PAGE_ITEMS {
            return Ok(tool_error(
                "agent.invalid_page_limit",
                "limit must be between 1 and 100",
            ));
        }
        Ok(self.bridge_call(|bridge| {
            bridge.list_catalog(
                request.project_id,
                request.search,
                request.cursor,
                request.limit,
            )
        }))
    }

    #[tool(
        name = "tarik_describe_relation",
        description = "Describe one exact relation and a bounded page of its columns in an explicitly granted active Tarik project. This performs catalog inspection only and never scans relation data. Continue with the opaque nextCursor when present."
    )]
    fn describe_relation(
        &self,
        Parameters(request): Parameters<DescribeRelationRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        if request.limit == 0 || request.limit > MAX_DISCOVERY_PAGE_ITEMS {
            return Ok(tool_error(
                "agent.invalid_page_limit",
                "limit must be between 1 and 100",
            ));
        }
        Ok(self.bridge_call(|bridge| {
            bridge.describe_relation(
                request.project_id,
                request.database,
                request.schema,
                request.name,
                request.cursor,
                request.limit,
            )
        }))
    }

    #[tool(
        name = "tarik_classify_sql",
        description = "Classify exactly one immutable SQL statement against the current granted project catalog without executing it. Safe reads return a one-use snapshotId for tarik_query_start. Mutations return approval or critical classification but cannot use the SafeRead lane. Unknown or external effects are blocked."
    )]
    fn classify_sql(
        &self,
        Parameters(request): Parameters<ClassifySqlRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.classify_sql(request.project_id, request.sql)))
    }

    #[tool(
        name = "tarik_query_start",
        description = "Start a bounded read using only a one-use immutable SafeRead snapshotId returned by tarik_classify_sql. SQL cannot be supplied or changed here. The result is capped at 5000 rows and 60 seconds."
    )]
    fn query_start(
        &self,
        Parameters(request): Parameters<SnapshotRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.start_query(request.snapshot_id)))
    }

    #[tool(
        name = "tarik_query_status",
        description = "Poll one SafeRead query owned by this paired client profile. Returns bounded lifecycle, slot, capped-result, and cache metadata only; use tarik_list_active to recover IDs after reconnecting."
    )]
    fn query_status(
        &self,
        Parameters(request): Parameters<ExecutionRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.query_status(request.execution_id)))
    }

    #[tool(
        name = "tarik_query_cancel",
        description = "Request cancellation of one query owned by this authenticated MCP connection."
    )]
    fn query_cancel(
        &self,
        Parameters(request): Parameters<ExecutionRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.cancel_query(request.execution_id)))
    }

    #[tool(
        name = "tarik_result_page",
        description = "Read at most 500 rows and 1 MiB from a bounded result owned by this paired client profile for the currently granted project. NULL and truncated-cell metadata are preserved."
    )]
    fn result_page(
        &self,
        Parameters(request): Parameters<ResultPageRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        if request.max_rows == 0 || request.max_rows > MAX_AGENT_PAGE_ROWS {
            return Ok(tool_error(
                "agent.invalid_page_limit",
                "maxRows must be between 1 and 500",
            ));
        }
        Ok(self.bridge_call(|bridge| {
            bridge.result_page(request.result_id, request.offset, request.max_rows)
        }))
    }

    #[tool(
        name = "tarik_result_release",
        description = "Idempotently release one bounded result owned by this paired client profile. Tables, sources, projects, and completed exports are preserved."
    )]
    fn result_release(
        &self,
        Parameters(request): Parameters<ResultRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.release_result(request.result_id)))
    }

    #[tool(
        name = "tarik_propose_sql",
        description = "Create a visible one-use Tarik-owned approval request from an immutable approval-required SQL snapshot. Accepts only snapshotId; SQL cannot be resent. This tool cannot approve the request."
    )]
    fn propose_sql(
        &self,
        Parameters(request): Parameters<SnapshotRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.propose_sql(request.snapshot_id)))
    }

    #[tool(
        name = "tarik_approval_status",
        description = "Read the pending, approved, denied, expired, used, or failed state of an approval owned by this MCP connection. No MCP method can approve it."
    )]
    fn approval_status(
        &self,
        Parameters(request): Parameters<ApprovalRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.approval_status(request.approval_id)))
    }

    #[tool(
        name = "tarik_execute_approved",
        description = "Execute exactly one immutable server-held SQL snapshot after direct approval inside visible Tarik. Accepts only approvalId; SQL and action arguments cannot be supplied."
    )]
    fn execute_approved(
        &self,
        Parameters(request): Parameters<ApprovalRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.execute_approved(request.approval_id)))
    }

    #[tool(
        name = "tarik_query_flow",
        description = "Explain a one-use immutable SafeRead snapshot. Estimate mode is non-executing; actual=true explicitly runs EXPLAIN ANALYZE under a bounded deadline. The original query is never opened in the editor."
    )]
    fn query_flow(
        &self,
        Parameters(request): Parameters<ExplainRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.explain_sql(request.snapshot_id, request.actual)))
    }

    #[tool(
        name = "tarik_profile_start",
        description = "Start a bounded exact or approximate profile for an exact relation and selected columns from the current granted catalog. Existing Tarik profile provenance and response budgets apply."
    )]
    fn profile_start(
        &self,
        Parameters(request): Parameters<ProfileRequestInput>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        let request = tarik_engine_protocol::ProfileRequest {
            project_id: request.project_id,
            target: tarik_engine_protocol::ProfileTarget {
                database: request.database,
                schema: request.schema,
                name: request.relation,
                kind: request.relation_kind,
            },
            columns: request
                .columns
                .into_iter()
                .map(|column| tarik_engine_protocol::ProfileColumn {
                    name: column.name,
                    data_type: column.data_type,
                })
                .collect(),
            catalog_revision: request.catalog_revision,
            mode: if request.exact {
                tarik_engine_protocol::ProfileMode::Exact
            } else {
                tarik_engine_protocol::ProfileMode::Approximate
            },
        };
        Ok(self.bridge_call(|bridge| bridge.start_profile(request)))
    }

    #[tool(
        name = "tarik_profile_status",
        description = "Poll one bounded profile owned by this authenticated MCP connection. Metrics retain Exact, Approximate, or Sampled provenance and exact SQL evidence."
    )]
    fn profile_status(
        &self,
        Parameters(request): Parameters<ProfileIdRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.profile_status(request.profile_id)))
    }

    #[tool(
        name = "tarik_profile_cancel",
        description = "Cancel one bounded profile owned by this authenticated MCP connection."
    )]
    fn profile_cancel(
        &self,
        Parameters(request): Parameters<ProfileIdRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| bridge.cancel_profile(request.profile_id)))
    }

    #[tool(
        name = "tarik_list_quality_checks",
        description = "List a bounded page of quality-check definitions from an explicitly granted project. This never runs a check or returns failure rows."
    )]
    fn list_quality_checks(
        &self,
        Parameters(request): Parameters<ProjectPageRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| {
            bridge.list_quality(request.project_id, request.offset, request.limit)
        }))
    }

    #[tool(
        name = "tarik_list_quality_runs",
        description = "List bounded aggregate quality-run facts. Failure rows are never persisted or returned; any future preview must be labeled current-data preview."
    )]
    fn list_quality_runs(
        &self,
        Parameters(request): Parameters<QualityRunsRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| {
            bridge.list_quality_runs(
                request.project_id,
                request.check_id,
                request.offset,
                request.limit,
            )
        }))
    }

    #[tool(
        name = "tarik_list_saved_queries",
        description = "List a bounded page of saved SQL from a project with explicit Modify workspace access. Listing never opens an editor or executes SQL."
    )]
    fn list_saved_queries(
        &self,
        Parameters(request): Parameters<ProjectPageRequest>,
    ) -> Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
        Ok(self.bridge_call(|bridge| {
            bridge.list_saved_queries(request.project_id, request.offset, request.limit)
        }))
    }
}

impl TarikMcpServer {
    fn bridge_call<T: Serialize>(
        &self,
        operation: impl FnOnce(&mut BridgeClient) -> Result<T, String>,
    ) -> rmcp::model::CallToolResult {
        let result = self
            .state
            .lock()
            .map_err(|_| "agent.state_unavailable: Local MCP state is unavailable.".to_string())
            .and_then(|mut state| {
                ensure_authenticated_bridge(&mut state)?;
                let bridge = state.bridge.as_mut().ok_or_else(|| {
                    "agent.authentication_required: Call tarik_server_info with refresh true and complete pairing in Tarik.".to_string()
                })?;
                let result = operation(bridge);
                if result.as_ref().is_err_and(|error| bridge_transport_failed(error)) {
                    state.bridge = None;
                    state.pending_pairing = None;
                    state.status = unavailable_status(
                        "The Tarik desktop connection closed. Retry the tool to reconnect.",
                    );
                }
                result
            });
        match result {
            Ok(value) => structured(value),
            Err(error) => {
                let (code, message) = error
                    .split_once(": ")
                    .unwrap_or(("agent.request_failed", error.as_str()));
                tool_error(code, message)
            }
        }
    }
}

fn ensure_authenticated_bridge(state: &mut ServerState) -> Result<(), String> {
    let agent_dir = profile::desktop_agent_directory()?;
    let bridge_is_current = state
        .bridge
        .as_ref()
        .map(|bridge| bridge.endpoint_is_current(&agent_dir))
        .unwrap_or(false);
    if !bridge_is_current {
        state.bridge = None;
        state.pending_pairing = None;
    }
    if bridge_is_current && state.status.authenticated {
        return Ok(());
    }

    let status = match connect_state(state) {
        Ok(status) => status,
        Err(error) => {
            state.status = unavailable_status(&error);
            return Err(error);
        }
    };
    let authenticated = status.authenticated;
    let guidance = status.guidance.clone();
    state.status = status;
    if authenticated {
        Ok(())
    } else {
        Err(format!("agent.authentication_required: {guidance}"))
    }
}

fn bridge_transport_failed(error: &str) -> bool {
    error.starts_with("Tarik closed the local agent connection")
        || error.starts_with("Tarik did not answer the local agent request")
        || error.starts_with("Tarik bridge response is too large")
        || error.starts_with("Tarik returned an invalid local agent response")
        || error.starts_with("Tarik returned an unexpected local agent response")
}

fn structured(value: impl Serialize) -> rmcp::model::CallToolResult {
    match serde_json::to_value(value) {
        Ok(value) => rmcp::model::CallToolResult::structured(value),
        Err(_) => tool_error(
            "agent.response_encode",
            "Tarik could not encode the response.",
        ),
    }
}

fn tool_error(code: &str, message: &str) -> rmcp::model::CallToolResult {
    rmcp::model::CallToolResult::structured_error(serde_json::json!({
        "code": code,
        "message": message,
        "retryable": matches!(
            code,
            "agent.authentication_required" | "agent.project_closed" | "agent.busy"
        ),
    }))
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for TarikMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(Implementation::new("tarik-mcp", env!("CARGO_PKG_VERSION")))
        .with_instructions(guidance::SERVER_INSTRUCTIONS)
    }

    fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListPromptsResult, rmcp::ErrorData>> + Send + '_ {
        std::future::ready(Ok(ListPromptsResult {
            prompts: guidance::prompts(),
            ..Default::default()
        }))
    }

    fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<GetPromptResponse, rmcp::ErrorData>> + Send + '_ {
        std::future::ready(
            guidance::get_prompt(&request.name)
                .map(Into::into)
                .ok_or_else(|| {
                    rmcp::ErrorData::resource_not_found(
                        "Unknown Tarik prompt",
                        Some(serde_json::json!({ "name": request.name })),
                    )
                }),
        )
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_PROTOCOLS)
    }
}

fn connect_state(state: &mut ServerState) -> Result<TarikServerStatus, String> {
    let agent_dir = profile::desktop_agent_directory()?;
    if state
        .bridge
        .as_ref()
        .is_some_and(|bridge| !bridge.endpoint_is_current(&agent_dir))
    {
        state.bridge = None;
        state.pending_pairing = None;
    }
    if let Some(pending) = state.pending_pairing.take() {
        let result = state
            .bridge
            .as_mut()
            .ok_or_else(|| "The pending Tarik pairing connection was closed.".to_string())?
            .authenticate(&pending.hello, &pending.pairing_key);
        return match result {
            Ok(authenticated) => {
                profile::save(
                    &pending.profile_dir,
                    &ClientProfile {
                        profile_id: pending.hello.profile_id,
                        client_label: pending.label,
                        pairing_key: pending.pairing_key.to_string(),
                    },
                )?;
                Ok(authenticated_status(authenticated))
            }
            Err(error) if pairing_still_pending(&error) => {
                let profile_id = pending.hello.profile_id.clone();
                state.pending_pairing = Some(pending);
                Ok(pairing_status(profile_id))
            }
            Err(error) => {
                state.bridge = None;
                Err(error)
            }
        };
    }

    let profile_dir = profile::profile_directory(&state.profile_name)?;
    let existing = profile::load(&profile_dir)?;
    let mut bridge = BridgeClient::connect(&agent_dir)?;
    match existing {
        Some(existing) => {
            let hello = bridge.hello(Some(&existing), &state.label, None)?;
            if hello.state == HelloState::PairingRequired {
                let profile_id = hello.profile_id.clone();
                state.bridge = Some(bridge);
                state.pending_pairing = Some(PendingPairing {
                    hello,
                    pairing_key: Zeroizing::new(existing.pairing_key.clone()),
                    profile_dir,
                    label: state.label.clone(),
                });
                return Ok(pairing_status(profile_id));
            }
            let authenticated = bridge.authenticate(&hello, &existing.pairing_key)?;
            state.bridge = Some(bridge);
            Ok(authenticated_status(authenticated))
        }
        None => {
            let pairing_key = Zeroizing::new(generate_pairing_key()?);
            let hello = bridge.hello(None, &state.label, Some(&pairing_key))?;
            if hello.state != HelloState::PairingRequired {
                return Err("Tarik did not create a pairing request".into());
            }
            profile::save(
                &profile_dir,
                &ClientProfile {
                    profile_id: hello.profile_id.clone(),
                    client_label: state.label.clone(),
                    pairing_key: pairing_key.to_string(),
                },
            )?;
            let profile_id = hello.profile_id.clone();
            state.bridge = Some(bridge);
            state.pending_pairing = Some(PendingPairing {
                hello,
                pairing_key,
                profile_dir,
                label: state.label.clone(),
            });
            Ok(pairing_status(profile_id))
        }
    }
}

fn pairing_still_pending(error: &str) -> bool {
    error.starts_with("agent.authentication_failed") || error.starts_with("agent.connection_stale")
}

fn pairing_status(profile_id: String) -> TarikServerStatus {
    TarikServerStatus {
        available: true,
        paired: false,
        authenticated: false,
        client_profile_id: Some(profile_id),
        granted_projects: 0,
        guidance: "Approve the pending pairing request in Tarik, then call tarik_server_info with refresh true.".into(),
    }
}

fn authenticated_status(result: AuthenticationResult) -> TarikServerStatus {
    let granted_projects = result.grants.len();
    TarikServerStatus {
        available: true,
        paired: true,
        authenticated: true,
        client_profile_id: Some(result.client_profile_id),
        granted_projects,
        guidance: if granted_projects == 0 {
            "Pairing is complete. Grant the active project in Tarik before using project tools."
                .into()
        } else {
            "Tarik is ready for the capabilities granted in the desktop.".into()
        },
    }
}

fn unavailable_status(guidance: &str) -> TarikServerStatus {
    TarikServerStatus {
        available: false,
        paired: false,
        authenticated: false,
        client_profile_id: None,
        granted_projects: 0,
        guidance: guidance.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_only_reviewed_protocol_versions() {
        let server = TarikMcpServer::new("test".into(), "Test".into());
        assert_eq!(
            server.supported_protocol_versions(),
            Cow::Borrowed(SUPPORTED_PROTOCOLS)
        );
        let info = server.get_info();
        assert!(info.capabilities.tools.is_some());
        assert!(info.capabilities.prompts.is_some());
        assert!(info.capabilities.resources.is_none());
        assert_eq!(
            info.instructions.as_deref(),
            Some(guidance::SERVER_INSTRUCTIONS)
        );
    }

    #[test]
    fn tool_contract_is_bounded_and_has_no_mutation_or_approval_tool() {
        let server = TarikMcpServer::new("test".into(), "Test".into());
        let tools = server.tool_router.list_all();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 26);
        for required in [
            "tarik_classify_sql",
            "tarik_describe_relation",
            "tarik_list_catalog",
            "tarik_list_projects",
            "tarik_list_active",
            "tarik_list_export_destinations",
            "tarik_propose_export",
            "tarik_export_status",
            "tarik_export_cancel",
            "tarik_export_release",
            "tarik_query_cancel",
            "tarik_query_start",
            "tarik_query_status",
            "tarik_query_flow",
            "tarik_result_page",
            "tarik_result_release",
            "tarik_server_info",
            "tarik_propose_sql",
            "tarik_approval_status",
            "tarik_execute_approved",
            "tarik_profile_start",
            "tarik_profile_status",
            "tarik_profile_cancel",
            "tarik_list_quality_checks",
            "tarik_list_quality_runs",
            "tarik_list_saved_queries",
        ] {
            assert!(names.contains(&required));
        }
        let catalog = server.tool_router.get("tarik_list_catalog").unwrap();
        assert_eq!(catalog.input_schema["properties"]["limit"]["maximum"], 100);
        let page = server.tool_router.get("tarik_result_page").unwrap();
        assert_eq!(page.input_schema["properties"]["maxRows"]["maximum"], 500);
        assert!(!names.contains(&"tarik_approve"));
        assert!(!names.iter().any(|name| name.contains("execute_sql")));
        assert!(!names.iter().any(|name| {
            name.contains("create_export_destination")
                || name.contains("repair_export_destination")
                || name.contains("revoke_export_destination")
        }));
        let destinations = &server
            .tool_router
            .get("tarik_list_export_destinations")
            .unwrap()
            .input_schema;
        assert_eq!(
            destinations.keys().collect::<Vec<_>>(),
            ["$schema", "properties", "required", "type"]
        );
        assert_eq!(destinations["required"], serde_json::json!(["projectId"]));
        assert!(destinations["properties"].get("path").is_none());
        let export = &server
            .tool_router
            .get("tarik_propose_export")
            .unwrap()
            .input_schema;
        let properties = export["properties"].as_object().unwrap();
        assert_eq!(properties.len(), 7);
        for required in [
            "snapshotId",
            "destinationId",
            "format",
            "baseName",
            "rowsPerPart",
        ] {
            assert!(properties.contains_key(required), "missing {required}");
        }
        for forbidden in ["sql", "path", "url", "overwrite", "options"] {
            assert!(!properties.contains_key(forbidden), "exposed {forbidden}");
        }
        assert_eq!(properties["baseName"]["maxLength"], 64);
        assert_eq!(properties["rowsPerPart"]["maximum"], 1_000_000);
    }
}

use std::{
    borrow::Cow,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ProtocolVersion, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router, ServerHandler,
};
use serde::{Deserialize, Serialize};
use tarik_agent_protocol::{
    AuthenticationResult, HelloResult, HelloState, MAX_AGENT_PAGE_ROWS, MAX_DISCOVERY_PAGE_ITEMS,
};
use zeroize::Zeroizing;

use crate::{
    bridge::{generate_pairing_key, BridgeClient},
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
        description = "Poll one query owned by this authenticated MCP connection. Returns bounded lifecycle and result metadata only."
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
        description = "Read at most 500 rows and 1 MiB from a bounded result owned by this authenticated connection. NULL and truncated-cell metadata are preserved."
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
        description = "Explicitly release one bounded result owned by this authenticated MCP connection."
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
                let bridge = state.bridge.as_mut().ok_or_else(|| {
                    "agent.authentication_required: Call tarik_server_info with refresh true and complete pairing in Tarik.".to_string()
                })?;
                operation(bridge)
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
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("tarik-mcp", env!("CARGO_PKG_VERSION")))
            .with_instructions(
                "Tarik is a local SQL workbench. The desktop owns project access, SQL policy, execution, approvals, and results. Use tarik_server_info first. No tool approval can be performed through MCP.",
            )
    }

    fn supported_protocol_versions(&self) -> Cow<'static, [ProtocolVersion]> {
        Cow::Borrowed(SUPPORTED_PROTOCOLS)
    }
}

fn connect_state(state: &mut ServerState) -> Result<TarikServerStatus, String> {
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
    let agent_dir = profile::desktop_agent_directory()?;
    let existing = profile::load(&profile_dir)?;
    let mut bridge = BridgeClient::connect(&agent_dir)?;
    match existing {
        Some(existing) => {
            let hello = bridge.hello(Some(&existing), &state.label, None)?;
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
        assert!(info.capabilities.prompts.is_none());
        assert!(info.capabilities.resources.is_none());
    }

    #[test]
    fn tool_contract_is_bounded_and_has_no_mutation_or_approval_tool() {
        let server = TarikMcpServer::new("test".into(), "Test".into());
        let tools = server.tool_router.list_all();
        let names = tools
            .iter()
            .map(|tool| tool.name.as_ref())
            .collect::<Vec<_>>();
        assert_eq!(names.len(), 13);
        for required in [
            "tarik_classify_sql",
            "tarik_describe_relation",
            "tarik_list_catalog",
            "tarik_list_projects",
            "tarik_query_cancel",
            "tarik_query_start",
            "tarik_query_status",
            "tarik_result_page",
            "tarik_result_release",
            "tarik_server_info",
            "tarik_propose_sql",
            "tarik_approval_status",
            "tarik_execute_approved",
        ] {
            assert!(names.contains(&required));
        }
        let catalog = server.tool_router.get("tarik_list_catalog").unwrap();
        assert_eq!(catalog.input_schema["properties"]["limit"]["maximum"], 100);
        let page = server.tool_router.get("tarik_result_page").unwrap();
        assert_eq!(page.input_schema["properties"]["maxRows"]["maximum"], 500);
        assert!(!names.contains(&"tarik_approve"));
        assert!(!names.iter().any(|name| name.contains("execute_sql")));
    }
}

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
use tarik_agent_protocol::{AuthenticationResult, HelloResult, HelloState};
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
        Ok(rmcp::model::CallToolResult::structured(
            serde_json::to_value(status).unwrap_or_else(|_| {
                serde_json::json!({
                    "available": false,
                    "guidance": "Tarik status could not be encoded."
                })
            }),
        ))
    }
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
}

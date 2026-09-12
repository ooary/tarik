//! Reviewed host-configuration assistant for the local MCP gateway.
//!
//! This module configures an MCP host only. It cannot pair a client, grant a
//! project, approve an action, or start a third-party agent. Mutating work is
//! represented by a short-lived, one-use plan and is applied only by a direct
//! Tauri command from the visible Tarik UI.

use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::metadata::{settings::SettingsRepository, MetadataDb};

const PLAN_LIFETIME: Duration = Duration::from_secs(5 * 60);
const PROCESS_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(windows)]
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PROCESS_OUTPUT: usize = 64 * 1024;
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
const MAX_PLANS: usize = 16;
const MAX_RECEIPTS: usize = 32;
const MAX_BACKUPS_PER_HOST: usize = 3;
const RECEIPTS_KEY: &str = "agent.setup.receipts";
const MANAGED_SERVER_NAME: &str = "tarik";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostKind {
    ClaudeDesktop,
    ClaudeCode,
    Codex,
    Pi,
    Cursor,
    VsCode,
    Generic,
}

impl HostKind {
    fn label(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "Claude Desktop",
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex",
            Self::Pi => "Pi",
            Self::Cursor => "Cursor",
            Self::VsCode => "VS Code",
            Self::Generic => "Generic MCP host",
        }
    }

    fn profile(self) -> &'static str {
        match self {
            Self::ClaudeDesktop => "claude-desktop",
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Pi => "pi",
            Self::Cursor => "cursor",
            Self::VsCode => "vscode",
            Self::Generic => "generic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupMethod {
    OfficialCli,
    ManagedJson,
    Guided,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostState {
    NotInstalled,
    Unsupported,
    NotConfigured,
    Configured,
    RestartRequired,
    RepairRequired,
    Conflict,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostInstallation {
    pub kind: HostKind,
    pub display_name: String,
    pub setup_method: SetupMethod,
    pub state: HostState,
    pub version: Option<String>,
    pub detail: String,
    pub can_configure: bool,
    pub can_remove: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSetupStatus {
    pub platform: String,
    pub packaged_server_ready: bool,
    pub hosts: Vec<HostInstallation>,
    pub topology_note: String,
    pub duplicate_diagnosis: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupOperation {
    Configure,
    Repair,
    Remove,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostSetupPlanView {
    pub plan_id: String,
    pub host_kind: HostKind,
    pub host_name: String,
    pub operation: SetupOperation,
    pub setup_method: SetupMethod,
    pub summary: String,
    pub command_preview: Option<String>,
    pub config_target: Option<String>,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupReceipt {
    pub receipt_id: String,
    pub host_kind: HostKind,
    pub operation: SetupOperation,
    pub managed_entry_hash: String,
    pub executable_identity: String,
    #[serde(default)]
    managed_command: String,
    #[serde(default)]
    managed_args: Vec<String>,
    pub backup_identity: Option<String>,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupApplyResult {
    pub host_kind: HostKind,
    pub operation: SetupOperation,
    pub state: HostState,
    pub message: String,
}

#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone)]
enum PlanAction {
    Guided,
    Cli {
        executable: PathBuf,
        commands: Vec<Vec<String>>,
        rollback_arguments: Option<Vec<String>>,
        receipt_rollback_commands: Vec<Vec<String>>,
        verify_arguments: Vec<String>,
        expected_server: ExpectedServer,
    },
    ClaudeDesktop {
        config_path: PathBuf,
        operation: SetupOperation,
        expected_entry: Value,
        owned_entry_hash: Option<String>,
        before_bytes: Vec<u8>,
        before_hash: String,
    },
}

#[derive(Debug, Clone)]
struct SetupPlan {
    view: HostSetupPlanView,
    packaged_mcp: PathBuf,
    action: PlanAction,
    created_at: Instant,
}

#[derive(Debug, Clone)]
struct ExpectedServer {
    host: HostKind,
    command: String,
    args: Vec<String>,
    expect_present: bool,
}

#[derive(Debug)]
struct ProcessOutcome {
    success: bool,
    stdout: String,
    stderr: String,
}

pub struct AgentSetupManager {
    settings: SettingsRepository,
    plans: Mutex<HashMap<String, SetupPlan>>,
    packaged_mcp_override: Option<PathBuf>,
}

impl AgentSetupManager {
    pub fn new(database: MetadataDb) -> Self {
        Self {
            settings: SettingsRepository::new(database),
            plans: Mutex::new(HashMap::new()),
            packaged_mcp_override: None,
        }
    }

    #[cfg(test)]
    fn with_packaged_mcp(mut self, path: PathBuf) -> Self {
        self.packaged_mcp_override = Some(path);
        self
    }

    pub fn status(&self) -> Result<AgentSetupStatus, String> {
        let packaged_mcp = self.locate_packaged_mcp()?;
        let packaged_server_ready = trusted_packaged_mcp(&packaged_mcp).is_ok();
        let receipts = self.receipts()?;
        let hosts = detect_hosts(&packaged_mcp, packaged_server_ready, &receipts)?;
        Ok(AgentSetupStatus {
            platform: std::env::consts::OS.into(),
            packaged_server_ready,
            hosts,
            topology_note: "Each configured MCP host transport owns one tarik-mcp stdio child. Multiple hosts or configured transport instances may legitimately create multiple processes; Tarik Desktop does not spawn adapters and does not enforce a machine-wide singleton.".into(),
            duplicate_diagnosis: if cfg!(windows) {
                "Configuration ownership is checked per reviewed host entry. Use Activity → Agents plus Task Manager parent-process details to distinguish legitimate host children, transient reconnect overlap, and orphans; Tarik never kills by executable name."
            } else {
                "Native Windows duplicate-process reproduction has not been run on this host. Configuration ownership checks remain available, but process parentage is unverified."
            }
            .into(),
        })
    }

    pub fn plan(
        &self,
        host_kind: HostKind,
        operation: SetupOperation,
    ) -> Result<HostSetupPlanView, String> {
        let packaged_mcp = trusted_packaged_mcp(&self.locate_packaged_mcp()?)?;
        let receipts = self.receipts()?;
        let receipt = receipts
            .iter()
            .find(|receipt| receipt.host_kind == host_kind);
        let plan = build_plan(host_kind, operation, packaged_mcp, receipt)?;
        let view = plan.view.clone();
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "agent.setup_state_unavailable".to_string())?;
        plans.retain(|_, plan| plan.created_at.elapsed() < PLAN_LIFETIME);
        if plans.len() >= MAX_PLANS {
            return Err(
                "agent.setup_plan_limit: Finish or wait for an existing setup plan.".into(),
            );
        }
        plans.insert(view.plan_id.clone(), plan);
        Ok(view)
    }

    pub fn apply(&self, plan_id: &str) -> Result<SetupApplyResult, String> {
        let plan = self
            .plans
            .lock()
            .map_err(|_| "agent.setup_state_unavailable".to_string())?
            .remove(plan_id)
            .ok_or_else(|| {
                "agent.setup_plan_missing: Review the host setup again before applying it."
                    .to_string()
            })?;
        if plan.created_at.elapsed() >= PLAN_LIFETIME {
            return Err("agent.setup_plan_expired: Review the host setup again.".into());
        }
        let current_mcp = trusted_packaged_mcp(&self.locate_packaged_mcp()?)?;
        if current_mcp != plan.packaged_mcp {
            return Err(
                "agent.setup_plan_stale: The Tarik package moved. Review setup again.".into(),
            );
        }

        let (state, managed_entry_hash, backup_identity, message) = match &plan.action {
            PlanAction::Guided => {
                return Err(
                    "agent.setup_guided_only: Follow the displayed instructions in the MCP host."
                        .into(),
                )
            }
            PlanAction::Cli {
                executable,
                commands,
                rollback_arguments,
                receipt_rollback_commands,
                verify_arguments,
                expected_server,
            } => {
                trusted_host_executable(executable)?;
                for (index, arguments) in commands.iter().enumerate() {
                    let outcome = run_bounded(executable, arguments, PROCESS_TIMEOUT)?;
                    if !outcome.success {
                        if index > 0 {
                            if let Some(rollback) = rollback_arguments {
                                let restored = run_bounded(executable, rollback, PROCESS_TIMEOUT)?;
                                if !restored.success {
                                    return Err("agent.setup_recovery_required: Host repair failed and the prior Tarik entry could not be restored.".into());
                                }
                            }
                        }
                        return Err(safe_process_error("agent.setup_process_failed", &outcome));
                    }
                }
                let verified = run_bounded(executable, verify_arguments, PROCESS_TIMEOUT)?;
                if !verified.success
                    || verify_server_output(&verified.stdout, expected_server)
                        != expected_server.expect_present
                {
                    if let Err(rollback_error) =
                        run_cli_rollback(executable, receipt_rollback_commands)
                    {
                        return Err(format!(
                            "agent.setup_recovery_required: Host verification failed and rollback failed: {rollback_error}"
                        ));
                    }
                    return Err("agent.setup_verification_failed: The host did not report Tarik's expected managed state; the reviewed change was reverted.".into());
                }
                (
                    match plan.view.operation {
                        SetupOperation::Remove => HostState::NotConfigured,
                        _ => HostState::RestartRequired,
                    },
                    expected_server.hash(),
                    None,
                    match plan.view.operation {
                        SetupOperation::Remove => "Tarik was removed from the host configuration.",
                        _ => "Tarik was configured. Restart the host to begin pairing.",
                    },
                )
            }
            PlanAction::ClaudeDesktop {
                config_path,
                operation,
                expected_entry,
                owned_entry_hash,
                before_bytes: _,
                before_hash,
            } => {
                let result = apply_claude_config(
                    config_path,
                    *operation,
                    expected_entry,
                    owned_entry_hash.as_deref(),
                    before_hash,
                )?;
                (
                    match operation {
                        SetupOperation::Remove => HostState::NotConfigured,
                        _ => HostState::RestartRequired,
                    },
                    hash_value(expected_entry),
                    result.backup_identity,
                    match operation {
                        SetupOperation::Remove => "Tarik was removed from Claude Desktop.",
                        _ => "Tarik was configured. Restart Claude Desktop to begin pairing.",
                    },
                )
            }
        };

        let receipt = SetupReceipt {
            receipt_id: uuid::Uuid::new_v4().to_string(),
            host_kind: plan.view.host_kind,
            operation: plan.view.operation,
            managed_entry_hash,
            executable_identity: hash_bytes(plan.packaged_mcp.to_string_lossy().as_bytes()),
            managed_command: plan.packaged_mcp.to_string_lossy().into_owned(),
            managed_args: mcp_arguments(plan.view.host_kind),
            backup_identity,
            completed_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(error) = self.record_receipt(receipt) {
            if let Err(rollback_error) = rollback_after_receipt_failure(&plan.action) {
                return Err(format!(
                    "agent.setup_recovery_required: Receipt storage failed ({error}); host rollback failed ({rollback_error})."
                ));
            }
            return Err(format!(
                "agent.setup_receipt_failed: Host change was reverted because its ownership receipt could not be stored: {error}"
            ));
        }

        Ok(SetupApplyResult {
            host_kind: plan.view.host_kind,
            operation: plan.view.operation,
            state,
            message: message.into(),
        })
    }

    fn locate_packaged_mcp(&self) -> Result<PathBuf, String> {
        if let Some(path) = &self.packaged_mcp_override {
            return Ok(path.clone());
        }
        let current = std::env::current_exe()
            .map_err(|error| format!("agent.setup_package_missing: {error}"))?;
        let name = if cfg!(windows) {
            "tarik-mcp.exe"
        } else {
            "tarik-mcp"
        };
        let sibling = current
            .parent()
            .ok_or_else(|| {
                "agent.setup_package_missing: Tarik has no executable directory.".to_string()
            })?
            .join(name);
        if sibling.exists() {
            return Ok(sibling);
        }
        let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap_or_else(|| Path::new("."));
        Ok(workspace.join("target/debug").join(name))
    }

    fn receipts(&self) -> Result<Vec<SetupReceipt>, String> {
        self.settings
            .get::<Vec<SetupReceipt>>(RECEIPTS_KEY)
            .map_err(|error| error.to_string())
            .map(Option::unwrap_or_default)
    }

    fn record_receipt(&self, receipt: SetupReceipt) -> Result<(), String> {
        let mut receipts = self.receipts()?;
        receipts.retain(|current| current.host_kind != receipt.host_kind);
        if receipt.operation != SetupOperation::Remove {
            receipts.insert(0, receipt);
            receipts.truncate(MAX_RECEIPTS);
        }
        self.settings
            .set(RECEIPTS_KEY, &receipts)
            .map_err(|error| error.to_string())
    }
}

fn build_plan(
    host: HostKind,
    operation: SetupOperation,
    packaged_mcp: PathBuf,
    receipt: Option<&SetupReceipt>,
) -> Result<SetupPlan, String> {
    let plan_id = uuid::Uuid::new_v4().to_string();
    let (setup_method, action, summary, command_preview, config_target) = if !cfg!(windows) {
        let command = stdio_command(&packaged_mcp, host);
        (
            SetupMethod::Guided,
            PlanAction::Guided,
            format!("Add Tarik as a local stdio MCP server in {}.", host.label()),
            Some(command),
            None,
        )
    } else {
        build_windows_plan(host, operation, &packaged_mcp, receipt)?
    };
    let view = HostSetupPlanView {
        plan_id,
        host_kind: host,
        host_name: host.label().into(),
        operation,
        setup_method,
        summary,
        command_preview,
        config_target,
        expires_in_seconds: PLAN_LIFETIME.as_secs(),
    };
    Ok(SetupPlan {
        view,
        packaged_mcp,
        action,
        created_at: Instant::now(),
    })
}

type SetupPlanParts = (
    SetupMethod,
    PlanAction,
    String,
    Option<String>,
    Option<String>,
);

#[cfg(not(windows))]
fn build_windows_plan(
    _host: HostKind,
    _operation: SetupOperation,
    _packaged_mcp: &Path,
    _receipt: Option<&SetupReceipt>,
) -> Result<SetupPlanParts, String> {
    unreachable!("Windows setup plans are built only on Windows")
}

#[cfg(windows)]
fn build_windows_plan(
    host: HostKind,
    operation: SetupOperation,
    packaged_mcp: &Path,
    receipt: Option<&SetupReceipt>,
) -> Result<SetupPlanParts, String> {
    match host {
        HostKind::ClaudeDesktop => {
            let app_data = std::env::var_os("APPDATA")
                .ok_or_else(|| "agent.setup_host_missing: APPDATA is unavailable.".to_string())?;
            let directory = PathBuf::from(app_data).join("Claude");
            verify_config_parent(&directory).map_err(|_| {
                "agent.setup_host_missing: A verified Claude Desktop user directory was not found."
                    .to_string()
            })?;
            let path = directory.join("claude_desktop_config.json");
            let expected = managed_entry(packaged_mcp, host);
            let before = read_config_bytes(&path)?;
            let before_hash = hash_bytes(&before);
            let owned_entry_hash = receipt.map(|receipt| receipt.managed_entry_hash.clone());
            let preview = preview_claude_change(&before, operation, &expected, receipt)?;
            Ok((
                SetupMethod::ManagedJson,
                PlanAction::ClaudeDesktop {
                    config_path: path.clone(),
                    operation,
                    expected_entry: expected,
                    owned_entry_hash,
                    before_bytes: before,
                    before_hash,
                },
                preview,
                None,
                Some(path.to_string_lossy().into_owned()),
            ))
        }
        HostKind::ClaudeCode | HostKind::Codex => {
            let executable = find_direct_host_executable(host)?.ok_or_else(|| {
                format!(
                    "agent.setup_host_missing: {} has no reviewed directly executable .exe.",
                    host.label()
                )
            })?;
            let current_state = inspect_cli_state(host, &executable, packaged_mcp, receipt)?;
            match (operation, current_state) {
                (SetupOperation::Configure, HostState::NotConfigured)
                | (SetupOperation::Repair, HostState::RepairRequired)
                | (SetupOperation::Remove, HostState::Configured | HostState::RepairRequired) => {}
                (SetupOperation::Configure, HostState::Configured) => {
                    return Err(format!(
                        "agent.setup_already_configured: {} already has Tarik's exact managed entry.",
                        host.label()
                    ));
                }
                (SetupOperation::Configure, HostState::Conflict)
                | (SetupOperation::Repair | SetupOperation::Remove, HostState::Conflict) => {
                    return Err(
                        "agent.setup_conflict: A same-named host entry is not owned by Tarik."
                            .into(),
                    );
                }
                _ => {
                    return Err(
                        "agent.setup_state_unsupported: Refresh and review the host state again."
                            .into(),
                    );
                }
            }
            let expected = ExpectedServer {
                host,
                command: packaged_mcp.to_string_lossy().into_owned(),
                args: mcp_arguments(host),
                expect_present: operation != SetupOperation::Remove,
            };
            let add = cli_arguments(host, SetupOperation::Configure, packaged_mcp);
            let remove = cli_arguments(host, SetupOperation::Remove, packaged_mcp);
            let (commands, rollback_arguments, receipt_rollback_commands) = match operation {
                SetupOperation::Configure => (vec![add.clone()], None, vec![remove.clone()]),
                SetupOperation::Remove => {
                    let receipt = require_owned_receipt(receipt, host)?;
                    let restore = cli_add_arguments(
                        host,
                        Path::new(&receipt.managed_command),
                        &receipt.managed_args,
                    );
                    (vec![remove.clone()], None, vec![restore])
                }
                SetupOperation::Repair => {
                    let receipt = require_owned_receipt(receipt, host)?;
                    let restore = cli_add_arguments(
                        host,
                        Path::new(&receipt.managed_command),
                        &receipt.managed_args,
                    );
                    (
                        vec![remove.clone(), add.clone()],
                        Some(restore.clone()),
                        vec![remove.clone(), restore],
                    )
                }
            };
            let verify_arguments = match operation {
                SetupOperation::Remove => vec!["mcp".into(), "list".into()],
                _ if host == HostKind::Codex => {
                    vec![
                        "mcp".into(),
                        "get".into(),
                        MANAGED_SERVER_NAME.into(),
                        "--json".into(),
                    ]
                }
                _ => vec!["mcp".into(), "get".into(), MANAGED_SERVER_NAME.into()],
            };
            let preview = commands
                .iter()
                .map(|arguments| format_command(&executable, arguments))
                .collect::<Vec<_>>()
                .join("\n");
            Ok((
                SetupMethod::OfficialCli,
                PlanAction::Cli {
                    executable,
                    commands,
                    rollback_arguments,
                    receipt_rollback_commands,
                    verify_arguments,
                    expected_server: expected,
                },
                match operation {
                    SetupOperation::Remove => format!("Remove Tarik from {}.", host.label()),
                    _ => format!(
                        "Register packaged tarik-mcp.exe for the current user in {}.",
                        host.label()
                    ),
                },
                Some(preview),
                None,
            ))
        }
        _ => Ok((
            SetupMethod::Guided,
            PlanAction::Guided,
            format!(
                "Add Tarik manually in {} using the reviewed stdio command.",
                host.label()
            ),
            Some(stdio_command(packaged_mcp, host)),
            None,
        )),
    }
}

fn detect_hosts(
    packaged_mcp: &Path,
    packaged_ready: bool,
    receipts: &[SetupReceipt],
) -> Result<Vec<HostInstallation>, String> {
    let mut hosts = Vec::new();
    for kind in [
        HostKind::ClaudeDesktop,
        HostKind::ClaudeCode,
        HostKind::Codex,
        HostKind::Pi,
        HostKind::Cursor,
        HostKind::VsCode,
        HostKind::Generic,
    ] {
        if cfg!(windows) {
            let detected = detect_windows_host(
                kind,
                packaged_mcp,
                packaged_ready,
                receipts.iter().find(|receipt| receipt.host_kind == kind),
            )
            .unwrap_or_else(|error| HostInstallation {
                kind,
                display_name: kind.label().into(),
                setup_method: SetupMethod::Guided,
                state: if kind == HostKind::ClaudeDesktop {
                    HostState::Conflict
                } else {
                    HostState::Unsupported
                },
                version: None,
                detail: safe_probe_error(&error),
                can_configure: false,
                can_remove: false,
            });
            hosts.push(detected);
        } else {
            hosts.push(HostInstallation {
                kind,
                display_name: kind.label().into(),
                setup_method: SetupMethod::Guided,
                state: HostState::Unsupported,
                version: None,
                detail: "One-click setup is reviewed for native Windows. Use the displayed stdio command on this platform.".into(),
                can_configure: packaged_ready,
                can_remove: false,
            });
        }
    }
    Ok(hosts)
}

#[cfg(not(windows))]
fn detect_windows_host(
    _kind: HostKind,
    _packaged_mcp: &Path,
    _packaged_ready: bool,
    _receipt: Option<&SetupReceipt>,
) -> Result<HostInstallation, String> {
    unreachable!("Windows host detection runs only on Windows")
}

#[cfg(windows)]
fn detect_windows_host(
    kind: HostKind,
    packaged_mcp: &Path,
    packaged_ready: bool,
    receipt: Option<&SetupReceipt>,
) -> Result<HostInstallation, String> {
    match kind {
        HostKind::ClaudeDesktop => {
            let Some(app_data) = std::env::var_os("APPDATA") else {
                return Ok(missing_host(kind, SetupMethod::ManagedJson, packaged_ready));
            };
            let directory = PathBuf::from(app_data).join("Claude");
            if !directory.is_dir() {
                return Ok(missing_host(kind, SetupMethod::ManagedJson, packaged_ready));
            }
            let path = directory.join("claude_desktop_config.json");
            let bytes = read_config_bytes(&path)?;
            let expected = managed_entry(packaged_mcp, kind);
            let state = inspect_owned_claude_state(&bytes, &expected, receipt)?;
            Ok(HostInstallation {
                kind,
                display_name: kind.label().into(),
                setup_method: SetupMethod::ManagedJson,
                state,
                version: None,
                detail: host_state_detail(state).into(),
                can_configure: packaged_ready && state != HostState::Conflict,
                can_remove: matches!(state, HostState::Configured | HostState::RepairRequired),
            })
        }
        HostKind::ClaudeCode | HostKind::Codex => {
            let Some(executable) = find_direct_host_executable(kind)? else {
                return Ok(missing_host(kind, SetupMethod::OfficialCli, packaged_ready));
            };
            let version = run_bounded(&executable, &["--version".into()], VERSION_TIMEOUT)?;
            if !version.success {
                return Ok(HostInstallation {
                    kind,
                    display_name: kind.label().into(),
                    setup_method: SetupMethod::Guided,
                    state: HostState::Unsupported,
                    version: None,
                    detail: "The installed executable did not return a reviewed version. Use guided setup.".into(),
                    can_configure: packaged_ready,
                    can_remove: false,
                });
            }
            let text = version.stdout.trim().to_string();
            let supported = reviewed_version(kind, &text);
            let state = if supported {
                inspect_cli_state(kind, &executable, packaged_mcp, receipt)?
            } else {
                HostState::Unsupported
            };
            Ok(HostInstallation {
                kind,
                display_name: kind.label().into(),
                setup_method: if supported {
                    SetupMethod::OfficialCli
                } else {
                    SetupMethod::Guided
                },
                state,
                version: Some(text),
                detail: if supported {
                    host_state_detail(state)
                } else {
                    "This host version has not been reviewed. Use guided setup."
                }
                .into(),
                can_configure: packaged_ready
                    && matches!(state, HostState::NotConfigured | HostState::RepairRequired),
                can_remove: matches!(state, HostState::Configured | HostState::RepairRequired),
            })
        }
        _ => Ok(HostInstallation {
            kind,
            display_name: kind.label().into(),
            setup_method: SetupMethod::Guided,
            state: HostState::Unsupported,
            version: None,
            detail:
                "Guided stdio setup is available; Tarik will not modify this host automatically."
                    .into(),
            can_configure: packaged_ready,
            can_remove: false,
        }),
    }
}

#[cfg(windows)]
fn missing_host(kind: HostKind, method: SetupMethod, packaged_ready: bool) -> HostInstallation {
    HostInstallation {
        kind,
        display_name: kind.label().into(),
        setup_method: method,
        state: HostState::NotInstalled,
        version: None,
        detail: "No reviewed current-user installation was found.".into(),
        can_configure: packaged_ready && method == SetupMethod::Guided,
        can_remove: false,
    }
}

#[cfg(windows)]
fn host_state_detail(state: HostState) -> &'static str {
    match state {
        HostState::NotConfigured => "Installed, but Tarik is not configured.",
        HostState::Configured => "Tarik's exact packaged MCP command is configured.",
        HostState::RepairRequired => {
            "Tarik is configured with another executable path. Review repair."
        }
        HostState::Conflict => {
            "A different entry named tarik exists. Tarik will not replace it automatically."
        }
        _ => "Review the host state before making a change.",
    }
}

#[cfg(windows)]
fn managed_entry(packaged_mcp: &Path, host: HostKind) -> Value {
    json!({
        "command": packaged_mcp.to_string_lossy(),
        "args": mcp_arguments(host),
    })
}

fn mcp_arguments(host: HostKind) -> Vec<String> {
    vec![
        "--profile".into(),
        host.profile().into(),
        "--label".into(),
        host.label().into(),
    ]
}

#[cfg(any(windows, test))]
fn cli_arguments(host: HostKind, operation: SetupOperation, packaged_mcp: &Path) -> Vec<String> {
    if operation == SetupOperation::Remove {
        let mut args = vec!["mcp".into(), "remove".into()];
        if host == HostKind::ClaudeCode {
            args.extend(["--scope".into(), "user".into()]);
        }
        args.push(MANAGED_SERVER_NAME.into());
        return args;
    }
    cli_add_arguments(host, packaged_mcp, &mcp_arguments(host))
}

#[cfg(any(windows, test))]
fn cli_add_arguments(host: HostKind, command: &Path, managed_args: &[String]) -> Vec<String> {
    let mut args = vec!["mcp".into(), "add".into()];
    if host == HostKind::ClaudeCode {
        args.extend(["--scope".into(), "user".into()]);
    }
    args.extend([
        MANAGED_SERVER_NAME.into(),
        "--".into(),
        command.to_string_lossy().into_owned(),
    ]);
    args.extend(managed_args.iter().cloned());
    args
}

#[cfg(windows)]
fn require_owned_receipt<'a>(
    receipt: Option<&'a SetupReceipt>,
    host: HostKind,
) -> Result<&'a SetupReceipt, String> {
    receipt
        .filter(|receipt| {
            receipt.host_kind == host
                && !receipt.managed_command.is_empty()
                && !receipt.managed_args.is_empty()
        })
        .ok_or_else(|| {
            "agent.setup_not_managed: Tarik has no ownership receipt for this host entry."
                .to_string()
        })
}

fn stdio_command(packaged_mcp: &Path, host: HostKind) -> String {
    format_command(packaged_mcp, &mcp_arguments(host))
}

fn format_command(executable: &Path, arguments: &[String]) -> String {
    std::iter::once(executable.to_string_lossy().into_owned())
        .chain(arguments.iter().cloned())
        .map(|value| {
            if value.contains([' ', '\t', '"']) {
                format!("\"{}\"", value.replace('"', "\\\""))
            } else {
                value
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn trusted_packaged_mcp(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| {
        "agent.setup_package_missing: Packaged tarik-mcp was not found.".to_string()
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("agent.setup_package_unsafe: Packaged tarik-mcp is not a regular file.".into());
    }
    #[cfg(windows)]
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
        != Some("exe")
        || is_windows_reparse(&metadata)
    {
        return Err(
            "agent.setup_package_unsafe: Packaged MCP server is not a direct executable.".into(),
        );
    }
    let canonical =
        fs::canonicalize(path).map_err(|error| format!("agent.setup_package_unsafe: {error}"))?;
    #[cfg(windows)]
    verify_windows_owner(&canonical)?;
    Ok(canonical)
}

fn trusted_host_executable(path: &Path) -> Result<PathBuf, String> {
    trusted_packaged_mcp(path)
}

fn safe_probe_error(error: &str) -> String {
    let message = error
        .split_once(": ")
        .map(|(_, message)| message)
        .unwrap_or("The host could not be inspected safely.");
    message.chars().take(240).collect()
}

#[cfg(windows)]
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(windows)]
pub(crate) fn verify_windows_owner(path: &Path) -> Result<(), String> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS},
        Security::{
            Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT},
            EqualSid, GetTokenInformation, TokenUser, OWNER_SECURITY_INFORMATION, TOKEN_QUERY,
            TOKEN_USER,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        let mut owner = ptr::null_mut();
        let mut descriptor = ptr::null_mut();
        let code = GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            &mut descriptor,
        );
        if code != ERROR_SUCCESS || owner.is_null() || descriptor.is_null() {
            return Err("agent.setup_unsafe_owner: Could not verify executable ownership.".into());
        }
        let mut token = ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            LocalFree(descriptor);
            return Err("agent.setup_unsafe_owner: Could not read current-user identity.".into());
        }
        let mut required = 0u32;
        GetTokenInformation(token, TokenUser, ptr::null_mut(), 0, &mut required);
        if required == 0
            || std::io::Error::last_os_error().raw_os_error()
                != Some(ERROR_INSUFFICIENT_BUFFER as i32)
        {
            CloseHandle(token);
            LocalFree(descriptor);
            return Err("agent.setup_unsafe_owner: Could not size current-user identity.".into());
        }
        let mut buffer = vec![0u8; required as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            buffer.as_mut_ptr().cast(),
            required,
            &mut required,
        ) == 0
        {
            CloseHandle(token);
            LocalFree(descriptor);
            return Err("agent.setup_unsafe_owner: Could not read current-user identity.".into());
        }
        let user = &*(buffer.as_ptr().cast::<TOKEN_USER>());
        let equal = EqualSid(owner, user.User.Sid) != 0;
        CloseHandle(token);
        LocalFree(descriptor);
        if !equal {
            return Err(
                "agent.setup_unsafe_owner: Executable is not owned by the current user.".into(),
            );
        }
    }
    Ok(())
}

#[cfg(windows)]
fn find_direct_host_executable(host: HostKind) -> Result<Option<PathBuf>, String> {
    let file_name = match host {
        HostKind::ClaudeCode => "claude.exe",
        HostKind::Codex => "codex.exe",
        _ => return Ok(None),
    };
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        candidates.push(
            local
                .join("Programs")
                .join(file_name.trim_end_matches(".exe"))
                .join(file_name),
        );
        candidates.push(
            local
                .join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join(file_name),
        );
    }
    if let Some(app_data) = std::env::var_os("APPDATA") {
        candidates.extend(npm_host_executable_candidates(
            host,
            &PathBuf::from(app_data),
        ));
    }
    if let Some(path) = std::env::var_os("PATH") {
        candidates.extend(
            std::env::split_paths(&path)
                .take(8)
                .map(|directory| directory.join(file_name)),
        );
    }
    for candidate in candidates.into_iter().take(8) {
        if candidate.is_file() {
            return trusted_host_executable(&candidate).map(Some);
        }
    }
    Ok(None)
}

#[cfg(windows)]
fn npm_host_executable_candidates(host: HostKind, app_data: &Path) -> Vec<PathBuf> {
    let modules = app_data.join("npm").join("node_modules");
    match host {
        HostKind::ClaudeCode => {
            let package = modules.join("@anthropic-ai").join("claude-code");
            let native_package = if cfg!(target_arch = "aarch64") {
                "claude-code-win32-arm64"
            } else {
                "claude-code-win32-x64"
            };
            vec![
                package.join("bin").join("claude.exe"),
                package
                    .join("node_modules")
                    .join("@anthropic-ai")
                    .join(native_package)
                    .join("claude.exe"),
            ]
        }
        HostKind::Codex => {
            let (native_package, target) = if cfg!(target_arch = "aarch64") {
                ("codex-win32-arm64", "aarch64-pc-windows-msvc")
            } else {
                ("codex-win32-x64", "x86_64-pc-windows-msvc")
            };
            vec![modules
                .join("@openai")
                .join("codex")
                .join("node_modules")
                .join("@openai")
                .join(native_package)
                .join("vendor")
                .join(target)
                .join("bin")
                .join("codex.exe")]
        }
        _ => Vec::new(),
    }
}

#[cfg(windows)]
fn inspect_cli_state(
    host: HostKind,
    executable: &Path,
    packaged_mcp: &Path,
    receipt: Option<&SetupReceipt>,
) -> Result<HostState, String> {
    let arguments = if host == HostKind::Codex {
        vec![
            "mcp".into(),
            "get".into(),
            MANAGED_SERVER_NAME.into(),
            "--json".into(),
        ]
    } else {
        vec!["mcp".into(), "get".into(), MANAGED_SERVER_NAME.into()]
    };
    let observed = run_bounded(executable, &arguments, VERSION_TIMEOUT)?;
    if !observed.success {
        let combined = format!("{}\n{}", observed.stdout, observed.stderr).to_ascii_lowercase();
        if combined.contains("no mcp server named") {
            return Ok(HostState::NotConfigured);
        }
        return Err("agent.setup_verification_failed: The host configuration could not be inspected safely.".into());
    }

    let current = ExpectedServer {
        host,
        command: packaged_mcp.to_string_lossy().into_owned(),
        args: mcp_arguments(host),
        expect_present: true,
    };
    let current_matches = verify_server_output(&observed.stdout, &current);
    let current_owned = receipt.is_some_and(|receipt| {
        receipt.managed_entry_hash == current.hash()
            && receipt.managed_command == current.command
            && receipt.managed_args == current.args
    });
    if current_matches {
        return Ok(if current_owned {
            HostState::Configured
        } else {
            HostState::Conflict
        });
    }

    let previous_owned = receipt.is_some_and(|receipt| {
        if receipt.managed_command.is_empty() || receipt.managed_args.is_empty() {
            return false;
        }
        let previous = ExpectedServer {
            host,
            command: receipt.managed_command.clone(),
            args: receipt.managed_args.clone(),
            expect_present: true,
        };
        receipt.managed_entry_hash == previous.hash()
            && verify_server_output(&observed.stdout, &previous)
    });
    Ok(if previous_owned {
        HostState::RepairRequired
    } else {
        HostState::Conflict
    })
}

#[cfg(windows)]
fn reviewed_version(host: HostKind, version: &str) -> bool {
    let number = version
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .unwrap_or_default();
    let mut pieces = number
        .split('.')
        .filter_map(|value| value.parse::<u32>().ok());
    match (host, pieces.next(), pieces.next()) {
        (HostKind::ClaudeCode, Some(major), Some(minor)) => major == 2 && minor >= 1,
        (HostKind::Codex, Some(major), Some(minor)) => major == 0 && minor >= 149,
        _ => false,
    }
}

fn run_bounded(
    executable: &Path,
    arguments: &[String],
    timeout: Duration,
) -> Result<ProcessOutcome, String> {
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("agent.setup_process_start_failed: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent.setup_process_pipe".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "agent.setup_process_pipe".to_string())?;
    let out_reader = thread::spawn(move || read_bounded(stdout));
    let err_reader = thread::spawn(move || read_bounded(stderr));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("agent.setup_process_wait_failed: {error}"))?
        {
            break status;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = out_reader.join();
            let _ = err_reader.join();
            return Err(
                "agent.setup_process_timeout: The host command timed out and was stopped.".into(),
            );
        }
        thread::sleep(Duration::from_millis(25));
    };
    let stdout = out_reader
        .join()
        .map_err(|_| "agent.setup_process_output_failed".to_string())??;
    let stderr = err_reader
        .join()
        .map_err(|_| "agent.setup_process_output_failed".to_string())??;
    Ok(ProcessOutcome {
        success: status.success(),
        stdout,
        stderr,
    })
}

fn read_bounded(mut reader: impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    reader
        .by_ref()
        .take((MAX_PROCESS_OUTPUT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("agent.setup_process_output_failed: {error}"))?;
    if bytes.len() > MAX_PROCESS_OUTPUT {
        return Err("agent.setup_process_output_too_large: Host output exceeded 64 KiB.".into());
    }
    String::from_utf8(bytes)
        .map_err(|_| "agent.setup_process_output_invalid: Host output was not UTF-8.".into())
}

fn safe_process_error(code: &str, outcome: &ProcessOutcome) -> String {
    let detail = outcome
        .stderr
        .lines()
        .chain(outcome.stdout.lines())
        .find(|line| !line.trim().is_empty())
        .unwrap_or("The host command failed.")
        .chars()
        .take(240)
        .collect::<String>();
    format!("{code}: {detail}")
}

fn run_cli_rollback(executable: &Path, commands: &[Vec<String>]) -> Result<(), String> {
    for arguments in commands {
        let outcome = run_bounded(executable, arguments, PROCESS_TIMEOUT)?;
        if !outcome.success {
            return Err(safe_process_error("agent.setup_rollback_failed", &outcome));
        }
    }
    Ok(())
}

fn rollback_after_receipt_failure(action: &PlanAction) -> Result<(), String> {
    match action {
        PlanAction::Guided => Ok(()),
        PlanAction::Cli {
            executable,
            receipt_rollback_commands,
            ..
        } => run_cli_rollback(executable, receipt_rollback_commands),
        PlanAction::ClaudeDesktop {
            config_path,
            operation,
            expected_entry,
            owned_entry_hash,
            before_bytes,
            ..
        } => rollback_claude_config(
            config_path,
            *operation,
            expected_entry,
            owned_entry_hash.as_deref(),
            before_bytes,
        ),
    }
}

impl ExpectedServer {
    fn hash(&self) -> String {
        hash_value(&json!({ "command": self.command, "args": self.args }))
    }
}

fn verify_server_output(output: &str, expected: &ExpectedServer) -> bool {
    match (expected.host, expected.expect_present) {
        (HostKind::Codex, true) => verify_codex_get(output, expected),
        (HostKind::Codex, false) => codex_list_contains_tarik(output),
        (HostKind::ClaudeCode, true) => verify_claude_get(output, expected),
        (HostKind::ClaudeCode, false) => claude_list_contains_tarik(output),
        _ => false,
    }
}

fn verify_codex_get(output: &str, expected: &ExpectedServer) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(output) else {
        return false;
    };
    let Some(transport) = value.get("transport").and_then(Value::as_object) else {
        return false;
    };
    let command = transport.get("command").and_then(Value::as_str);
    let kind = transport.get("type").and_then(Value::as_str);
    let args = transport
        .get("args")
        .and_then(Value::as_array)
        .map(|args| args.iter().map(Value::as_str).collect::<Option<Vec<_>>>());
    value.get("name").and_then(Value::as_str) == Some(MANAGED_SERVER_NAME)
        && kind == Some("stdio")
        && command == Some(expected.command.as_str())
        && args.flatten().as_deref()
            == Some(
                expected
                    .args
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
            )
}

fn codex_list_contains_tarik(output: &str) -> bool {
    serde_json::from_str::<Value>(output)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .is_some_and(|servers| {
            servers.iter().any(|server| {
                server.get("name").and_then(Value::as_str) == Some(MANAGED_SERVER_NAME)
            })
        })
}

fn verify_claude_get(output: &str, expected: &ExpectedServer) -> bool {
    let type_field = unique_claude_field(output, "Type: ");
    let command = unique_claude_field(output, "Command: ");
    let args = unique_claude_field(output, "Args: ");
    type_field.as_deref() == Some("stdio")
        && command.as_deref() == Some(expected.command.as_str())
        && args.as_deref() == Some(expected.args.join(" ").as_str())
}

fn unique_claude_field(output: &str, prefix: &str) -> Option<String> {
    let mut values = output
        .lines()
        .filter_map(|line| line.trim().strip_prefix(prefix))
        .map(str::to_string);
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn claude_list_contains_tarik(output: &str) -> bool {
    output
        .lines()
        .any(|line| line.trim_start().starts_with("tarik:"))
}

struct ConfigApplyResult {
    backup_identity: Option<String>,
}

fn read_config_bytes(path: &Path) -> Result<Vec<u8>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("agent.setup_config_unsafe: Host config is not a regular file.".into());
            }
            #[cfg(windows)]
            {
                if is_windows_reparse(&metadata) {
                    return Err("agent.setup_config_unsafe: Host config is a reparse point.".into());
                }
                verify_windows_owner(path)?;
            }
            if metadata.len() > MAX_CONFIG_BYTES {
                return Err("agent.setup_config_too_large: Host config exceeds 1 MiB.".into());
            }
            fs::read(path).map_err(|error| format!("agent.setup_config_read_failed: {error}"))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(format!("agent.setup_config_read_failed: {error}")),
    }
}

fn parse_config(bytes: &[u8]) -> Result<Map<String, Value>, String> {
    if bytes.is_empty() {
        return Ok(Map::new());
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| {
        "agent.setup_config_malformed: Host config is not valid UTF-8 JSON.".to_string()
    })?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| "agent.setup_config_malformed: Host config root must be an object.".into())
}

fn inspect_claude_state(bytes: &[u8], expected: &Value) -> Result<HostState, String> {
    let root = parse_config(bytes)?;
    let Some(servers) = root.get("mcpServers") else {
        return Ok(HostState::NotConfigured);
    };
    let servers = servers
        .as_object()
        .ok_or_else(|| "agent.setup_config_malformed: mcpServers must be an object.".to_string())?;
    let Some(entry) = servers.get(MANAGED_SERVER_NAME) else {
        return Ok(HostState::NotConfigured);
    };
    if entry == expected {
        return Ok(HostState::Configured);
    }
    let managed_args = mcp_arguments(HostKind::ClaudeDesktop);
    let same_args = entry
        .get("args")
        .and_then(Value::as_array)
        .is_some_and(|args| {
            args.iter().filter_map(Value::as_str).collect::<Vec<_>>()
                == managed_args.iter().map(String::as_str).collect::<Vec<_>>()
        });
    let tarik_binary = entry
        .get("command")
        .and_then(Value::as_str)
        .and_then(|command| command.rsplit(['\\', '/']).next())
        .is_some_and(|name| name.eq_ignore_ascii_case("tarik-mcp.exe"));
    if same_args && tarik_binary {
        Ok(HostState::RepairRequired)
    } else {
        Ok(HostState::Conflict)
    }
}

#[cfg(any(windows, test))]
fn inspect_owned_claude_state(
    bytes: &[u8],
    expected: &Value,
    receipt: Option<&SetupReceipt>,
) -> Result<HostState, String> {
    let raw = inspect_claude_state(bytes, expected)?;
    if raw == HostState::NotConfigured || raw == HostState::Conflict {
        return Ok(raw);
    }
    let root = parse_config(bytes)?;
    let entry = root
        .get("mcpServers")
        .and_then(Value::as_object)
        .and_then(|servers| servers.get(MANAGED_SERVER_NAME))
        .ok_or_else(|| "agent.setup_config_conflict: Managed entry disappeared.".to_string())?;
    let owned = receipt.is_some_and(|receipt| {
        receipt.host_kind == HostKind::ClaudeDesktop
            && receipt.managed_entry_hash == hash_value(entry)
            && !receipt.managed_command.is_empty()
            && !receipt.managed_args.is_empty()
    });
    Ok(if owned { raw } else { HostState::Conflict })
}

#[cfg(any(windows, test))]
fn preview_claude_change(
    bytes: &[u8],
    operation: SetupOperation,
    expected: &Value,
    receipt: Option<&SetupReceipt>,
) -> Result<String, String> {
    let state = inspect_owned_claude_state(bytes, expected, receipt)?;
    match (operation, state) {
        (SetupOperation::Configure, HostState::NotConfigured) => Ok(
            "Add one managed tarik entry and preserve every unrelated Claude Desktop setting."
                .into(),
        ),
        (SetupOperation::Configure, HostState::Configured) => Err(
            "agent.setup_already_configured: Claude Desktop already has Tarik's exact entry."
                .into(),
        ),
        (SetupOperation::Configure, HostState::RepairRequired | HostState::Conflict) => Err(
            "agent.setup_conflict: A different tarik entry exists. Use Repair after reviewing it."
                .into(),
        ),
        (SetupOperation::Repair, HostState::RepairRequired) => Ok(
            "Replace only Tarik's prior managed executable path and preserve unrelated settings."
                .into(),
        ),
        (SetupOperation::Repair, _) => {
            Err("agent.setup_not_repairable: No prior Tarik-managed entry requires repair.".into())
        }
        (SetupOperation::Remove, HostState::Configured | HostState::RepairRequired) => {
            Ok("Remove only Tarik's managed entry and preserve every unrelated setting.".into())
        }
        (SetupOperation::Remove, _) => {
            Err("agent.setup_not_managed: Tarik has no removable managed entry.".into())
        }
        _ => Err("agent.setup_state_unsupported: Review the host state again.".into()),
    }
}

fn apply_claude_config(
    path: &Path,
    operation: SetupOperation,
    expected: &Value,
    owned_entry_hash: Option<&str>,
    before_hash: &str,
) -> Result<ConfigApplyResult, String> {
    let before = read_config_bytes(path)?;
    if hash_bytes(&before) != before_hash {
        return Err(
            "agent.setup_config_changed: Claude Desktop config changed after review.".into(),
        );
    }
    let mut root = parse_config(&before)?;
    let current_state = inspect_claude_state(&before, expected)?;
    if matches!(operation, SetupOperation::Repair | SetupOperation::Remove) {
        let current_entry = root
            .get("mcpServers")
            .and_then(Value::as_object)
            .and_then(|servers| servers.get(MANAGED_SERVER_NAME));
        let owned = current_entry
            .map(hash_value)
            .zip(owned_entry_hash)
            .is_some_and(|(actual, expected_hash)| actual == expected_hash);
        if !owned {
            return Err(
                "agent.setup_not_managed: The current Claude Desktop entry does not match Tarik's ownership receipt."
                    .into(),
            );
        }
    }
    match operation {
        SetupOperation::Configure if current_state == HostState::NotConfigured => {
            servers_mut(&mut root)?.insert(MANAGED_SERVER_NAME.into(), expected.clone());
        }
        SetupOperation::Repair if current_state == HostState::RepairRequired => {
            servers_mut(&mut root)?.insert(MANAGED_SERVER_NAME.into(), expected.clone());
        }
        SetupOperation::Remove
            if matches!(
                current_state,
                HostState::Configured | HostState::RepairRequired
            ) =>
        {
            servers_mut(&mut root)?.remove(MANAGED_SERVER_NAME);
        }
        _ => {
            return Err(
                "agent.setup_config_conflict: Host config no longer matches the reviewed plan."
                    .into(),
            )
        }
    }
    let encoded = serde_json::to_vec_pretty(&Value::Object(root))
        .map_err(|error| format!("agent.setup_config_encode_failed: {error}"))?;
    if encoded.len() as u64 > MAX_CONFIG_BYTES {
        return Err("agent.setup_config_too_large: Updated host config exceeds 1 MiB.".into());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "agent.setup_config_unsafe".to_string())?;
    verify_config_parent(parent)?;
    let suffix = uuid::Uuid::new_v4();
    let stage = parent.join(format!(".claude_desktop_config.tarik-{suffix}.tmp"));
    let backup = (!before.is_empty())
        .then(|| parent.join(format!("claude_desktop_config.tarik-{suffix}.backup.json")));
    if let Some(backup) = &backup {
        write_new_synced(backup, &before)
            .map_err(|error| format!("agent.setup_backup_failed: {error}"))?;
    }
    write_new_synced(&stage, &encoded)?;
    if let Err(error) = replace_file(&stage, path) {
        let _ = fs::remove_file(&stage);
        return Err(error);
    }
    let observed = read_config_bytes(path)?;
    if observed != encoded {
        return Err(
            "agent.setup_recovery_required: Updated Claude Desktop config did not verify; Tarik preserved the observed file and retained its backup."
                .into(),
        );
    }
    prune_config_backups(parent, backup.as_deref())?;
    Ok(ConfigApplyResult {
        backup_identity: backup
            .as_ref()
            .map(|path| hash_bytes(path.to_string_lossy().as_bytes())),
    })
}

fn rollback_claude_config(
    path: &Path,
    operation: SetupOperation,
    expected: &Value,
    owned_entry_hash: Option<&str>,
    before: &[u8],
) -> Result<(), String> {
    let expected_after = render_claude_config(before, operation, expected, owned_entry_hash)?;
    let current = read_config_bytes(path)?;
    if current != expected_after {
        return Err(
            "agent.setup_rollback_conflict: Claude Desktop config changed after Tarik applied setup."
                .into(),
        );
    }
    restore_config_bytes(path, before)
}

fn render_claude_config(
    before: &[u8],
    operation: SetupOperation,
    expected: &Value,
    owned_entry_hash: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut root = parse_config(before)?;
    let current_state = inspect_claude_state(before, expected)?;
    if matches!(operation, SetupOperation::Repair | SetupOperation::Remove) {
        let current_entry = root
            .get("mcpServers")
            .and_then(Value::as_object)
            .and_then(|servers| servers.get(MANAGED_SERVER_NAME));
        let owned = current_entry
            .map(hash_value)
            .zip(owned_entry_hash)
            .is_some_and(|(actual, expected_hash)| actual == expected_hash);
        if !owned {
            return Err(
                "agent.setup_not_managed: The current Claude Desktop entry does not match Tarik's ownership receipt."
                    .into(),
            );
        }
    }
    match operation {
        SetupOperation::Configure if current_state == HostState::NotConfigured => {
            servers_mut(&mut root)?.insert(MANAGED_SERVER_NAME.into(), expected.clone());
        }
        SetupOperation::Repair if current_state == HostState::RepairRequired => {
            servers_mut(&mut root)?.insert(MANAGED_SERVER_NAME.into(), expected.clone());
        }
        SetupOperation::Remove
            if matches!(
                current_state,
                HostState::Configured | HostState::RepairRequired
            ) =>
        {
            servers_mut(&mut root)?.remove(MANAGED_SERVER_NAME);
        }
        _ => {
            return Err(
                "agent.setup_config_conflict: Host config no longer matches the reviewed plan."
                    .into(),
            )
        }
    }
    let encoded = serde_json::to_vec_pretty(&Value::Object(root))
        .map_err(|error| format!("agent.setup_config_encode_failed: {error}"))?;
    if encoded.len() as u64 > MAX_CONFIG_BYTES {
        return Err("agent.setup_config_too_large: Updated host config exceeds 1 MiB.".into());
    }
    Ok(encoded)
}

fn restore_config_bytes(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "agent.setup_config_unsafe".to_string())?;
    verify_config_parent(parent)?;
    if bytes.is_empty() {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(format!("agent.setup_rollback_failed: {error}")),
        }
    }
    let stage = parent.join(format!(
        ".claude_desktop_config.tarik-rollback-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    write_new_synced(&stage, bytes)?;
    if let Err(error) = replace_file(&stage, path) {
        let _ = fs::remove_file(&stage);
        return Err(error);
    }
    let restored = read_config_bytes(path)?;
    if restored != bytes {
        return Err("agent.setup_rollback_failed: Restored config did not verify.".into());
    }
    Ok(())
}

fn verify_config_parent(parent: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(parent)
        .map_err(|error| format!("agent.setup_config_directory_failed: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(
            "agent.setup_config_unsafe: Host config parent is not a regular directory.".into(),
        );
    }
    #[cfg(windows)]
    {
        if is_windows_reparse(&metadata) {
            return Err("agent.setup_config_unsafe: Host config parent is a reparse point.".into());
        }
        verify_windows_owner(parent)?;
    }
    Ok(())
}

fn prune_config_backups(parent: &Path, keep: Option<&Path>) -> Result<(), String> {
    let mut backups = fs::read_dir(parent)
        .map_err(|error| format!("agent.setup_backup_read_failed: {error}"))?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            (name.starts_with("claude_desktop_config.tarik-") && name.ends_with(".backup.json"))
                .then(|| entry.path())
        })
        .collect::<Vec<_>>();
    backups.sort_by_key(|path| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok()
    });
    backups.reverse();
    let protected_present = keep.is_some_and(|keep| backups.iter().any(|backup| backup == keep));
    let mut ordinary_retained = 0usize;
    let ordinary_limit = MAX_BACKUPS_PER_HOST - usize::from(protected_present);
    for backup in backups {
        if keep.is_some_and(|keep| keep == backup) {
            continue;
        }
        if ordinary_retained < ordinary_limit {
            ordinary_retained += 1;
            continue;
        }
        fs::remove_file(&backup)
            .map_err(|error| format!("agent.setup_backup_prune_failed: {error}"))?;
    }
    Ok(())
}

fn servers_mut(root: &mut Map<String, Value>) -> Result<&mut Map<String, Value>, String> {
    if !root.contains_key("mcpServers") {
        root.insert("mcpServers".into(), Value::Object(Map::new()));
    }
    root.get_mut("mcpServers")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| "agent.setup_config_malformed: mcpServers must be an object.".into())
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| format!("agent.setup_config_stage_failed: {error}"))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("agent.setup_config_stage_failed: {error}"))
}

#[cfg(not(windows))]
fn replace_file(stage: &Path, target: &Path) -> Result<(), String> {
    fs::rename(stage, target).map_err(|error| format!("agent.setup_atomic_replace_failed: {error}"))
}

#[cfg(windows)]
fn replace_file(stage: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let stage = stage
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let replaced = unsafe {
        MoveFileExW(
            stage.as_ptr(),
            target.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(format!(
            "agent.setup_atomic_replace_failed: {}",
            std::io::Error::last_os_error()
        ))
    } else {
        Ok(())
    }
}

fn hash_value(value: &Value) -> String {
    hash_bytes(&serde_json::to_vec(value).unwrap_or_default())
}

fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[tauri::command]
pub fn get_agent_setup_status(
    manager: tauri::State<'_, AgentSetupManager>,
) -> Result<AgentSetupStatus, String> {
    manager.status()
}

#[tauri::command]
pub fn plan_agent_setup(
    host_kind: HostKind,
    operation: SetupOperation,
    manager: tauri::State<'_, AgentSetupManager>,
) -> Result<HostSetupPlanView, String> {
    manager.plan(host_kind, operation)
}

#[tauri::command]
pub fn apply_agent_setup(
    plan_id: String,
    manager: tauri::State<'_, AgentSetupManager>,
) -> Result<SetupApplyResult, String> {
    manager.apply(&plan_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tarik-agent-setup-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn receipt(host_kind: HostKind, entry: &Value) -> SetupReceipt {
        SetupReceipt {
            receipt_id: "receipt-1".into(),
            host_kind,
            operation: SetupOperation::Configure,
            managed_entry_hash: hash_value(entry),
            executable_identity: "executable-hash".into(),
            managed_command: entry
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            managed_args: entry
                .get("args")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            backup_identity: None,
            completed_at: "2026-09-11T00:00:00Z".into(),
        }
    }

    #[test]
    fn non_windows_hosts_fail_closed_to_guided_setup() {
        if cfg!(windows) {
            return;
        }
        let root = temp_root("guided");
        let mcp = root.join("tarik-mcp");
        fs::write(&mcp, b"fixture").unwrap();
        let manager =
            AgentSetupManager::new(MetadataDb::open_in_memory().unwrap()).with_packaged_mcp(mcp);
        let status = manager.status().unwrap();
        assert!(status.packaged_server_ready);
        assert!(status
            .hosts
            .iter()
            .all(|host| host.setup_method == SetupMethod::Guided));
        let plan = manager
            .plan(HostKind::Pi, SetupOperation::Configure)
            .unwrap();
        assert_eq!(plan.setup_method, SetupMethod::Guided);
        assert!(plan.command_preview.unwrap().contains("--profile pi"));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_npm_hosts_include_direct_native_executables() {
        let app_data = Path::new(r"C:\Users\tester\AppData\Roaming");
        let claude = npm_host_executable_candidates(HostKind::ClaudeCode, app_data);
        assert_eq!(claude.len(), 2);
        assert!(claude[0].ends_with(r"@anthropic-ai\claude-code\bin\claude.exe"));
        assert!(claude[1].ends_with("claude.exe"));

        let codex = npm_host_executable_candidates(HostKind::Codex, app_data);
        assert_eq!(codex.len(), 1);
        assert!(codex[0].ends_with(r"bin\codex.exe"));
        let expected_package = if cfg!(target_arch = "aarch64") {
            "codex-win32-arm64"
        } else {
            "codex-win32-x64"
        };
        assert!(codex[0].to_string_lossy().contains(expected_package));
    }

    #[test]
    fn setup_status_documents_host_owned_non_singleton_topology() {
        let root =
            std::env::temp_dir().join(format!("tarik-setup-status-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let mcp = root.join(if cfg!(windows) {
            "tarik-mcp.exe"
        } else {
            "tarik-mcp"
        });
        std::fs::write(&mcp, b"test executable").unwrap();
        let manager =
            AgentSetupManager::new(MetadataDb::open_in_memory().unwrap()).with_packaged_mcp(mcp);
        let status = manager.status().unwrap();
        assert!(status.topology_note.contains("host transport owns one"));
        assert!(status
            .topology_note
            .contains("does not enforce a machine-wide singleton"));
        if !cfg!(windows) {
            assert!(status
                .duplicate_diagnosis
                .contains("has not been run on this host"));
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn claude_merge_preserves_unrelated_fields_and_exact_remove() {
        let original = br#"{
          "theme": "dark",
          "mcpServers": { "other": { "command": "other" } }
        }"#;
        let expected = json!({
            "command": "C:\\Tarik café\\tarik-mcp.exe",
            "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
        });
        assert_eq!(
            inspect_claude_state(original, &expected).unwrap(),
            HostState::NotConfigured
        );
        let mut root = parse_config(original).unwrap();
        servers_mut(&mut root)
            .unwrap()
            .insert("tarik".into(), expected.clone());
        assert_eq!(root["theme"], "dark");
        assert_eq!(root["mcpServers"]["other"]["command"], "other");
        let configured = serde_json::to_vec(&root).unwrap();
        assert_eq!(
            inspect_claude_state(&configured, &expected).unwrap(),
            HostState::Configured
        );
        servers_mut(&mut root).unwrap().remove("tarik");
        assert_eq!(root["mcpServers"]["other"]["command"], "other");
    }

    #[test]
    fn claude_ownership_requires_receipt_and_allows_only_owned_move_repair() {
        let current = json!({
            "command": "C:\\Tarik new\\tarik-mcp.exe",
            "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
        });
        let current_bytes = serde_json::to_vec(&json!({
            "mcpServers": { "tarik": current }
        }))
        .unwrap();
        assert_eq!(
            inspect_owned_claude_state(&current_bytes, &current, None).unwrap(),
            HostState::Conflict
        );

        let previous = json!({
            "command": "C:\\Tarik old\\tarik-mcp.exe",
            "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
        });
        let previous_bytes = serde_json::to_vec(&json!({
            "mcpServers": { "tarik": previous }
        }))
        .unwrap();
        let prior_receipt = receipt(HostKind::ClaudeDesktop, &previous);
        assert_eq!(
            inspect_owned_claude_state(&previous_bytes, &current, Some(&prior_receipt)).unwrap(),
            HostState::RepairRequired
        );
        assert!(preview_claude_change(
            &previous_bytes,
            SetupOperation::Repair,
            &current,
            Some(&prior_receipt),
        )
        .is_ok());
    }

    #[test]
    fn claude_config_rejects_malformed_conflict_and_concurrent_change() {
        let expected = json!({ "command": "C:\\Tarik\\tarik-mcp.exe", "args": [] });
        assert!(parse_config(b"{").unwrap_err().contains("malformed"));
        let conflict = br#"{"mcpServers":{"tarik":{"command":"foreign"}}}"#;
        assert_eq!(
            inspect_claude_state(conflict, &expected).unwrap(),
            HostState::Conflict
        );
        assert!(
            preview_claude_change(conflict, SetupOperation::Configure, &expected, None,).is_err()
        );

        let root = temp_root("race");
        let path = root.join("claude_desktop_config.json");
        fs::write(&path, b"{}").unwrap();
        let reviewed = hash_bytes(b"{}");
        fs::write(&path, b"{\"changed\":true}").unwrap();
        let error =
            apply_claude_config(&path, SetupOperation::Configure, &expected, None, &reviewed)
                .err()
                .unwrap();
        assert!(error.contains("changed after review"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn config_apply_creates_backup_and_verifies_exact_entry() {
        let root = temp_root("apply");
        let path = root.join("claude_desktop_config.json");
        let original = br#"{"theme":"light","mcpServers":{"other":{"command":"x"}}}"#;
        fs::write(&path, original).unwrap();
        let expected = json!({
            "command": "C:\\Portable Tarik\\tarik-mcp.exe",
            "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
        });
        let result = apply_claude_config(
            &path,
            SetupOperation::Configure,
            &expected,
            None,
            &hash_bytes(original),
        )
        .unwrap();
        assert!(result.backup_identity.is_some());
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            inspect_claude_state(&bytes, &expected).unwrap(),
            HostState::Configured
        );
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["theme"], "light");
        assert_eq!(value["mcpServers"]["other"]["command"], "x");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn claude_receipt_failure_rollback_restores_exact_bytes_and_preserves_concurrent_edits() {
        let root = temp_root("receipt-rollback");
        let path = root.join("claude_desktop_config.json");
        let original = br#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#;
        fs::write(&path, original).unwrap();
        let expected = json!({
            "command": "C:\\Tarik\\tarik-mcp.exe",
            "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
        });
        apply_claude_config(
            &path,
            SetupOperation::Configure,
            &expected,
            None,
            &hash_bytes(original),
        )
        .unwrap();
        rollback_claude_config(&path, SetupOperation::Configure, &expected, None, original)
            .unwrap();
        assert_eq!(fs::read(&path).unwrap(), original);

        apply_claude_config(
            &path,
            SetupOperation::Configure,
            &expected,
            None,
            &hash_bytes(original),
        )
        .unwrap();
        fs::write(&path, br#"{"concurrent":true}"#).unwrap();
        let error =
            rollback_claude_config(&path, SetupOperation::Configure, &expected, None, original)
                .unwrap_err();
        assert!(error.contains("changed after Tarik applied"));
        assert_eq!(fs::read(&path).unwrap(), br#"{"concurrent":true}"#);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn removal_clears_receipt_and_backup_retention_is_bounded() {
        let database = MetadataDb::open_in_memory().unwrap();
        let manager = AgentSetupManager::new(database);
        let entry = json!({
            "command": "C:\\Tarik\\tarik-mcp.exe",
            "args": ["--profile", "claude-desktop", "--label", "Claude Desktop"]
        });
        manager
            .record_receipt(receipt(HostKind::ClaudeDesktop, &entry))
            .unwrap();
        assert_eq!(manager.receipts().unwrap().len(), 1);
        let mut removed = receipt(HostKind::ClaudeDesktop, &entry);
        removed.operation = SetupOperation::Remove;
        manager.record_receipt(removed).unwrap();
        assert!(manager.receipts().unwrap().is_empty());

        let root = temp_root("backups");
        let mut protected = None;
        for index in 0..5 {
            let path = root.join(format!("claude_desktop_config.tarik-{index}.backup.json"));
            fs::write(&path, index.to_string()).unwrap();
            if index == 0 {
                protected = Some(path);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        prune_config_backups(&root, protected.as_deref()).unwrap();
        let retained = fs::read_dir(&root).unwrap().count();
        assert_eq!(retained, MAX_BACKUPS_PER_HOST);
        assert!(protected.unwrap().exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cli_arguments_are_structured_and_use_user_scope_where_supported() {
        let mcp = Path::new("C:\\Tarik folder\\tarik-mcp.exe");
        assert_eq!(
            cli_arguments(HostKind::ClaudeCode, SetupOperation::Configure, mcp),
            vec![
                "mcp",
                "add",
                "--scope",
                "user",
                "tarik",
                "--",
                "C:\\Tarik folder\\tarik-mcp.exe",
                "--profile",
                "claude-code",
                "--label",
                "Claude Code"
            ]
        );
        assert_eq!(
            cli_arguments(HostKind::Codex, SetupOperation::Remove, mcp),
            vec!["mcp", "remove", "tarik"]
        );
    }

    #[test]
    fn host_output_verification_requires_exact_reviewed_shapes() {
        let codex = ExpectedServer {
            host: HostKind::Codex,
            command: "C:\\Tarik\\tarik-mcp.exe".into(),
            args: mcp_arguments(HostKind::Codex),
            expect_present: true,
        };
        let codex_valid = serde_json::to_string(&json!({
            "name": "tarik",
            "enabled": true,
            "transport": {
                "type": "stdio",
                "command": codex.command,
                "args": codex.args,
                "env": null,
                "cwd": null
            }
        }))
        .unwrap();
        assert!(verify_server_output(&codex_valid, &codex));
        assert!(!verify_server_output(
            r#"{"name":"tarik","transport":{"type":"stdio","command":"cmd.exe","args":["/c","tarik"]}}"#,
            &codex
        ));

        let claude = ExpectedServer {
            host: HostKind::ClaudeCode,
            command: "C:\\Tarik folder\\tarik-mcp.exe".into(),
            args: mcp_arguments(HostKind::ClaudeCode),
            expect_present: true,
        };
        let claude_valid = "tarik:\n  Scope: User config\n  Status: Failed to connect\n  Type: stdio\n  Command: C:\\Tarik folder\\tarik-mcp.exe\n  Args: --profile claude-code --label Claude Code\n  Environment:\n";
        assert!(verify_server_output(claude_valid, &claude));
        assert!(!verify_server_output(
            &format!("{claude_valid}  Command: C:\\foreign.exe\n"),
            &claude
        ));

        let codex_absent = ExpectedServer {
            expect_present: false,
            ..codex
        };
        assert!(!verify_server_output("[]", &codex_absent));
        assert!(verify_server_output(
            r#"[{"name":"tarik","transport":{"type":"stdio","command":"x","args":[]}}]"#,
            &codex_absent
        ));
        let claude_absent = ExpectedServer {
            expect_present: false,
            ..claude
        };
        assert!(!verify_server_output(
            "No MCP servers configured. Use `claude mcp add` to add a server.",
            &claude_absent
        ));
        assert!(verify_server_output(
            "tarik: C:\\Tarik\\tarik-mcp.exe --profile claude-code",
            &claude_absent
        ));
    }

    #[test]
    fn one_use_setup_plan_cannot_be_replayed() {
        if cfg!(windows) {
            return;
        }
        let root = temp_root("replay");
        let mcp = root.join("tarik-mcp");
        fs::write(&mcp, b"fixture").unwrap();
        let manager =
            AgentSetupManager::new(MetadataDb::open_in_memory().unwrap()).with_packaged_mcp(mcp);
        let plan = manager
            .plan(HostKind::Generic, SetupOperation::Configure)
            .unwrap();
        assert!(manager.apply(&plan.plan_id).unwrap_err().contains("guided"));
        assert!(manager
            .apply(&plan.plan_id)
            .unwrap_err()
            .contains("missing"));
        fs::remove_dir_all(root).unwrap();
    }
}

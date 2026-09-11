use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[cfg(windows)]
use interprocess::local_socket::GenericNamespaced;
use interprocess::local_socket::{
    prelude::*, GenericFilePath, Listener, ListenerNonblockingMode, ListenerOptions, Name,
};
use serde::{Deserialize, Serialize};
use tarik_agent_protocol::{BridgeAction, BridgeRequest, BridgeResponse, MAX_BRIDGE_MESSAGE_BYTES};

use crate::{
    agent_access::AgentAccessManager,
    agent_destinations::AgentDestinationManager,
    agent_exports::AgentExportManager,
    observability::{AppLogger, EventFields, LogLevel},
};

const DESCRIPTOR_FILE: &str = "bridge.json";
const SOCKET_FILE: &str = "bridge.sock";
const MAX_TRANSPORT_CONNECTIONS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeDescriptor {
    pub protocol_version: u32,
    pub endpoint: String,
    pub transport: &'static str,
    pub instance_id: String,
}

pub struct AgentBridge {
    runtime_dir: PathBuf,
    access: Arc<AgentAccessManager>,
    destinations: Arc<AgentDestinationManager>,
    exports: Arc<AgentExportManager>,
    logger: Arc<AppLogger>,
    runtime: Mutex<Option<BridgeRuntime>>,
}

struct BridgeRuntime {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AgentBridge {
    pub fn new(
        runtime_dir: PathBuf,
        access: Arc<AgentAccessManager>,
        destinations: Arc<AgentDestinationManager>,
        exports: Arc<AgentExportManager>,
        logger: Arc<AppLogger>,
    ) -> Self {
        Self {
            runtime_dir,
            access,
            destinations,
            exports,
            logger,
            runtime: Mutex::new(None),
        }
    }

    pub fn start_if_enabled(&self) -> Result<Option<BridgeDescriptor>, String> {
        if self.access.enabled() {
            self.start().map(Some)
        } else {
            Ok(None)
        }
    }

    pub fn start(&self) -> Result<BridgeDescriptor, String> {
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "agent.bridge_state_unavailable".to_string())?;
        if runtime.is_some() {
            return read_descriptor(&self.runtime_dir);
        }
        if !self.access.enabled() {
            return Err("agent.disabled: Enable Agent Access before starting the bridge.".into());
        }
        prepare_runtime_dir(&self.runtime_dir)?;
        let descriptor = descriptor(&self.runtime_dir);
        let listener = create_listener(&descriptor)?;
        write_descriptor(&self.runtime_dir, &descriptor)?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = stop.clone();
        let access = self.access.clone();
        let destinations = self.destinations.clone();
        let exports = self.exports.clone();
        let logger = self.logger.clone();
        let runtime_dir = self.runtime_dir.clone();
        let thread = thread::Builder::new()
            .name("tarik-agent-bridge".into())
            .spawn(move || {
                run_listener(
                    listener,
                    stop_thread,
                    access,
                    destinations,
                    exports,
                    logger,
                    runtime_dir,
                )
            })
            .map_err(|error| format!("agent.bridge_start_failed: {error}"))?;
        *runtime = Some(BridgeRuntime {
            stop,
            thread: Some(thread),
        });
        self.access.set_endpoint_ready(true);
        Ok(descriptor)
    }

    pub fn stop(&self) {
        let runtime = self
            .runtime
            .lock()
            .ok()
            .and_then(|mut runtime| runtime.take());
        if let Some(mut runtime) = runtime {
            runtime.stop.store(true, Ordering::Release);
            if let Some(thread) = runtime.thread.take() {
                let _ = thread.join();
            }
        }
        self.access.set_endpoint_ready(false);
        cleanup_runtime(&self.runtime_dir);
    }
}

impl Drop for AgentBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_listener(
    listener: Listener,
    stop: Arc<AtomicBool>,
    access: Arc<AgentAccessManager>,
    destinations: Arc<AgentDestinationManager>,
    exports: Arc<AgentExportManager>,
    logger: Arc<AppLogger>,
    runtime_dir: PathBuf,
) {
    logger.record(
        LogLevel::Info,
        "agent",
        "bridge",
        EventFields {
            status: Some("started"),
            ..EventFields::default()
        },
    );
    let active = Arc::new(AtomicUsize::new(0));
    let mut connections: Vec<JoinHandle<()>> = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let mut index = 0;
        while index < connections.len() {
            if connections[index].is_finished() {
                let handle = connections.swap_remove(index);
                let _ = handle.join();
            } else {
                index += 1;
            }
        }
        match listener.accept() {
            Ok(stream) if active.load(Ordering::Acquire) < MAX_TRANSPORT_CONNECTIONS => {
                active.fetch_add(1, Ordering::AcqRel);
                let active_connection = active.clone();
                let stop_connection = stop.clone();
                let access_connection = access.clone();
                let destinations_connection = destinations.clone();
                let exports_connection = exports.clone();
                let logger_connection = logger.clone();
                match thread::Builder::new()
                    .name("tarik-agent-connection".into())
                    .spawn(move || {
                        if let Err(error) = handle_connection(
                            stream,
                            &access_connection,
                            &destinations_connection,
                            &exports_connection,
                            &stop_connection,
                        ) {
                            logger_connection.record(
                                LogLevel::Warning,
                                "agent",
                                "bridge_connection",
                                EventFields {
                                    status: Some("closed"),
                                    error_code: Some("agent.bridge_connection"),
                                    message: Some(&error),
                                    ..EventFields::default()
                                },
                            );
                        }
                        active_connection.fetch_sub(1, Ordering::AcqRel);
                    }) {
                    Ok(handle) => connections.push(handle),
                    Err(_) => {
                        active.fetch_sub(1, Ordering::AcqRel);
                    }
                }
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
            }
            Err(error) => {
                logger.record(
                    LogLevel::Error,
                    "agent",
                    "bridge",
                    EventFields {
                        status: Some("failed"),
                        error_code: Some("agent.bridge_accept"),
                        message: Some(&error.to_string()),
                        ..EventFields::default()
                    },
                );
                break;
            }
        }
    }
    for connection in connections {
        let _ = connection.join();
    }
    access.set_endpoint_ready(false);
    cleanup_runtime(&runtime_dir);
}

fn handle_connection(
    stream: interprocess::local_socket::Stream,
    access: &AgentAccessManager,
    destinations: &AgentDestinationManager,
    exports: &Arc<AgentExportManager>,
    stop: &AtomicBool,
) -> Result<(), String> {
    stream
        .set_recv_timeout(Some(Duration::from_millis(250)))
        .map_err(|error| format!("agent.bridge_timeout: {error}"))?;
    stream
        .set_send_timeout(Some(Duration::from_secs(10)))
        .map_err(|error| format!("agent.bridge_timeout: {error}"))?;
    let mut reader = BufReader::new(stream);
    let mut owned_connections = Vec::new();
    loop {
        let mut bytes = Vec::with_capacity(1024);
        let read = match std::io::Read::by_ref(&mut reader)
            .take((MAX_BRIDGE_MESSAGE_BYTES + 1) as u64)
            .read_until(b'\n', &mut bytes)
        {
            Ok(read) => read,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) && !stop.load(Ordering::Acquire) =>
            {
                continue;
            }
            Err(error) => return Err(format!("agent.bridge_read: {error}")),
        };
        if read == 0 || stop.load(Ordering::Acquire) {
            break;
        }
        if bytes.len() > MAX_BRIDGE_MESSAGE_BYTES {
            return Err("agent.frame_too_large".into());
        }
        while matches!(bytes.last(), Some(b'\n' | b'\r')) {
            bytes.pop();
        }
        let response = match serde_json::from_slice::<BridgeRequest>(&bytes) {
            Ok(request) => {
                let request_id = request.id.clone();
                match request.validate() {
                    Ok(()) => {
                        match dispatch(
                            request.action,
                            access,
                            destinations,
                            exports,
                            &mut owned_connections,
                        ) {
                            Ok(value) => BridgeResponse::success(request_id, value),
                            Err(error) => failure(request_id, error),
                        }
                    }
                    Err(error) => BridgeResponse::failure(
                        request_id,
                        "agent.invalid_request",
                        error.to_string(),
                    ),
                }
            }
            Err(_) => BridgeResponse::failure(
                "invalid",
                "agent.malformed_request",
                "The local agent request was not valid JSON.",
            ),
        };
        let mut encoded = serde_json::to_vec(&response)
            .map_err(|error| format!("agent.bridge_encode: {error}"))?;
        encoded.push(b'\n');
        reader
            .get_mut()
            .write_all(&encoded)
            .map_err(|error| format!("agent.bridge_write: {error}"))?;
    }
    for connection_id in owned_connections {
        let _ = access.disconnect(&connection_id);
    }
    Ok(())
}

fn dispatch(
    action: BridgeAction,
    access: &AgentAccessManager,
    destinations: &AgentDestinationManager,
    exports: &Arc<AgentExportManager>,
    owned_connections: &mut Vec<String>,
) -> Result<serde_json::Value, String> {
    match action {
        BridgeAction::Ping => Ok(serde_json::json!({
            "protocolVersion": tarik_agent_protocol::BRIDGE_PROTOCOL_VERSION,
            "available": access.enabled(),
        })),
        BridgeAction::Hello {
            profile_id,
            label,
            pairing_key,
        } => {
            let pairing_key = pairing_key.map(zeroize::Zeroizing::new);
            let hello = access.hello(
                &profile_id,
                &label,
                pairing_key.as_deref().map(|key| key.as_str()),
            )?;
            owned_connections.push(hello.connection_id.clone());
            serde_json::to_value(hello).map_err(|error| error.to_string())
        }
        BridgeAction::Authenticate {
            connection_id,
            profile_id,
            proof,
        } => serde_json::to_value(access.authenticate(&connection_id, &profile_id, &proof)?)
            .map_err(|error| error.to_string()),
        BridgeAction::Status { connection_id } => {
            serde_json::to_value(access.connection_status(&connection_id)?)
                .map_err(|error| error.to_string())
        }
        BridgeAction::ListProjects { connection_id } => {
            serde_json::to_value(access.list_granted_projects(&connection_id)?)
                .map_err(|error| error.to_string())
        }
        BridgeAction::ListExportDestinations {
            connection_id,
            project_id,
        } => {
            let client_id = access.require_analyze_identity(&connection_id, &project_id)?;
            serde_json::to_value(destinations.list_for_agent(&client_id, &project_id)?)
                .map_err(|error| error.to_string())
        }
        BridgeAction::ProposeExport {
            connection_id,
            intent,
        } => serde_json::to_value(exports.propose(&connection_id, intent)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ExportStatus {
            connection_id,
            export_id,
        } => serde_json::to_value(exports.status(&connection_id, &export_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ExportCancel {
            connection_id,
            export_id,
        } => serde_json::to_value(exports.cancel(&connection_id, &export_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ExportRelease {
            connection_id,
            export_id,
        } => serde_json::to_value(exports.release(&connection_id, &export_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ListCatalog {
            connection_id,
            project_id,
            search,
            cursor,
            limit,
        } => serde_json::to_value(access.list_catalog(
            &connection_id,
            &project_id,
            search.as_deref(),
            cursor.as_deref(),
            limit,
        )?)
        .map_err(|error| error.to_string()),
        BridgeAction::DescribeRelation {
            connection_id,
            project_id,
            database,
            schema,
            name,
            cursor,
            limit,
        } => serde_json::to_value(access.describe_relation(
            &connection_id,
            &project_id,
            &database,
            &schema,
            &name,
            cursor.as_deref(),
            limit,
        )?)
        .map_err(|error| error.to_string()),
        BridgeAction::ClassifySql {
            connection_id,
            project_id,
            sql,
        } => serde_json::to_value(access.classify_sql(&connection_id, &project_id, &sql)?)
            .map_err(|error| error.to_string()),
        BridgeAction::StartQuery {
            connection_id,
            snapshot_id,
        } => serde_json::to_value(access.start_query(&connection_id, &snapshot_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::QueryStatus {
            connection_id,
            execution_id,
        } => serde_json::to_value(access.query_status(&connection_id, &execution_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::CancelQuery {
            connection_id,
            execution_id,
        } => serde_json::to_value(access.cancel_query(&connection_id, &execution_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::GetResultPage {
            connection_id,
            result_id,
            offset,
            max_rows,
        } => serde_json::to_value(access.result_page(
            &connection_id,
            &result_id,
            offset,
            max_rows,
        )?)
        .map_err(|error| error.to_string()),
        BridgeAction::ReleaseResult {
            connection_id,
            result_id,
        } => serde_json::to_value(access.release_result(&connection_id, &result_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ProposeSql {
            connection_id,
            snapshot_id,
        } => serde_json::to_value(access.propose_sql(&connection_id, &snapshot_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ApprovalStatus {
            connection_id,
            approval_id,
        } => serde_json::to_value(access.approval_status(&connection_id, &approval_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ExecuteApproved {
            connection_id,
            approval_id,
        } => serde_json::to_value(access.execute_approved(&connection_id, &approval_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::StartProfile {
            connection_id,
            request,
        } => serde_json::to_value(access.start_profile(&connection_id, request)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ProfileStatus {
            connection_id,
            profile_id,
        } => serde_json::to_value(access.profile_status(&connection_id, &profile_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::CancelProfile {
            connection_id,
            profile_id,
        } => serde_json::to_value(access.cancel_profile(&connection_id, &profile_id)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ExplainSql {
            connection_id,
            snapshot_id,
            actual,
        } => serde_json::to_value(access.explain_sql(&connection_id, &snapshot_id, actual)?)
            .map_err(|error| error.to_string()),
        BridgeAction::ListQuality {
            connection_id,
            project_id,
            offset,
            limit,
        } => {
            serde_json::to_value(access.list_quality(&connection_id, &project_id, offset, limit)?)
                .map_err(|error| error.to_string())
        }
        BridgeAction::ListQualityRuns {
            connection_id,
            project_id,
            check_id,
            offset,
            limit,
        } => serde_json::to_value(access.list_quality_runs(
            &connection_id,
            &project_id,
            check_id.as_deref(),
            offset,
            limit,
        )?)
        .map_err(|error| error.to_string()),
        BridgeAction::ListSavedQueries {
            connection_id,
            project_id,
            offset,
            limit,
        } => serde_json::to_value(access.list_saved_queries(
            &connection_id,
            &project_id,
            offset,
            limit,
        )?)
        .map_err(|error| error.to_string()),
        BridgeAction::Disconnect { connection_id } => {
            owned_connections.retain(|owned| owned != &connection_id);
            Ok(serde_json::json!({ "disconnected": access.disconnect(&connection_id)? }))
        }
    }
}

fn failure(id: String, error: String) -> BridgeResponse {
    let (code, message) = error
        .split_once(": ")
        .map(|(code, message)| (code.to_string(), message.to_string()))
        .unwrap_or_else(|| {
            (
                "agent.request_failed".into(),
                "The request was rejected.".into(),
            )
        });
    BridgeResponse::failure(id, code, message)
}

fn descriptor(runtime_dir: &Path) -> BridgeDescriptor {
    let instance_id = uuid::Uuid::new_v4().to_string();
    #[cfg(unix)]
    let (endpoint, transport) = (
        runtime_dir.join(SOCKET_FILE).to_string_lossy().into_owned(),
        "unix_socket",
    );
    #[cfg(windows)]
    let (endpoint, transport) = (format!("tarik-agent-{instance_id}"), "windows_named_pipe");
    BridgeDescriptor {
        protocol_version: tarik_agent_protocol::BRIDGE_PROTOCOL_VERSION,
        endpoint,
        transport,
        instance_id,
    }
}

fn endpoint_name(descriptor: &BridgeDescriptor) -> Result<Name<'static>, String> {
    #[cfg(unix)]
    {
        PathBuf::from(&descriptor.endpoint)
            .to_fs_name::<GenericFilePath>()
            .map_err(|error| format!("agent.bridge_name: {error}"))
    }
    #[cfg(windows)]
    {
        descriptor
            .endpoint
            .clone()
            .to_ns_name::<GenericNamespaced>()
            .map_err(|error| format!("agent.bridge_name: {error}"))
    }
}

fn create_listener(descriptor: &BridgeDescriptor) -> Result<Listener, String> {
    let options = ListenerOptions::new()
        .name(endpoint_name(descriptor)?)
        .nonblocking(ListenerNonblockingMode::Accept)
        .reclaim_name(true)
        .try_overwrite(false);
    #[cfg(unix)]
    let options = {
        use interprocess::os::unix::local_socket::ListenerOptionsExt;
        options.mode(0o600)
    };
    #[cfg(windows)]
    let options = {
        use interprocess::os::windows::{
            local_socket::ListenerOptionsExt, security_descriptor::SecurityDescriptor,
        };
        use widestring::U16CString;
        let sddl = U16CString::from_str("D:P(A;;GA;;;OW)")
            .map_err(|error| format!("agent.bridge_acl: {error}"))?;
        let descriptor = SecurityDescriptor::deserialize(&sddl)
            .map_err(|error| format!("agent.bridge_acl: {error}"))?;
        options.security_descriptor(descriptor)
    };
    options
        .create_sync()
        .map_err(|error| format!("agent.bridge_bind: {error}"))
}

fn prepare_runtime_dir(path: &Path) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(
                "agent.bridge_unsafe_path: Runtime path is not a private directory.".into(),
            );
        }
    } else {
        fs::create_dir_all(path).map_err(|error| format!("agent.bridge_directory: {error}"))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("agent.bridge_permissions: {error}"))?;
        let metadata =
            fs::metadata(path).map_err(|error| format!("agent.bridge_metadata: {error}"))?;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(
                "agent.bridge_unsafe_permissions: Runtime directory is not user-private.".into(),
            );
        }
    }
    remove_stale_runtime_files(path)?;
    Ok(())
}

fn write_descriptor(path: &Path, descriptor: &BridgeDescriptor) -> Result<(), String> {
    let target = path.join(DESCRIPTOR_FILE);
    let stage = path.join(format!(".{DESCRIPTOR_FILE}.tmp"));
    if fs::symlink_metadata(&target)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("agent.bridge_unsafe_descriptor".into());
    }
    let bytes = serde_json::to_vec(descriptor).map_err(|error| error.to_string())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&stage)
        .map_err(|error| format!("agent.bridge_descriptor: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("agent.bridge_descriptor: {error}"))?;
    fs::rename(stage, target).map_err(|error| format!("agent.bridge_descriptor: {error}"))
}

fn read_descriptor(path: &Path) -> Result<BridgeDescriptor, String> {
    let target = path.join(DESCRIPTOR_FILE);
    let metadata = fs::symlink_metadata(&target)
        .map_err(|error| format!("agent.bridge_descriptor: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("agent.bridge_unsafe_descriptor".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("agent.bridge_unsafe_descriptor_permissions".into());
        }
    }
    let bytes = fs::read(target).map_err(|error| format!("agent.bridge_descriptor: {error}"))?;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct OwnedDescriptor {
        protocol_version: u32,
        endpoint: String,
        transport: String,
        instance_id: String,
    }
    let decoded: OwnedDescriptor = serde_json::from_slice(&bytes)
        .map_err(|error| format!("agent.bridge_descriptor: {error}"))?;
    let transport = match decoded.transport.as_str() {
        "unix_socket" => "unix_socket",
        "windows_named_pipe" => "windows_named_pipe",
        _ => return Err("agent.bridge_descriptor: Unsupported local transport.".into()),
    };
    Ok(BridgeDescriptor {
        protocol_version: decoded.protocol_version,
        endpoint: decoded.endpoint,
        transport,
        instance_id: decoded.instance_id,
    })
}

fn remove_stale_runtime_files(path: &Path) -> Result<(), String> {
    let descriptor = path.join(DESCRIPTOR_FILE);
    let socket = path.join(SOCKET_FILE);
    if descriptor.exists() || socket.exists() {
        #[cfg(unix)]
        if socket.exists() {
            let name = socket
                .to_fs_name::<GenericFilePath>()
                .map_err(|error| format!("agent.bridge_name: {error}"))?;
            match interprocess::local_socket::Stream::connect(name) {
                Ok(_) => {
                    return Err(
                        "agent.bridge_in_use: Another Tarik agent bridge is already active.".into(),
                    )
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionRefused
                            | std::io::ErrorKind::NotFound
                            | std::io::ErrorKind::ConnectionReset
                    ) => {}
                Err(error) => return Err(format!("agent.bridge_stale_check: {error}")),
            }
        }
        #[cfg(windows)]
        return Err(
            "agent.bridge_in_use: An existing bridge descriptor must be removed by Tarik shutdown."
                .into(),
        );
    }
    cleanup_runtime(path);
    Ok(())
}

fn cleanup_runtime(path: &Path) {
    for name in [
        DESCRIPTOR_FILE,
        &format!(".{DESCRIPTOR_FILE}.tmp"),
        SOCKET_FILE,
    ] {
        let target = path.join(name);
        if fs::symlink_metadata(&target)
            .ok()
            .is_some_and(|metadata| !metadata.file_type().is_symlink())
        {
            let _ = fs::remove_file(target);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufRead, BufReader, Write},
        path::PathBuf,
    };

    use tarik_agent_protocol::ProjectGrant;

    use crate::{
        agent_access::{challenge_proof, derive_verifier, hex, parse_hex_array},
        agent_destinations::{AgentDestinationManager, DestinationPolicyInput},
        agent_exports::AgentExportManager,
        engine_manager::EngineManager,
        export::ExportCoordinator,
        metadata::{projects::ProjectOwnership, MetadataDb},
        projects::{ActiveProject, ProjectManager},
    };

    use super::*;

    fn fixture() -> (Arc<AgentAccessManager>, AgentBridge, PathBuf) {
        let database = MetadataDb::open_in_memory().unwrap();
        let engine = Arc::new(EngineManager::new(
            PathBuf::from("unused"),
            std::env::temp_dir().join("tarik-bridge-results"),
        ));
        let projects = ProjectManager::new(database.clone(), std::env::temp_dir(), engine.clone());
        let root =
            std::env::temp_dir().join(format!("tarik-agent-bridge-{}", uuid::Uuid::new_v4()));
        let logger = Arc::new(AppLogger::open(root.join("logs")));
        let access = Arc::new(AgentAccessManager::new(
            database.clone(),
            projects.clone(),
            engine.clone(),
            logger.clone(),
        ));
        let destinations = Arc::new(AgentDestinationManager::new(
            database.clone(),
            projects,
            Vec::new(),
        ));
        let coordinator = Arc::new(ExportCoordinator::new(engine, database.clone()));
        let exports = Arc::new(AgentExportManager::new(
            database,
            access.clone(),
            destinations.clone(),
            coordinator,
        ));
        let bridge = AgentBridge::new(
            root.join("runtime"),
            access.clone(),
            destinations,
            exports,
            logger,
        );
        (access, bridge, root)
    }

    #[test]
    fn disabled_bridge_cannot_start() {
        let (_, bridge, root) = fixture();
        assert!(bridge.start().unwrap_err().contains("Enable Agent Access"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn destination_dispatch_requires_authenticated_analyze_and_redacts_paths() {
        let database = MetadataDb::open_in_memory().unwrap();
        let root = std::env::temp_dir().join(format!(
            "tarik-agent-bridge-destination-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&root).unwrap();
        let project_store = root.join("project-store");
        let destination_path = root.join("exports");
        fs::create_dir(&project_store).unwrap();
        fs::create_dir(&destination_path).unwrap();
        let project = crate::metadata::projects::ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                &project_store.join("project.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        let engine = Arc::new(EngineManager::new(
            root.join("missing-engine"),
            root.join("results"),
        ));
        let projects = ProjectManager::new(database.clone(), root.join("projects"), engine.clone());
        projects.set_active_for_test(ActiveProject {
            id: project.id.clone(),
            name: project.name,
            duckdb_path: project.duckdb_path.into(),
        });
        let logger = Arc::new(AppLogger::open(root.join("logs")));
        let access = Arc::new(AgentAccessManager::new(
            database.clone(),
            projects.clone(),
            engine.clone(),
            logger,
        ));
        let destinations = Arc::new(AgentDestinationManager::new(
            database.clone(),
            projects,
            Vec::new(),
        ));
        let coordinator = Arc::new(ExportCoordinator::new(engine, database.clone()));
        let exports = Arc::new(AgentExportManager::new(
            database,
            access.clone(),
            destinations.clone(),
            coordinator,
        ));
        let mut owned = Vec::new();
        access.set_enabled(true).unwrap();

        let unauthenticated = dispatch(
            BridgeAction::ListExportDestinations {
                connection_id: "missing".into(),
                project_id: project.id.clone(),
            },
            &access,
            &destinations,
            &exports,
            &mut owned,
        )
        .unwrap_err();
        assert!(unauthenticated.contains("authentication_required"));

        let pairing_key = [7u8; 32];
        let hello = access.hello("", "Pi", Some(&hex(&pairing_key))).unwrap();
        let profile_id = access
            .approve_pairing(hello.pairing_request_id.as_deref().unwrap())
            .unwrap();
        let salt = parse_hex_array::<32>(&hello.salt).unwrap();
        let challenge = parse_hex_array::<32>(&hello.challenge).unwrap();
        let verifier = derive_verifier(&pairing_key, &salt);
        let proof = challenge_proof(&verifier, &hello.connection_id, &challenge).unwrap();
        access
            .authenticate(&hello.connection_id, &profile_id, &hex(&proof))
            .unwrap();

        let ungranted = dispatch(
            BridgeAction::ListExportDestinations {
                connection_id: hello.connection_id.clone(),
                project_id: project.id.clone(),
            },
            &access,
            &destinations,
            &exports,
            &mut owned,
        )
        .unwrap_err();
        assert!(ungranted.contains("permission_denied"));

        access
            .set_project_grant(
                &profile_id,
                ProjectGrant {
                    project_id: project.id.clone(),
                    inspect: true,
                    analyze: true,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        let created = destinations
            .create(
                &profile_id,
                &project.id,
                destination_path.to_str().unwrap(),
                DestinationPolicyInput {
                    display_label: "Agent exports".into(),
                    allow_csv: true,
                    allow_parquet: false,
                    maximum_rows_per_part: 10_000,
                    maximum_total_bytes: 1024 * 1024,
                },
            )
            .unwrap();
        let value = dispatch(
            BridgeAction::ListExportDestinations {
                connection_id: hello.connection_id,
                project_id: project.id,
            },
            &access,
            &destinations,
            &exports,
            &mut owned,
        )
        .unwrap();
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(encoded.contains(&created.destination_id));
        assert!(encoded.contains("Agent exports"));
        assert!(!encoded.contains(destination_path.to_str().unwrap()));
        assert!(!encoded.contains("canonicalPath"));
        assert!(!encoded.contains("directoryIdentity"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bridge_binds_private_endpoint_and_serves_ping() {
        let (access, bridge, root) = fixture();
        access.set_enabled(true).unwrap();
        let descriptor = bridge.start().unwrap();
        let name = endpoint_name(&descriptor).unwrap();
        let mut stream = interprocess::local_socket::Stream::connect(name).unwrap();
        let request = BridgeRequest {
            id: "ping-1".into(),
            protocol_version: tarik_agent_protocol::BRIDGE_PROTOCOL_VERSION,
            action: BridgeAction::Ping,
        };
        let mut encoded = serde_json::to_vec(&request).unwrap();
        encoded.push(b'\n');
        stream.write_all(&encoded).unwrap();
        let mut line = String::new();
        BufReader::new(stream).read_line(&mut line).unwrap();
        let response: BridgeResponse = serde_json::from_str(&line).unwrap();
        assert!(response.ok);
        assert_eq!(response.id, "ping-1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(root.join("runtime"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
            assert_eq!(
                fs::metadata(root.join("runtime").join(DESCRIPTOR_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        bridge.stop();
        assert!(!root.join("runtime").join(DESCRIPTOR_FILE).exists());
        let _ = fs::remove_dir_all(root);
    }
}

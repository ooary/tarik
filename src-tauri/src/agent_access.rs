use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tarik_agent_protocol::{
    AuthenticationResult, ConnectionStatusResult, HelloResult, HelloState, ProjectGrant,
    CHALLENGE_BYTES, PROOF_BYTES,
};
use zeroize::Zeroizing;

use crate::{
    metadata::{
        agent::{AgentClientState, AgentRepository},
        settings::SettingsRepository,
        MetadataDb,
    },
    observability::{AppLogger, EventFields, LogLevel},
    projects::ProjectManager,
};

const MAX_PENDING_PAIRINGS: usize = 4;
const MAX_CONNECTIONS: usize = 4;
const PAIRING_LIFETIME: Duration = Duration::from_secs(5 * 60);
const CHALLENGE_LIFETIME: Duration = Duration::from_secs(60);
const AGENT_ACCESS_ENABLED_KEY: &str = "agent.access.enabled";

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccessStatus {
    pub enabled: bool,
    pub endpoint_ready: bool,
    pub paired_clients: Vec<AgentClientView>,
    pub pending_pairings: Vec<PairingRequestView>,
    pub connected_clients: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentClientView {
    pub id: String,
    pub display_name: String,
    pub connected: bool,
    pub last_connected_at: Option<String>,
    pub grants: Vec<ProjectGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingRequestView {
    pub id: String,
    pub display_name: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccessChange {
    pub enabled: bool,
    pub endpoint_ready: bool,
}

struct PendingPairing {
    id: String,
    display_name: String,
    connection_id: String,
    salt: [u8; 32],
    pairing_key: Zeroizing<[u8; PROOF_BYTES]>,
    created_at: Instant,
}

struct ConnectionRecord {
    profile_id: String,
    challenge: [u8; CHALLENGE_BYTES],
    created_at: Instant,
    authenticated: bool,
}

#[derive(Default)]
struct AccessState {
    enabled: bool,
    endpoint_ready: bool,
    pending: HashMap<String, PendingPairing>,
    connections: HashMap<String, ConnectionRecord>,
}

pub trait AgentResourceCleaner: Send + Sync + 'static {
    fn cleanup_connection(&self, connection_id: &str);
    fn cleanup_all(&self);
}

struct NoopCleaner;
impl AgentResourceCleaner for NoopCleaner {
    fn cleanup_connection(&self, _connection_id: &str) {}
    fn cleanup_all(&self) {}
}

pub struct AgentAccessManager {
    repository: AgentRepository,
    settings: SettingsRepository,
    projects: ProjectManager,
    logger: Arc<AppLogger>,
    state: Mutex<AccessState>,
    cleaner: Arc<dyn AgentResourceCleaner>,
}

impl AgentAccessManager {
    pub fn new(database: MetadataDb, projects: ProjectManager, logger: Arc<AppLogger>) -> Self {
        let settings = SettingsRepository::new(database.clone());
        let enabled = settings
            .get::<bool>(AGENT_ACCESS_ENABLED_KEY)
            .ok()
            .flatten()
            .unwrap_or(false);
        Self {
            repository: AgentRepository::new(database),
            settings,
            projects,
            logger,
            state: Mutex::new(AccessState {
                enabled,
                ..AccessState::default()
            }),
            cleaner: Arc::new(NoopCleaner),
        }
    }

    #[cfg(test)]
    fn with_cleaner(mut self, cleaner: Arc<dyn AgentResourceCleaner>) -> Self {
        self.cleaner = cleaner;
        self
    }

    pub fn set_enabled(&self, enabled: bool) -> Result<AgentAccessChange, String> {
        self.settings
            .set(AGENT_ACCESS_ENABLED_KEY, &enabled)
            .map_err(|error| error.to_string())?;
        let mut state = self.lock()?;
        state.enabled = enabled;
        if !enabled {
            state.endpoint_ready = false;
            state.pending.clear();
            state.connections.clear();
            self.cleaner.cleanup_all();
        }
        self.logger.record(
            LogLevel::Info,
            "agent",
            "access",
            EventFields {
                status: Some(if enabled { "enabled" } else { "disabled" }),
                ..EventFields::default()
            },
        );
        Ok(AgentAccessChange {
            enabled: state.enabled,
            endpoint_ready: state.endpoint_ready,
        })
    }

    pub fn enabled(&self) -> bool {
        self.state
            .lock()
            .map(|state| state.enabled)
            .unwrap_or(false)
    }

    pub fn set_endpoint_ready(&self, ready: bool) {
        if let Ok(mut state) = self.state.lock() {
            state.endpoint_ready = ready && state.enabled;
        }
    }

    pub fn status(&self) -> Result<AgentAccessStatus, String> {
        let mut state = self.lock()?;
        prune(&mut state);
        let clients = self
            .repository
            .list_clients()
            .map_err(|error| error.to_string())?
            .into_iter()
            .filter(|client| client.state == AgentClientState::Paired)
            .map(|client| {
                let connected = state.connections.values().any(|connection| {
                    connection.authenticated && connection.profile_id == client.id
                });
                Ok(AgentClientView {
                    grants: self
                        .repository
                        .list_grants(&client.id)
                        .map_err(|error| error.to_string())?,
                    id: client.id,
                    display_name: client.display_name,
                    connected,
                    last_connected_at: client.last_connected_at,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let pending_pairings = state
            .pending
            .values()
            .map(|pairing| PairingRequestView {
                id: pairing.id.clone(),
                display_name: pairing.display_name.clone(),
                expires_in_seconds: PAIRING_LIFETIME
                    .saturating_sub(pairing.created_at.elapsed())
                    .as_secs(),
            })
            .collect();
        Ok(AgentAccessStatus {
            enabled: state.enabled,
            endpoint_ready: state.endpoint_ready,
            paired_clients: clients,
            pending_pairings,
            connected_clients: state
                .connections
                .values()
                .filter(|connection| connection.authenticated)
                .count(),
        })
    }

    pub fn hello(
        &self,
        requested_profile_id: &str,
        display_name: &str,
        pairing_key_hex: Option<&str>,
    ) -> Result<HelloResult, String> {
        let mut state = self.lock()?;
        prune(&mut state);
        if !state.enabled {
            return Err("agent.disabled: Enable Agent Access in Tarik first.".into());
        }
        if state.connections.len() >= MAX_CONNECTIONS {
            return Err("agent.busy: Too many agent connections are open.".into());
        }
        let connection_id = uuid::Uuid::new_v4().to_string();
        let challenge = random_array::<CHALLENGE_BYTES>()?;
        if requested_profile_id.is_empty() {
            if state.pending.len() >= MAX_PENDING_PAIRINGS {
                return Err(
                    "agent.pairing_limit: Resolve an existing pairing request first.".into(),
                );
            }
            let pairing_key = parse_pairing_key(
                pairing_key_hex.ok_or_else(|| "agent.pairing_key_required".to_string())?,
            )?;
            let profile_id = uuid::Uuid::new_v4().to_string();
            let pairing_id = uuid::Uuid::new_v4().to_string();
            let salt = random_array::<32>()?;
            state.pending.insert(
                pairing_id.clone(),
                PendingPairing {
                    id: pairing_id.clone(),
                    display_name: display_name.trim().to_string(),
                    connection_id: connection_id.clone(),
                    salt,
                    pairing_key: Zeroizing::new(pairing_key),
                    created_at: Instant::now(),
                },
            );
            state.connections.insert(
                connection_id.clone(),
                ConnectionRecord {
                    profile_id: profile_id.clone(),
                    challenge,
                    created_at: Instant::now(),
                    authenticated: false,
                },
            );
            return Ok(HelloResult {
                state: HelloState::PairingRequired,
                connection_id,
                profile_id,
                challenge: hex(&challenge),
                salt: hex(&salt),
                pairing_request_id: Some(pairing_id),
            });
        }

        let client = self
            .repository
            .find_client(requested_profile_id)
            .map_err(|error| error.to_string())?
            .filter(|client| client.state == AgentClientState::Paired)
            .ok_or_else(|| {
                "agent.authentication_failed: Unknown or revoked profile.".to_string()
            })?;
        let salt: [u8; 32] = client
            .secret_salt
            .try_into()
            .map_err(|_| "agent.authentication_failed: Invalid stored verifier.".to_string())?;
        state.connections.insert(
            connection_id.clone(),
            ConnectionRecord {
                profile_id: requested_profile_id.to_string(),
                challenge,
                created_at: Instant::now(),
                authenticated: false,
            },
        );
        Ok(HelloResult {
            state: HelloState::AuthenticationRequired,
            connection_id,
            profile_id: requested_profile_id.to_string(),
            challenge: hex(&challenge),
            salt: hex(&salt),
            pairing_request_id: None,
        })
    }

    pub fn approve_pairing(&self, pairing_id: &str) -> Result<String, String> {
        let (pending, profile_id) = {
            let mut state = self.lock()?;
            prune(&mut state);
            let pending = state.pending.remove(pairing_id).ok_or_else(|| {
                "agent.pairing_missing: The pairing request expired or was denied.".to_string()
            })?;
            let profile_id = state
                .connections
                .get(&pending.connection_id)
                .map(|connection| connection.profile_id.clone())
                .ok_or_else(|| "agent.connection_stale: Pairing connection is gone.".to_string())?;
            (pending, profile_id)
        };
        let verifier = derive_verifier(&pending.pairing_key, &pending.salt);
        if let Err(error) =
            self.repository
                .pair(&profile_id, &pending.display_name, &pending.salt, &verifier)
        {
            if let Ok(mut state) = self.state.lock() {
                state.connections.remove(&pending.connection_id);
            }
            self.cleaner.cleanup_connection(&pending.connection_id);
            return Err(error.to_string());
        }
        self.logger.record(
            LogLevel::Info,
            "agent",
            "pairing",
            EventFields {
                status: Some("approved"),
                ..EventFields::default()
            },
        );
        Ok(profile_id)
    }

    pub fn deny_pairing(&self, pairing_id: &str) -> Result<bool, String> {
        let mut state = self.lock()?;
        let Some(pending) = state.pending.remove(pairing_id) else {
            return Ok(false);
        };
        state.connections.remove(&pending.connection_id);
        self.cleaner.cleanup_connection(&pending.connection_id);
        self.logger.record(
            LogLevel::Info,
            "agent",
            "pairing",
            EventFields {
                status: Some("denied"),
                ..EventFields::default()
            },
        );
        Ok(true)
    }

    pub fn authenticate(
        &self,
        connection_id: &str,
        profile_id: &str,
        proof_hex: &str,
    ) -> Result<AuthenticationResult, String> {
        let client = self
            .repository
            .find_client(profile_id)
            .map_err(|error| error.to_string())?
            .filter(|client| client.state == AgentClientState::Paired)
            .ok_or_else(|| {
                "agent.authentication_failed: Unknown or revoked profile.".to_string()
            })?;
        let proof = parse_hex_array::<PROOF_BYTES>(proof_hex)?;
        {
            let mut state = self.lock()?;
            prune(&mut state);
            let connection = state
                .connections
                .get(connection_id)
                .filter(|connection| connection.profile_id == profile_id)
                .ok_or_else(|| "agent.connection_stale: Start authentication again.".to_string())?;
            let verifier = Zeroizing::new(client.secret_verifier);
            let expected = challenge_proof(&verifier, connection_id, &connection.challenge)?;
            if !constant_time_eq(&expected, &proof) {
                state.connections.remove(connection_id);
                return Err(
                    "agent.authentication_failed: Authentication proof was rejected.".into(),
                );
            }
        }
        if !self
            .repository
            .mark_connected(profile_id)
            .map_err(|error| error.to_string())?
        {
            let _ = self.disconnect(connection_id);
            return Err("agent.authentication_failed: Client was revoked.".into());
        }
        let grants = self
            .repository
            .list_grants(profile_id)
            .map_err(|error| error.to_string())?;
        let mut state = self.lock()?;
        let connection = state
            .connections
            .get_mut(connection_id)
            .filter(|connection| connection.profile_id == profile_id)
            .ok_or_else(|| {
                "agent.connection_stale: Authentication connection is gone.".to_string()
            })?;
        connection.authenticated = true;
        Ok(AuthenticationResult {
            client_profile_id: profile_id.to_string(),
            connection_id: connection_id.to_string(),
            grants,
        })
    }

    pub fn connection_status(&self, connection_id: &str) -> Result<ConnectionStatusResult, String> {
        let mut state = self.lock()?;
        prune(&mut state);
        let Some(connection) = state.connections.get(connection_id) else {
            return Ok(ConnectionStatusResult {
                authenticated: false,
                client_profile_id: None,
                grants: Vec::new(),
            });
        };
        let grants = if connection.authenticated {
            self.repository
                .list_grants(&connection.profile_id)
                .map_err(|error| error.to_string())?
        } else {
            Vec::new()
        };
        Ok(ConnectionStatusResult {
            authenticated: connection.authenticated,
            client_profile_id: connection
                .authenticated
                .then(|| connection.profile_id.clone()),
            grants,
        })
    }

    pub fn disconnect(&self, connection_id: &str) -> Result<bool, String> {
        let removed = self.lock()?.connections.remove(connection_id).is_some();
        if removed {
            self.cleaner.cleanup_connection(connection_id);
        }
        Ok(removed)
    }

    pub fn set_project_grant(
        &self,
        client_id: &str,
        mut grant: ProjectGrant,
    ) -> Result<(), String> {
        let active = self
            .projects
            .active()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "agent.no_active_project: Open the project before granting it.".to_string()
            })?;
        if active.id != grant.project_id {
            return Err(
                "agent.project_mismatch: Only the active visible project can be granted.".into(),
            );
        }
        if grant.modify_data {
            grant.modify_workspace = true;
            grant.analyze = true;
            grant.inspect = true;
        } else if grant.modify_workspace {
            grant.inspect = true;
        }
        if grant.analyze {
            grant.inspect = true;
        }
        self.repository
            .set_grant(client_id, &grant)
            .map_err(|error| error.to_string())
    }

    pub fn revoke_client(&self, client_id: &str) -> Result<bool, String> {
        let changed = self
            .repository
            .revoke(client_id)
            .map_err(|error| error.to_string())?;
        let connection_ids = {
            let mut state = self.lock()?;
            let ids = state
                .connections
                .iter()
                .filter(|(_, connection)| connection.profile_id == client_id)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            state
                .connections
                .retain(|_, connection| connection.profile_id != client_id);
            ids
        };
        for id in connection_ids {
            self.cleaner.cleanup_connection(&id);
        }
        Ok(changed)
    }

    pub fn shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.enabled = false;
            state.endpoint_ready = false;
            state.pending.clear();
            state.connections.clear();
        }
        self.cleaner.cleanup_all();
    }

    fn lock(&self) -> Result<MutexGuard<'_, AccessState>, String> {
        self.state
            .lock()
            .map_err(|_| "agent.state_unavailable".to_string())
    }
}

fn prune(state: &mut AccessState) {
    state
        .pending
        .retain(|_, pairing| pairing.created_at.elapsed() < PAIRING_LIFETIME);
    state.connections.retain(|_, connection| {
        connection.authenticated || connection.created_at.elapsed() < CHALLENGE_LIFETIME
    });
}

pub(crate) fn derive_verifier(key: &[u8; PROOF_BYTES], salt: &[u8; 32]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"tarik-agent-verifier-v1");
    hash.update(salt);
    hash.update(key);
    hash.finalize().into()
}

pub(crate) fn challenge_proof(
    verifier: &[u8],
    connection_id: &str,
    challenge: &[u8; CHALLENGE_BYTES],
) -> Result<[u8; 32], String> {
    let mut mac = HmacSha256::new_from_slice(verifier)
        .map_err(|_| "agent.authentication_failed: Invalid verifier.".to_string())?;
    mac.update(b"tarik-agent-proof-v1");
    mac.update(connection_id.as_bytes());
    mac.update(challenge);
    Ok(mac.finalize().into_bytes().into())
}

fn random_array<const N: usize>() -> Result<[u8; N], String> {
    let mut value = [0u8; N];
    getrandom::fill(&mut value).map_err(|error| format!("agent.entropy_unavailable: {error}"))?;
    Ok(value)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(DIGITS[(byte >> 4) as usize] as char);
        encoded.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn parse_pairing_key(value: &str) -> Result<[u8; PROOF_BYTES], String> {
    parse_hex_with_message(
        value,
        "agent.pairing_key_invalid: The pairing key is invalid.",
    )
}

pub(crate) fn parse_hex_array<const N: usize>(value: &str) -> Result<[u8; N], String> {
    parse_hex_with_message(
        value,
        "agent.authentication_failed: Invalid authentication proof.",
    )
}

fn parse_hex_with_message<const N: usize>(value: &str, message: &str) -> Result<[u8; N], String> {
    if value.len() != N * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(message.into());
    }
    let mut decoded = [0u8; N];
    for (index, slot) in decoded.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| message.to_string())?;
    }
    Ok(decoded)
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (left, right)| difference | (left ^ right))
        == 0
}

#[tauri::command]
pub fn get_agent_access_status(
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<AgentAccessStatus, String> {
    manager.status()
}

#[tauri::command]
pub fn set_agent_access_enabled(
    enabled: bool,
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
    bridge: tauri::State<'_, Arc<crate::agent_bridge::AgentBridge>>,
) -> Result<AgentAccessChange, String> {
    if enabled {
        manager.set_enabled(true)?;
        if let Err(error) = bridge.start() {
            let _ = manager.set_enabled(false);
            return Err(error);
        }
        manager.status().map(|status| AgentAccessChange {
            enabled: status.enabled,
            endpoint_ready: status.endpoint_ready,
        })
    } else {
        bridge.stop();
        manager.set_enabled(false)
    }
}

#[tauri::command]
pub fn approve_agent_pairing(
    pairing_id: String,
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<String, String> {
    manager.approve_pairing(&pairing_id)
}

#[tauri::command]
pub fn deny_agent_pairing(
    pairing_id: String,
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<bool, String> {
    manager.deny_pairing(&pairing_id)
}

#[tauri::command]
pub fn set_agent_project_grant(
    client_id: String,
    grant: ProjectGrant,
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<(), String> {
    manager.set_project_grant(&client_id, grant)
}

#[tauri::command]
pub fn revoke_agent_client(
    client_id: String,
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<bool, String> {
    manager.revoke_client(&client_id)
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use crate::{engine_manager::EngineManager, metadata::projects::ProjectsRepository};

    use super::*;

    struct TrackingCleaner(AtomicUsize);
    impl AgentResourceCleaner for TrackingCleaner {
        fn cleanup_connection(&self, _connection_id: &str) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
        fn cleanup_all(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn fixture() -> (AgentAccessManager, String) {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Agent test",
                std::path::Path::new("/tmp/agent-access.duckdb"),
                crate::metadata::projects::ProjectOwnership::External,
            )
            .unwrap();
        let engine = Arc::new(EngineManager::new(
            PathBuf::from("unused"),
            std::env::temp_dir().join("tarik-agent-results"),
        ));
        let projects = ProjectManager::new(database.clone(), std::env::temp_dir(), engine);
        projects.set_active_for_test(crate::projects::ActiveProject {
            id: project.id.clone(),
            name: project.name,
            duckdb_path: PathBuf::from(project.duckdb_path),
        });
        let logger = Arc::new(AppLogger::open(
            std::env::temp_dir().join(format!("tarik-agent-log-{}", uuid::Uuid::new_v4())),
        ));
        (
            AgentAccessManager::new(database, projects, logger),
            project.id,
        )
    }

    #[test]
    fn disabled_access_rejects_hello_and_disable_cleans_resources() {
        let (manager, _) = fixture();
        assert!(manager.hello("", "Pi", Some(&"01".repeat(32))).is_err());
        let cleaner = Arc::new(TrackingCleaner(AtomicUsize::new(0)));
        let manager = manager.with_cleaner(cleaner.clone());
        manager.set_enabled(true).unwrap();
        manager.set_enabled(false).unwrap();
        assert_eq!(cleaner.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn pairing_requires_local_approval_then_challenge_proof() {
        let (manager, project_id) = fixture();
        manager.set_enabled(true).unwrap();
        let key = [7u8; 32];
        let hello = manager.hello("", "Pi", Some(&hex(&key))).unwrap();
        assert_eq!(hello.state, HelloState::PairingRequired);
        let profile_id = manager
            .approve_pairing(hello.pairing_request_id.as_deref().unwrap())
            .unwrap();
        let stored = manager
            .repository
            .find_client(&profile_id)
            .unwrap()
            .unwrap();
        let expected = challenge_proof(
            &stored.secret_verifier,
            &hello.connection_id,
            &parse_hex_array(&hello.challenge).unwrap(),
        )
        .unwrap();
        let authenticated = manager
            .authenticate(&hello.connection_id, &profile_id, &hex(&expected))
            .unwrap();
        assert_eq!(authenticated.client_profile_id, profile_id);
        assert!(authenticated.grants.is_empty());
        manager
            .set_project_grant(
                &profile_id,
                ProjectGrant {
                    project_id: project_id.clone(),
                    inspect: false,
                    analyze: true,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        let status = manager.connection_status(&hello.connection_id).unwrap();
        assert!(status.authenticated);
        assert_eq!(status.grants[0].project_id, project_id);
        assert!(status.grants[0].inspect);
    }

    #[test]
    fn wrong_proof_invalidates_connection_and_replay_fails() {
        let (manager, _) = fixture();
        manager.set_enabled(true).unwrap();
        let hello = manager.hello("", "Pi", Some(&"07".repeat(32))).unwrap();
        let profile_id = manager
            .approve_pairing(hello.pairing_request_id.as_deref().unwrap())
            .unwrap();
        assert!(manager
            .authenticate(&hello.connection_id, &profile_id, &"00".repeat(32))
            .is_err());
        assert!(manager
            .authenticate(&hello.connection_id, &profile_id, &"00".repeat(32))
            .is_err());
    }

    #[test]
    fn revocation_removes_connections_and_grants() {
        let (manager, project_id) = fixture();
        manager
            .repository
            .pair("client", "Pi", &[1; 32], &[2; 32])
            .unwrap();
        manager
            .repository
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id,
                    inspect: true,
                    analyze: false,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        assert!(manager.revoke_client("client").unwrap());
        assert!(manager.repository.list_grants("client").unwrap().is_empty());
    }
}

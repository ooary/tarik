use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tarik_agent_protocol::{
    AgentExecutionResult, AgentResultPage, ApprovalResult, ApprovalState, AuthenticationResult,
    CatalogPageResult, CatalogRelationSummary, ConnectionStatusResult, GrantedProject,
    GrantedProjectsResult, HelloResult, HelloState, ProjectGrant, RelationColumn,
    RelationDescriptionResult, SqlSnapshotResult, CHALLENGE_BYTES, MAX_AGENT_PAGE_RESPONSE_BYTES,
    MAX_AGENT_RESULT_ROWS, MAX_DISCOVERY_RESPONSE_BYTES, PROOF_BYTES,
};
use zeroize::Zeroizing;

use crate::{
    engine_manager::EngineManager,
    metadata::{
        agent::{AgentAuditRecord, AgentClientState, AgentRepository},
        projects::ProjectsRepository,
        settings::SettingsRepository,
        sources::SourcesRepository,
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
const MAX_SQL_SNAPSHOTS: usize = 32;
const SQL_SNAPSHOT_LIFETIME: Duration = Duration::from_secs(120);
const QUERY_LIFETIME: Duration = Duration::from_secs(60);
const MAX_AGENT_QUERIES_GLOBAL: usize = 4;
const MAX_PENDING_APPROVALS: usize = 16;
const MAX_PENDING_APPROVALS_PER_CLIENT: usize = 4;
const APPROVAL_LIFETIME: Duration = Duration::from_secs(120);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequestView {
    pub id: String,
    pub client_name: String,
    pub project_id: String,
    pub project_name: String,
    pub sql: String,
    pub decision: tarik_engine_protocol::AgentSqlDecision,
    pub reason_code: String,
    pub affected_objects: Vec<String>,
    pub has_top_level_filter: Option<bool>,
    pub snapshot_hash: String,
    pub critical_phrase: Option<String>,
    pub expires_in_seconds: u64,
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

struct SqlSnapshot {
    connection_id: String,
    project_id: String,
    sql: String,
    classification: tarik_engine_protocol::AgentSqlClassification,
    created_at: Instant,
}

struct AgentQuery {
    connection_id: String,
    project_id: String,
    execution_id: String,
    started_at: Instant,
    result_id: Option<String>,
}

struct PendingApproval {
    approval_id: String,
    connection_id: String,
    profile_id: String,
    project_id: String,
    sql: String,
    classification: tarik_engine_protocol::AgentSqlClassification,
    snapshot_hash: String,
    critical_phrase: Option<String>,
    state: ApprovalState,
    created_at: Instant,
}

#[derive(Default)]
struct AccessState {
    enabled: bool,
    endpoint_ready: bool,
    pending: HashMap<String, PendingPairing>,
    connections: HashMap<String, ConnectionRecord>,
    sql_snapshots: HashMap<String, SqlSnapshot>,
    agent_queries: HashMap<String, AgentQuery>,
    approvals: HashMap<String, PendingApproval>,
    profiles: HashMap<String, (String, String)>,
    critical_projects: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct DiscoveryCursor {
    kind: String,
    project_id: String,
    revision: String,
    identity: String,
    offset: usize,
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
    metadata: MetadataDb,
    repository: AgentRepository,
    settings: SettingsRepository,
    projects: ProjectManager,
    engine: Arc<EngineManager>,
    logger: Arc<AppLogger>,
    state: Mutex<AccessState>,
    cleaner: Arc<dyn AgentResourceCleaner>,
    cursor_key: Option<[u8; 32]>,
    mutation_lane: Mutex<()>,
}

impl AgentAccessManager {
    pub fn new(
        database: MetadataDb,
        projects: ProjectManager,
        engine: Arc<EngineManager>,
        logger: Arc<AppLogger>,
    ) -> Self {
        let settings = SettingsRepository::new(database.clone());
        let enabled = settings
            .get::<bool>(AGENT_ACCESS_ENABLED_KEY)
            .ok()
            .flatten()
            .unwrap_or(false);
        Self {
            metadata: database.clone(),
            repository: AgentRepository::new(database),
            settings,
            projects,
            engine,
            logger,
            state: Mutex::new(AccessState {
                enabled,
                ..AccessState::default()
            }),
            cleaner: Arc::new(NoopCleaner),
            cursor_key: random_array::<32>().ok(),
            mutation_lane: Mutex::new(()),
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
            state.sql_snapshots.clear();
            state.agent_queries.clear();
            state.profiles.clear();
            for approval in state
                .approvals
                .values_mut()
                .filter(|approval| approval.state == ApprovalState::Pending)
            {
                approval.state = ApprovalState::Denied;
            }
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

    pub fn list_granted_projects(
        &self,
        connection_id: &str,
    ) -> Result<GrantedProjectsResult, String> {
        let (_, grants) = self.authenticated_connection(connection_id)?;
        let active_id = self
            .projects
            .active()
            .map_err(|error| error.to_string())?
            .map(|project| project.id);
        let repository = ProjectsRepository::new(self.metadata.clone());
        let mut projects = Vec::new();
        for grant in grants.into_iter().filter(|grant| grant.inspect) {
            let Some(project) = repository
                .find(&grant.project_id)
                .map_err(|error| error.to_string())?
            else {
                continue;
            };
            projects.push(GrantedProject {
                active: active_id.as_deref() == Some(project.id.as_str()),
                project_id: project.id,
                name: project.name,
                grant,
            });
        }
        Ok(GrantedProjectsResult { projects })
    }

    pub fn list_catalog(
        &self,
        connection_id: &str,
        project_id: &str,
        search: Option<&str>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<CatalogPageResult, String> {
        self.require_inspect(connection_id, project_id)?;
        let snapshot = self.projects.catalog().map_err(|error| error.to_string())?;
        self.require_inspect(connection_id, project_id)?;
        let offset = self.decode_cursor(cursor, "catalog", project_id, &snapshot.revision, "")?;
        let needle = search
            .map(|value| value.trim().to_lowercase())
            .filter(|value| !value.is_empty());
        let sources = SourcesRepository::new(self.metadata.clone())
            .list_sources(project_id)
            .map_err(|error| error.to_string())?;
        let filtered = snapshot
            .objects
            .iter()
            .filter(|object| {
                needle.as_ref().is_none_or(|needle| {
                    object.name.to_lowercase().contains(needle)
                        || object.schema.to_lowercase().contains(needle)
                        || object.database.to_lowercase().contains(needle)
                })
            })
            .collect::<Vec<_>>();
        if offset > filtered.len() {
            return Err("agent.cursor_stale: Start catalog listing again.".into());
        }
        let end = (offset + limit as usize).min(filtered.len());
        let relations = filtered[offset..end]
            .iter()
            .map(|object| relation_summary(object, &snapshot.columns, &sources))
            .collect::<Vec<_>>();
        let next_cursor = (end < filtered.len())
            .then(|| self.encode_cursor("catalog", project_id, &snapshot.revision, "", end))
            .transpose()?;
        let result = CatalogPageResult {
            project_id: project_id.to_string(),
            catalog_revision: snapshot.revision,
            relations,
            next_cursor,
        };
        ensure_discovery_budget(&result)?;
        self.require_inspect(connection_id, project_id)?;
        Ok(result)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn describe_relation(
        &self,
        connection_id: &str,
        project_id: &str,
        database: &str,
        schema: &str,
        name: &str,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<RelationDescriptionResult, String> {
        self.require_inspect(connection_id, project_id)?;
        let snapshot = self.projects.catalog().map_err(|error| error.to_string())?;
        self.require_inspect(connection_id, project_id)?;
        let identity = format!("{database}\u{0}{schema}\u{0}{name}");
        let object = snapshot
            .objects
            .iter()
            .find(|object| {
                object.database == database && object.schema == schema && object.name == name
            })
            .ok_or_else(|| {
                "agent.relation_missing: The relation is not in the current catalog.".to_string()
            })?;
        let offset = self.decode_cursor(
            cursor,
            "relation",
            project_id,
            &snapshot.revision,
            &identity,
        )?;
        let relation_columns = snapshot
            .columns
            .iter()
            .filter(|column| {
                column.database == database && column.schema == schema && column.object == name
            })
            .collect::<Vec<_>>();
        if offset > relation_columns.len() {
            return Err("agent.cursor_stale: Describe the relation again.".into());
        }
        let end = (offset + limit as usize).min(relation_columns.len());
        let columns = relation_columns[offset..end]
            .iter()
            .map(|column| RelationColumn {
                name: column.name.clone(),
                data_type: column.data_type.clone(),
                position: column.position,
                nullable: column.nullable,
            })
            .collect();
        let sources = SourcesRepository::new(self.metadata.clone())
            .list_sources(project_id)
            .map_err(|error| error.to_string())?;
        let next_cursor = (end < relation_columns.len())
            .then(|| self.encode_cursor("relation", project_id, &snapshot.revision, &identity, end))
            .transpose()?;
        let result = RelationDescriptionResult {
            project_id: project_id.to_string(),
            catalog_revision: snapshot.revision,
            relation: relation_summary(object, &snapshot.columns, &sources),
            columns,
            next_cursor,
        };
        ensure_discovery_budget(&result)?;
        self.require_inspect(connection_id, project_id)?;
        Ok(result)
    }

    pub fn classify_sql(
        &self,
        connection_id: &str,
        project_id: &str,
        sql: &str,
    ) -> Result<SqlSnapshotResult, String> {
        self.require_capability(connection_id, project_id, |grant| grant.analyze, "Analyze")?;
        let sources = self.registered_sources(project_id)?;
        let classification = self.engine.classify_agent_sql(sql, &sources)?;
        self.require_capability(connection_id, project_id, |grant| grant.analyze, "Analyze")?;
        let snapshot_id = uuid::Uuid::new_v4().to_string();
        let mut state = self.lock()?;
        prune(&mut state);
        if state.sql_snapshots.len() >= MAX_SQL_SNAPSHOTS {
            return Err("agent.snapshot_limit: Release or wait for existing SQL snapshots.".into());
        }
        state.sql_snapshots.insert(
            snapshot_id.clone(),
            SqlSnapshot {
                connection_id: connection_id.to_string(),
                project_id: project_id.to_string(),
                sql: sql.to_string(),
                classification: classification.clone(),
                created_at: Instant::now(),
            },
        );
        Ok(SqlSnapshotResult {
            snapshot_id,
            project_id: project_id.to_string(),
            classification,
        })
    }

    pub fn start_query(
        &self,
        connection_id: &str,
        snapshot_id: &str,
    ) -> Result<AgentExecutionResult, String> {
        let (project_id, sql, expected_revision) = {
            let mut state = self.lock()?;
            prune(&mut state);
            if state
                .agent_queries
                .values()
                .any(|query| query.connection_id == connection_id && query.result_id.is_some())
            {
                return Err("agent.result_limit: Release the existing agent result first.".into());
            }
            if state
                .agent_queries
                .values()
                .filter(|query| query.result_id.is_none())
                .count()
                >= MAX_AGENT_QUERIES_GLOBAL
            {
                return Err("agent.query_limit: Too many agent queries are active.".into());
            }
            if state
                .agent_queries
                .values()
                .any(|query| query.connection_id == connection_id && query.result_id.is_none())
            {
                return Err("agent.query_limit: This client already has an active query.".into());
            }
            let snapshot = state.sql_snapshots.remove(snapshot_id).ok_or_else(|| {
                "agent.snapshot_missing: Classify the SQL again before execution.".to_string()
            })?;
            if snapshot.connection_id != connection_id {
                return Err(
                    "agent.snapshot_owner_mismatch: SQL snapshots cannot be transferred.".into(),
                );
            }
            if snapshot.classification.decision != tarik_engine_protocol::AgentSqlDecision::SafeRead
            {
                return Err(
                    "agent.approval_required: This SQL cannot use the SafeRead lane.".into(),
                );
            }
            (
                snapshot.project_id,
                snapshot.sql,
                snapshot.classification.catalog_revision,
            )
        };
        self.require_capability(connection_id, &project_id, |grant| grant.analyze, "Analyze")?;
        let current = self
            .engine
            .classify_agent_sql(&sql, &self.registered_sources(&project_id)?)?;
        if current.decision != tarik_engine_protocol::AgentSqlDecision::SafeRead
            || current.catalog_revision != expected_revision
        {
            return Err(
                "agent.snapshot_stale: Catalog policy changed; classify the SQL again.".into(),
            );
        }
        let execution_id = format!("agent-{}", uuid::Uuid::new_v4());
        self.engine
            .execute_query_bounded(&execution_id, &sql, Some(MAX_AGENT_RESULT_ROWS))?;
        self.lock()?.agent_queries.insert(
            execution_id.clone(),
            AgentQuery {
                connection_id: connection_id.to_string(),
                project_id: project_id.clone(),
                execution_id: execution_id.clone(),
                started_at: Instant::now(),
                result_id: None,
            },
        );
        Ok(AgentExecutionResult {
            execution_id,
            project_id,
            state: "queued".into(),
            duration_ms: 0,
            rows_produced: None,
            rows_affected: None,
            result_id: None,
            row_total: None,
            row_total_exact: None,
            error: None,
        })
    }

    pub fn query_status(
        &self,
        connection_id: &str,
        execution_id: &str,
    ) -> Result<AgentExecutionResult, String> {
        let project_id = self.query_owner(connection_id, execution_id)?.project_id;
        self.require_capability(connection_id, &project_id, |grant| grant.analyze, "Analyze")?;
        let mut status = self.engine.query_status(execution_id)?.ok_or_else(|| {
            "agent.execution_missing: The engine no longer tracks this query.".to_string()
        })?;
        let timed_out = self
            .lock()?
            .agent_queries
            .get(execution_id)
            .is_some_and(|query| query.started_at.elapsed() >= QUERY_LIFETIME);
        if timed_out
            && matches!(
                status.state,
                tarik_engine_protocol::ExecutionState::Queued
                    | tarik_engine_protocol::ExecutionState::Running
            )
        {
            let _ = self.engine.cancel_query(execution_id);
            status = self.engine.query_status(execution_id)?.unwrap_or(status);
        }
        if let Some(result) = &status.result {
            if let Some(query) = self.lock()?.agent_queries.get_mut(execution_id) {
                query.result_id = Some(result.result_id.clone());
            }
        }
        Ok(execution_result(project_id, status))
    }

    pub fn cancel_query(
        &self,
        connection_id: &str,
        execution_id: &str,
    ) -> Result<AgentExecutionResult, String> {
        let project_id = self.query_owner(connection_id, execution_id)?.project_id;
        self.require_capability(connection_id, &project_id, |grant| grant.analyze, "Analyze")?;
        let status = self.engine.cancel_query(execution_id)?.ok_or_else(|| {
            "agent.execution_missing: The engine no longer tracks this query.".to_string()
        })?;
        Ok(execution_result(project_id, status))
    }

    pub fn result_page(
        &self,
        connection_id: &str,
        result_id: &str,
        offset: u64,
        max_rows: u32,
    ) -> Result<AgentResultPage, String> {
        let project_id = self.query_owner(connection_id, result_id)?.project_id;
        self.require_capability(connection_id, &project_id, |grant| grant.analyze, "Analyze")?;
        let page = self.engine.result_page(result_id, offset, max_rows)?;
        if serde_json::to_vec(&page)
            .map_err(|error| error.to_string())?
            .len()
            > MAX_AGENT_PAGE_RESPONSE_BYTES
        {
            return Err("agent.page_too_large: Request fewer rows.".into());
        }
        Ok(AgentResultPage {
            result_id: page
                .get("resultId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or(result_id)
                .to_string(),
            offset: page
                .get("offset")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(offset),
            row_total: page
                .get("rowTotal")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            row_total_exact: page
                .get("rowTotalExact")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            columns: page
                .get("columns")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            rows: page.get("rows").cloned().unwrap_or(serde_json::Value::Null),
            truncated_cells: page
                .get("truncatedCells")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        })
    }

    pub fn release_result(&self, connection_id: &str, result_id: &str) -> Result<bool, String> {
        self.query_owner(connection_id, result_id)?;
        self.engine.release_result(result_id)?;
        self.lock()?.agent_queries.remove(result_id);
        Ok(true)
    }

    pub fn propose_sql(
        &self,
        connection_id: &str,
        snapshot_id: &str,
    ) -> Result<ApprovalResult, String> {
        let (profile_id, _) = self.authenticated_connection(connection_id)?;
        let mut state = self.lock()?;
        prune(&mut state);
        let snapshot = state.sql_snapshots.remove(snapshot_id).ok_or_else(|| {
            "agent.snapshot_missing: Classify the SQL again before proposing it.".to_string()
        })?;
        if snapshot.connection_id != connection_id {
            return Err(
                "agent.snapshot_owner_mismatch: SQL snapshots cannot be transferred.".into(),
            );
        }
        if !matches!(
            snapshot.classification.decision,
            tarik_engine_protocol::AgentSqlDecision::ApprovalRequired
                | tarik_engine_protocol::AgentSqlDecision::CriticalConfirmation
        ) {
            return Err(
                "agent.not_approvable: Safe or blocked SQL cannot create an approval.".into(),
            );
        }
        let pending_global = state
            .approvals
            .values()
            .filter(|approval| approval.state == ApprovalState::Pending)
            .count();
        let pending_client = state
            .approvals
            .values()
            .filter(|approval| {
                approval.state == ApprovalState::Pending && approval.profile_id == profile_id
            })
            .count();
        if pending_global >= MAX_PENDING_APPROVALS
            || pending_client >= MAX_PENDING_APPROVALS_PER_CLIENT
        {
            return Err("agent.approval_limit: Resolve an existing approval request first.".into());
        }
        let approval_id = uuid::Uuid::new_v4().to_string();
        let snapshot_hash = sql_snapshot_hash(
            &snapshot.sql,
            connection_id,
            &profile_id,
            &snapshot.project_id,
            &snapshot.classification,
        );
        let critical_phrase = (snapshot.classification.decision
            == tarik_engine_protocol::AgentSqlDecision::CriticalConfirmation)
            .then(|| format!("APPROVE {}", &approval_id[..8].to_ascii_uppercase()));
        let approval = PendingApproval {
            approval_id: approval_id.clone(),
            connection_id: connection_id.to_string(),
            profile_id,
            project_id: snapshot.project_id,
            sql: snapshot.sql,
            classification: snapshot.classification,
            snapshot_hash,
            critical_phrase,
            state: ApprovalState::Pending,
            created_at: Instant::now(),
        };
        let result = approval_result(&approval);
        state.approvals.insert(approval_id, approval);
        Ok(result)
    }

    pub fn approval_status(
        &self,
        connection_id: &str,
        approval_id: &str,
    ) -> Result<ApprovalResult, String> {
        self.authenticated_connection(connection_id)?;
        let mut state = self.lock()?;
        prune(&mut state);
        let approval = state
            .approvals
            .get(approval_id)
            .filter(|approval| approval.connection_id == connection_id)
            .ok_or_else(|| {
                "agent.approval_missing: The approval is unknown or belongs to another connection."
                    .to_string()
            })?;
        Ok(approval_result(approval))
    }

    pub fn execute_approved(
        &self,
        connection_id: &str,
        approval_id: &str,
    ) -> Result<AgentExecutionResult, String> {
        let _lane = self
            .mutation_lane
            .lock()
            .map_err(|_| "agent.mutation_lane_unavailable".to_string())?;
        let (profile_id, project_id, sql, classification, snapshot_hash) = {
            let (profile_id, _) = self.authenticated_connection(connection_id)?;
            let mut state = self.lock()?;
            prune(&mut state);
            let approval = state.approvals.get_mut(approval_id).ok_or_else(|| {
                "agent.approval_missing: The approval is unknown or expired.".to_string()
            })?;
            if approval.connection_id != connection_id || approval.profile_id != profile_id {
                return Err(
                    "agent.approval_owner_mismatch: Approval cannot be transferred.".into(),
                );
            }
            if approval.created_at.elapsed() >= APPROVAL_LIFETIME {
                approval.state = ApprovalState::Expired;
                return Err("agent.approval_expired: Propose the SQL again.".into());
            }
            if approval.state != ApprovalState::Approved {
                return Err(
                    "agent.approval_not_approved: Approve this request inside Tarik first.".into(),
                );
            }
            approval.state = ApprovalState::Used;
            (
                profile_id,
                approval.project_id.clone(),
                approval.sql.clone(),
                approval.classification.clone(),
                approval.snapshot_hash.clone(),
            )
        };
        if self.lock()?.critical_projects.contains_key(&project_id) {
            self.mark_approval_failed(approval_id);
            return Err("agent.critical_recovery: Close and reopen the project before another agent mutation.".into());
        }
        if let Err(error) = self.require_capability(
            connection_id,
            &project_id,
            |grant| grant.modify_data,
            "Modify data",
        ) {
            self.mark_approval_failed(approval_id);
            return Err(error);
        }
        let current = self
            .engine
            .classify_agent_sql(&sql, &self.registered_sources(&project_id)?)?;
        if current.decision != classification.decision
            || current.catalog_revision != classification.catalog_revision
        {
            self.mark_approval_failed(approval_id);
            self.write_audit(
                &profile_id,
                connection_id,
                approval_id,
                &project_id,
                &classification,
                &snapshot_hash,
                "failed",
                None,
                "not_started",
                Some("agent.snapshot_stale"),
            )?;
            return Err(
                "agent.snapshot_stale: Catalog policy changed; propose the SQL again.".into(),
            );
        }
        let result = self.engine.execute_agent_mutation(
            &sql,
            &classification.catalog_revision,
            &self.registered_sources(&project_id)?,
        );
        match result {
            Ok(result) => {
                if let Err(error) = self.write_audit(
                    &profile_id,
                    connection_id,
                    approval_id,
                    &project_id,
                    &classification,
                    &snapshot_hash,
                    "succeeded",
                    result.rows_affected,
                    "not_needed",
                    None,
                ) {
                    self.mark_approval_failed(approval_id);
                    return Err(format!(
                        "agent.critical_recovery: Mutation committed but terminal audit failed: {error}"
                    ));
                }
                Ok(AgentExecutionResult {
                    execution_id: approval_id.to_string(),
                    project_id,
                    state: "succeeded".into(),
                    duration_ms: 0,
                    rows_produced: None,
                    rows_affected: result.rows_affected,
                    result_id: None,
                    row_total: None,
                    row_total_exact: None,
                    error: None,
                })
            }
            Err(error) => {
                self.mark_approval_failed(approval_id);
                let rollback_failed = error.contains("agent.rollback_failed");
                if rollback_failed {
                    self.lock()?
                        .critical_projects
                        .insert(project_id.clone(), "rollback outcome is ambiguous".into());
                }
                self.write_audit(
                    &profile_id,
                    connection_id,
                    approval_id,
                    &project_id,
                    &classification,
                    &snapshot_hash,
                    "failed",
                    None,
                    if rollback_failed {
                        "failed"
                    } else {
                        "confirmed"
                    },
                    Some(if rollback_failed {
                        "agent.rollback_failed"
                    } else {
                        "agent.mutation_failed"
                    }),
                )?;
                if rollback_failed {
                    Err(format!("agent.critical_recovery: {error}"))
                } else {
                    Err(format!("agent.mutation_failed: {error}"))
                }
            }
        }
    }

    pub fn explain_sql(
        &self,
        connection_id: &str,
        snapshot_id: &str,
        actual: bool,
    ) -> Result<crate::plan::QueryPlan, String> {
        let (project_id, sql, expected_revision) = {
            let mut state = self.lock()?;
            prune(&mut state);
            let snapshot = state.sql_snapshots.remove(snapshot_id).ok_or_else(|| {
                "agent.snapshot_missing: Classify the SQL again before explaining it.".to_string()
            })?;
            if snapshot.connection_id != connection_id
                || snapshot.classification.decision
                    != tarik_engine_protocol::AgentSqlDecision::SafeRead
            {
                return Err("agent.snapshot_owner_mismatch: Only owned SafeRead snapshots can be explained.".into());
            }
            (
                snapshot.project_id,
                snapshot.sql,
                snapshot.classification.catalog_revision,
            )
        };
        self.require_capability(connection_id, &project_id, |grant| grant.analyze, "Analyze")?;
        let current = self
            .engine
            .classify_agent_sql(&sql, &self.registered_sources(&project_id)?)?;
        if current.decision != tarik_engine_protocol::AgentSqlDecision::SafeRead
            || current.catalog_revision != expected_revision
        {
            return Err("agent.snapshot_stale: Catalog policy changed; classify again.".into());
        }
        crate::plan::capture_and_normalize_with_limit(
            &self.engine,
            &sql,
            if actual {
                crate::plan::PlanMode::Profile
            } else {
                crate::plan::PlanMode::Explain
            },
            if actual { 2_400 } else { 400 },
        )
    }

    pub fn start_profile(
        &self,
        connection_id: &str,
        request: tarik_engine_protocol::ProfileRequest,
    ) -> Result<tarik_engine_protocol::ProfileStatus, String> {
        self.require_capability(
            connection_id,
            &request.project_id,
            |grant| grant.analyze,
            "Analyze",
        )?;
        let catalog = self.projects.catalog().map_err(|error| error.to_string())?;
        let object = catalog.objects.iter().find(|object| {
            object.database == request.target.database
                && object.schema == request.target.schema
                && object.name == request.target.name
                && object.kind == request.target.kind
        });
        if object.is_none() || catalog.revision != request.catalog_revision {
            return Err("agent.profile_target_stale: Refresh catalog metadata first.".into());
        }
        let profile_id = format!("agent-profile-{}", uuid::Uuid::new_v4());
        self.engine.execute_profile(&profile_id, &request)?;
        self.lock()?.profiles.insert(
            profile_id.clone(),
            (connection_id.to_string(), request.project_id.clone()),
        );
        Ok(tarik_engine_protocol::ProfileStatus {
            profile_id,
            state: tarik_engine_protocol::ProfileState::Queued,
            duration_ms: 0,
            snapshot: None,
            error: None,
        })
    }

    pub fn profile_status(
        &self,
        connection_id: &str,
        profile_id: &str,
    ) -> Result<tarik_engine_protocol::ProfileStatus, String> {
        let project_id = self.profile_owner(connection_id, profile_id)?;
        self.require_capability(connection_id, &project_id, |grant| grant.analyze, "Analyze")?;
        let status = self.engine.profile_status(profile_id)?.ok_or_else(|| {
            "agent.profile_missing: This profile is no longer tracked.".to_string()
        })?;
        if matches!(
            status.state,
            tarik_engine_protocol::ProfileState::Succeeded
                | tarik_engine_protocol::ProfileState::Failed
                | tarik_engine_protocol::ProfileState::Cancelled
        ) {
            self.lock()?.profiles.remove(profile_id);
        }
        Ok(status)
    }

    pub fn cancel_profile(
        &self,
        connection_id: &str,
        profile_id: &str,
    ) -> Result<tarik_engine_protocol::ProfileStatus, String> {
        self.profile_owner(connection_id, profile_id)?;
        self.engine
            .cancel_profile(profile_id)?
            .ok_or_else(|| "agent.profile_missing: This profile is no longer tracked.".to_string())
    }

    pub fn list_quality(
        &self,
        connection_id: &str,
        project_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value, String> {
        self.require_capability(connection_id, project_id, |grant| grant.inspect, "Inspect")?;
        let definitions = crate::metadata::quality::QualityRepository::new(self.metadata.clone())
            .list(project_id)
            .map_err(|error| error.to_string())?;
        let start = (offset as usize).min(definitions.len());
        let end = (start + limit.min(100) as usize).min(definitions.len());
        ensure_discovery_budget(&definitions[start..end])?;
        Ok(serde_json::json!({
            "projectId": project_id,
            "definitions": &definitions[start..end],
            "offset": offset,
            "nextOffset": (end < definitions.len()).then_some(end as u32),
        }))
    }

    pub fn list_quality_runs(
        &self,
        connection_id: &str,
        project_id: &str,
        check_id: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<crate::metadata::quality::CheckHistoryPage, String> {
        self.require_capability(connection_id, project_id, |grant| grant.inspect, "Inspect")?;
        crate::metadata::quality::QualityRepository::new(self.metadata.clone())
            .history(project_id, check_id, offset, limit)
            .map_err(|error| error.to_string())
    }

    pub fn list_saved_queries(
        &self,
        connection_id: &str,
        project_id: &str,
        offset: u32,
        limit: u32,
    ) -> Result<serde_json::Value, String> {
        self.require_capability(
            connection_id,
            project_id,
            |grant| grant.modify_workspace,
            "Modify workspace",
        )?;
        let queries = crate::metadata::queries::QueriesRepository::new(self.metadata.clone())
            .list_saved(project_id, None)
            .map_err(|error| error.to_string())?;
        let start = (offset as usize).min(queries.len());
        let end = (start + limit.min(100) as usize).min(queries.len());
        ensure_discovery_budget(&queries[start..end])?;
        Ok(serde_json::json!({
            "projectId": project_id,
            "queries": &queries[start..end],
            "offset": offset,
            "nextOffset": (end < queries.len()).then_some(end as u32),
        }))
    }

    pub fn list_pending_approvals(&self) -> Result<Vec<ApprovalRequestView>, String> {
        let mut state = self.lock()?;
        prune(&mut state);
        let projects = ProjectsRepository::new(self.metadata.clone());
        let clients = self
            .repository
            .list_clients()
            .map_err(|error| error.to_string())?;
        let mut approvals = Vec::new();
        for approval in state
            .approvals
            .values()
            .filter(|approval| approval.state == ApprovalState::Pending)
        {
            let Some(project) = projects
                .find(&approval.project_id)
                .map_err(|error| error.to_string())?
            else {
                continue;
            };
            let client_name = clients
                .iter()
                .find(|client| client.id == approval.profile_id)
                .map(|client| client.display_name.clone())
                .unwrap_or_else(|| "Revoked client".into());
            approvals.push(ApprovalRequestView {
                id: approval.approval_id.clone(),
                client_name,
                project_id: approval.project_id.clone(),
                project_name: project.name,
                sql: approval.sql.clone(),
                decision: approval.classification.decision,
                reason_code: approval.classification.reason_code.clone(),
                affected_objects: approval.classification.affected_objects.clone(),
                has_top_level_filter: approval.classification.has_top_level_filter,
                snapshot_hash: approval.snapshot_hash.clone(),
                critical_phrase: approval.critical_phrase.clone(),
                expires_in_seconds: APPROVAL_LIFETIME
                    .saturating_sub(approval.created_at.elapsed())
                    .as_secs(),
            });
        }
        approvals.sort_by_key(|approval| approval.expires_in_seconds);
        Ok(approvals)
    }

    fn mark_approval_failed(&self, approval_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(approval) = state.approvals.get_mut(approval_id) {
                approval.state = ApprovalState::Failed;
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn write_audit(
        &self,
        client_id: &str,
        connection_id: &str,
        approval_id: &str,
        project_id: &str,
        classification: &tarik_engine_protocol::AgentSqlClassification,
        snapshot_hash: &str,
        outcome: &str,
        rows_affected: Option<u64>,
        rollback_state: &str,
        error_code: Option<&str>,
    ) -> Result<(), String> {
        self.repository
            .add_audit(&AgentAuditRecord {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                client_id: client_id.to_string(),
                connection_id: connection_id.to_string(),
                approval_id: approval_id.to_string(),
                tool: "tarik_execute_approved".into(),
                risk: format!("{:?}", classification.decision).to_ascii_lowercase(),
                snapshot_hash: snapshot_hash.to_string(),
                decision: "approved".into(),
                outcome: outcome.into(),
                affected_objects: classification.affected_objects.clone(),
                rows_affected,
                rollback_state: rollback_state.into(),
                error_code: error_code.map(ToOwned::to_owned),
                created_at: chrono::Utc::now().to_rfc3339(),
            })
            .map_err(|error| error.to_string())
    }

    pub fn decide_approval(
        &self,
        approval_id: &str,
        approve: bool,
        typed_phrase: Option<&str>,
    ) -> Result<bool, String> {
        let mut state = self.lock()?;
        prune(&mut state);
        let approval = state.approvals.get_mut(approval_id).ok_or_else(|| {
            "agent.approval_missing: The approval expired or was already removed.".to_string()
        })?;
        if approval.state != ApprovalState::Pending
            || approval.created_at.elapsed() >= APPROVAL_LIFETIME
        {
            approval.state = ApprovalState::Expired;
            return Err("agent.approval_terminal: This approval is no longer pending.".into());
        }
        if approve {
            if let Some(expected) = &approval.critical_phrase {
                if typed_phrase != Some(expected.as_str()) {
                    return Err(
                        "agent.confirmation_mismatch: Type the displayed phrase exactly.".into(),
                    );
                }
            }
            approval.state = ApprovalState::Approved;
        } else {
            approval.state = ApprovalState::Denied;
        }
        Ok(approve)
    }

    pub fn disconnect(&self, connection_id: &str) -> Result<bool, String> {
        let (removed, queries, profiles) = {
            let mut state = self.lock()?;
            let removed = state.connections.remove(connection_id).is_some();
            state
                .sql_snapshots
                .retain(|_, snapshot| snapshot.connection_id != connection_id);
            for approval in state.approvals.values_mut().filter(|approval| {
                approval.connection_id == connection_id && approval.state == ApprovalState::Pending
            }) {
                approval.state = ApprovalState::Denied;
            }
            let queries = state
                .agent_queries
                .values()
                .filter(|query| query.connection_id == connection_id)
                .map(|query| query.execution_id.clone())
                .collect::<Vec<_>>();
            state
                .agent_queries
                .retain(|_, query| query.connection_id != connection_id);
            let profiles = state
                .profiles
                .iter()
                .filter(|(_, (owner, _))| owner == connection_id)
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            state
                .profiles
                .retain(|_, (owner, _)| owner != connection_id);
            (removed, queries, profiles)
        };
        for query in queries {
            let _ = self.engine.cancel_query(&query);
            let _ = self.engine.release_result(&query);
        }
        for profile in profiles {
            let _ = self.engine.cancel_profile(&profile);
        }
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
            for approval in state.approvals.values_mut().filter(|approval| {
                approval.profile_id == client_id && approval.state == ApprovalState::Pending
            }) {
                approval.state = ApprovalState::Denied;
            }
            ids
        };
        for id in connection_ids {
            self.cleaner.cleanup_connection(&id);
        }
        Ok(changed)
    }

    pub fn invalidate_project(&self, project_id: &str) {
        let (queries, profiles, approvals) = match self.state.lock() {
            Ok(mut state) => {
                state
                    .sql_snapshots
                    .retain(|_, snapshot| snapshot.project_id != project_id);
                let queries = state
                    .agent_queries
                    .values()
                    .filter(|query| query.project_id == project_id)
                    .map(|query| query.execution_id.clone())
                    .collect::<Vec<_>>();
                state
                    .agent_queries
                    .retain(|_, query| query.project_id != project_id);
                let profiles = state
                    .profiles
                    .iter()
                    .filter(|(_, (_, owner_project))| owner_project == project_id)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>();
                state
                    .profiles
                    .retain(|_, (_, owner_project)| owner_project != project_id);
                state.critical_projects.remove(project_id);
                let mut approvals = 0;
                for approval in state.approvals.values_mut().filter(|approval| {
                    approval.project_id == project_id
                        && matches!(
                            approval.state,
                            ApprovalState::Pending | ApprovalState::Approved
                        )
                }) {
                    approval.state = ApprovalState::Denied;
                    approvals += 1;
                }
                (queries, profiles, approvals)
            }
            Err(_) => return,
        };
        for query in queries {
            let _ = self.engine.cancel_query(&query);
            let _ = self.engine.release_result(&query);
        }
        for profile in profiles {
            let _ = self.engine.cancel_profile(&profile);
        }
        let _ = approvals;
    }

    pub fn shutdown(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.enabled = false;
            state.endpoint_ready = false;
            state.pending.clear();
            state.connections.clear();
            state.sql_snapshots.clear();
            state.agent_queries.clear();
            state.profiles.clear();
            state.approvals.clear();
        }
        self.cleaner.cleanup_all();
    }

    fn authenticated_connection(
        &self,
        connection_id: &str,
    ) -> Result<(String, Vec<ProjectGrant>), String> {
        let mut state = self.lock()?;
        prune(&mut state);
        if !state.enabled {
            return Err("agent.disabled: Agent Access is disabled.".into());
        }
        let connection = state
            .connections
            .get(connection_id)
            .filter(|connection| connection.authenticated)
            .ok_or_else(|| {
                "agent.authentication_required: Reconnect and authenticate.".to_string()
            })?;
        let profile_id = connection.profile_id.clone();
        drop(state);
        let grants = self
            .repository
            .list_grants(&profile_id)
            .map_err(|error| error.to_string())?;
        Ok((profile_id, grants))
    }

    fn require_inspect(&self, connection_id: &str, project_id: &str) -> Result<(), String> {
        self.require_capability(connection_id, project_id, |grant| grant.inspect, "Inspect")
    }

    fn require_capability(
        &self,
        connection_id: &str,
        project_id: &str,
        allowed: impl Fn(&ProjectGrant) -> bool,
        name: &str,
    ) -> Result<(), String> {
        let (_, grants) = self.authenticated_connection(connection_id)?;
        if !grants
            .iter()
            .any(|grant| grant.project_id == project_id && allowed(grant))
        {
            return Err(format!(
                "agent.permission_denied: {name} is not granted for this project."
            ));
        }
        let active = self
            .projects
            .active()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "agent.project_closed: Open the granted project in Tarik.".to_string()
            })?;
        if active.id != project_id {
            return Err("agent.project_closed: The granted project is not active in Tarik.".into());
        }
        Ok(())
    }

    fn registered_sources(
        &self,
        project_id: &str,
    ) -> Result<Vec<tarik_engine_protocol::AgentRegisteredSource>, String> {
        let catalog = self.projects.catalog().map_err(|error| error.to_string())?;
        let sources = SourcesRepository::new(self.metadata.clone())
            .list_sources(project_id)
            .map_err(|error| error.to_string())?;
        Ok(sources
            .into_iter()
            .filter_map(|source| {
                let object = catalog
                    .objects
                    .iter()
                    .find(|object| object.name == source.duckdb_name)?;
                Some(tarik_engine_protocol::AgentRegisteredSource {
                    source_id: source.id,
                    database: object.database.clone(),
                    schema: object.schema.clone(),
                    name: object.name.clone(),
                    kind: match source.kind {
                        crate::metadata::sources::SourceKind::DuckdbTable => "duckdb_table",
                        crate::metadata::sources::SourceKind::LinkedParquet => "linked_parquet",
                        crate::metadata::sources::SourceKind::LinkedCsv => "linked_csv",
                    }
                    .into(),
                    state: match source.state {
                        crate::metadata::sources::SourceState::Ready => "ready",
                        crate::metadata::sources::SourceState::Missing => "missing",
                        crate::metadata::sources::SourceState::InvalidSchema => "invalid_schema",
                    }
                    .into(),
                })
            })
            .collect())
    }

    fn profile_owner(&self, connection_id: &str, profile_id: &str) -> Result<String, String> {
        self.lock()?
            .profiles
            .get(profile_id)
            .filter(|(owner, _)| owner == connection_id)
            .map(|(_, project_id)| project_id.clone())
            .ok_or_else(|| {
                "agent.profile_owner_mismatch: This profile belongs to another connection."
                    .to_string()
            })
    }

    fn query_owner(&self, connection_id: &str, execution_id: &str) -> Result<AgentQuery, String> {
        let state = self.lock()?;
        let query = state
            .agent_queries
            .get(execution_id)
            .filter(|query| query.connection_id == connection_id)
            .ok_or_else(|| {
                "agent.execution_owner_mismatch: This query belongs to another connection."
                    .to_string()
            })?;
        Ok(AgentQuery {
            connection_id: query.connection_id.clone(),
            project_id: query.project_id.clone(),
            execution_id: query.execution_id.clone(),
            started_at: query.started_at,
            result_id: query.result_id.clone(),
        })
    }

    fn encode_cursor(
        &self,
        kind: &str,
        project_id: &str,
        revision: &str,
        identity: &str,
        offset: usize,
    ) -> Result<String, String> {
        let payload = DiscoveryCursor {
            kind: kind.to_string(),
            project_id: project_id.to_string(),
            revision: revision.to_string(),
            identity: identity.to_string(),
            offset,
        };
        let bytes = serde_json::to_vec(&payload).map_err(|error| error.to_string())?;
        let mut mac = HmacSha256::new_from_slice(self.cursor_key.as_ref().ok_or_else(|| {
            "agent.entropy_unavailable: Discovery cursors are unavailable.".to_string()
        })?)
        .map_err(|_| "agent.cursor_invalid".to_string())?;
        mac.update(b"tarik-agent-cursor-v1");
        mac.update(&bytes);
        let signature = mac.finalize().into_bytes();
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(bytes),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    fn decode_cursor(
        &self,
        cursor: Option<&str>,
        kind: &str,
        project_id: &str,
        revision: &str,
        identity: &str,
    ) -> Result<usize, String> {
        let Some(cursor) = cursor else {
            return Ok(0);
        };
        let (payload, signature) = cursor
            .split_once('.')
            .ok_or_else(|| "agent.cursor_invalid: Start discovery again.".to_string())?;
        let bytes = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| "agent.cursor_invalid: Start discovery again.".to_string())?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| "agent.cursor_invalid: Start discovery again.".to_string())?;
        let mut mac = HmacSha256::new_from_slice(self.cursor_key.as_ref().ok_or_else(|| {
            "agent.entropy_unavailable: Discovery cursors are unavailable.".to_string()
        })?)
        .map_err(|_| "agent.cursor_invalid".to_string())?;
        mac.update(b"tarik-agent-cursor-v1");
        mac.update(&bytes);
        mac.verify_slice(&signature)
            .map_err(|_| "agent.cursor_invalid: Start discovery again.".to_string())?;
        let decoded: DiscoveryCursor = serde_json::from_slice(&bytes)
            .map_err(|_| "agent.cursor_invalid: Start discovery again.".to_string())?;
        if decoded.kind != kind
            || decoded.project_id != project_id
            || decoded.revision != revision
            || decoded.identity != identity
        {
            return Err(
                "agent.cursor_stale: Catalog identity changed; start discovery again.".into(),
            );
        }
        Ok(decoded.offset)
    }

    fn lock(&self) -> Result<MutexGuard<'_, AccessState>, String> {
        self.state
            .lock()
            .map_err(|_| "agent.state_unavailable".to_string())
    }
}

fn approval_result(approval: &PendingApproval) -> ApprovalResult {
    ApprovalResult {
        approval_id: approval.approval_id.clone(),
        project_id: approval.project_id.clone(),
        state: approval.state,
        decision: approval.classification.decision,
        reason_code: approval.classification.reason_code.clone(),
        affected_objects: approval.classification.affected_objects.clone(),
        has_top_level_filter: approval.classification.has_top_level_filter,
        snapshot_hash: approval.snapshot_hash.clone(),
        expires_in_seconds: if approval.state == ApprovalState::Pending {
            APPROVAL_LIFETIME
                .saturating_sub(approval.created_at.elapsed())
                .as_secs()
        } else {
            0
        },
    }
}

fn sql_snapshot_hash(
    sql: &str,
    connection_id: &str,
    profile_id: &str,
    project_id: &str,
    classification: &tarik_engine_protocol::AgentSqlClassification,
) -> String {
    let mut hash = Sha256::new();
    hash.update(b"tarik-agent-sql-snapshot-v1");
    hash.update(sql.as_bytes());
    hash.update([0]);
    hash.update(connection_id.as_bytes());
    hash.update([0]);
    hash.update(profile_id.as_bytes());
    hash.update([0]);
    hash.update(project_id.as_bytes());
    hash.update([0]);
    hash.update(classification.catalog_revision.as_bytes());
    hash.update([classification.decision as u8]);
    hex(&hash.finalize())
}

fn execution_result(
    project_id: String,
    status: tarik_engine_protocol::ExecutionStatus,
) -> AgentExecutionResult {
    AgentExecutionResult {
        execution_id: status.execution_id,
        project_id,
        state: match status.state {
            tarik_engine_protocol::ExecutionState::Queued => "queued",
            tarik_engine_protocol::ExecutionState::Running => "running",
            tarik_engine_protocol::ExecutionState::Succeeded => "succeeded",
            tarik_engine_protocol::ExecutionState::Failed => "failed",
            tarik_engine_protocol::ExecutionState::Cancelled => "cancelled",
        }
        .into(),
        duration_ms: status.duration_ms,
        rows_produced: status.rows_produced,
        rows_affected: status.rows_affected,
        result_id: status
            .result
            .as_ref()
            .map(|result| result.result_id.clone()),
        row_total: status.result.as_ref().map(|result| result.row_count),
        row_total_exact: status.result.as_ref().map(|result| result.row_count_exact),
        error: status.error,
    }
}

fn relation_summary(
    object: &tarik_engine_protocol::CatalogObject,
    columns: &[tarik_engine_protocol::CatalogColumn],
    sources: &[crate::metadata::sources::SourceRecord],
) -> CatalogRelationSummary {
    let source = sources.iter().find(|source| {
        source.duckdb_name == object.name && (object.schema == "main" || object.schema == "public")
    });
    CatalogRelationSummary {
        database: object.database.clone(),
        schema: object.schema.clone(),
        name: object.name.clone(),
        kind: object.kind.clone(),
        estimated_row_count: object.estimated_row_count,
        column_count: columns
            .iter()
            .filter(|column| {
                column.database == object.database
                    && column.schema == object.schema
                    && column.object == object.name
            })
            .count()
            .try_into()
            .unwrap_or(u32::MAX),
        registered_source_id: source.map(|source| source.id.clone()),
        source_kind: source.map(|source| {
            match source.kind {
                crate::metadata::sources::SourceKind::DuckdbTable => "duckdb_table",
                crate::metadata::sources::SourceKind::LinkedParquet => "linked_parquet",
                crate::metadata::sources::SourceKind::LinkedCsv => "linked_csv",
            }
            .to_string()
        }),
        source_state: source.map(|source| {
            match source.state {
                crate::metadata::sources::SourceState::Ready => "ready",
                crate::metadata::sources::SourceState::Missing => "missing",
                crate::metadata::sources::SourceState::InvalidSchema => "invalid_schema",
            }
            .to_string()
        }),
    }
}

fn ensure_discovery_budget(value: &(impl Serialize + ?Sized)) -> Result<(), String> {
    let size = serde_json::to_vec(value)
        .map_err(|error| format!("agent.discovery_encode: {error}"))?
        .len();
    if size > MAX_DISCOVERY_RESPONSE_BYTES {
        Err("agent.response_too_large: Request a smaller discovery page.".into())
    } else {
        Ok(())
    }
}

fn prune(state: &mut AccessState) {
    state
        .pending
        .retain(|_, pairing| pairing.created_at.elapsed() < PAIRING_LIFETIME);
    state.connections.retain(|_, connection| {
        connection.authenticated || connection.created_at.elapsed() < CHALLENGE_LIFETIME
    });
    state
        .sql_snapshots
        .retain(|_, snapshot| snapshot.created_at.elapsed() < SQL_SNAPSHOT_LIFETIME);
    for approval in state.approvals.values_mut().filter(|approval| {
        matches!(
            approval.state,
            ApprovalState::Pending | ApprovalState::Approved
        ) && approval.created_at.elapsed() >= APPROVAL_LIFETIME
    }) {
        approval.state = ApprovalState::Expired;
    }
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

#[tauri::command]
pub fn list_agent_approvals(
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<Vec<ApprovalRequestView>, String> {
    manager.list_pending_approvals()
}

#[tauri::command]
pub fn decide_agent_approval(
    approval_id: String,
    approve: bool,
    typed_phrase: Option<String>,
    manager: tauri::State<'_, Arc<AgentAccessManager>>,
) -> Result<bool, String> {
    manager.decide_approval(&approval_id, approve, typed_phrase.as_deref())
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
        let projects = ProjectManager::new(database.clone(), std::env::temp_dir(), engine.clone());
        projects.set_active_for_test(crate::projects::ActiveProject {
            id: project.id.clone(),
            name: project.name,
            duckdb_path: PathBuf::from(project.duckdb_path),
        });
        let logger = Arc::new(AppLogger::open(
            std::env::temp_dir().join(format!("tarik-agent-log-{}", uuid::Uuid::new_v4())),
        ));
        (
            AgentAccessManager::new(database, projects, engine, logger),
            project.id,
        )
    }

    fn pair_and_authenticate(manager: &AgentAccessManager) -> (String, String) {
        manager.set_enabled(true).unwrap();
        let key = [7u8; 32];
        let hello = manager.hello("", "Pi", Some(&hex(&key))).unwrap();
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
        manager
            .authenticate(&hello.connection_id, &profile_id, &hex(&expected))
            .unwrap();
        (profile_id, hello.connection_id)
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
        let (profile_id, connection_id) = pair_and_authenticate(&manager);
        assert!(manager
            .connection_status(&connection_id)
            .unwrap()
            .grants
            .is_empty());
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
        let status = manager.connection_status(&connection_id).unwrap();
        assert!(status.authenticated);
        assert_eq!(status.grants[0].project_id, project_id);
        assert!(status.grants[0].inspect);
    }

    #[test]
    fn discovery_lists_only_granted_identity_without_paths() {
        let (manager, project_id) = fixture();
        let (profile_id, connection_id) = pair_and_authenticate(&manager);
        assert!(manager
            .list_granted_projects(&connection_id)
            .unwrap()
            .projects
            .is_empty());
        manager
            .set_project_grant(
                &profile_id,
                ProjectGrant {
                    project_id: project_id.clone(),
                    inspect: true,
                    analyze: false,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        let projects = manager.list_granted_projects(&connection_id).unwrap();
        assert_eq!(projects.projects.len(), 1);
        assert_eq!(projects.projects[0].project_id, project_id);
        assert!(projects.projects[0].active);
        let encoded = serde_json::to_string(&projects).unwrap();
        assert!(!encoded.contains("duckdb"));
        assert!(!encoded.contains("/tmp"));
        assert!(manager
            .list_catalog(&connection_id, "another-project", None, None, 10)
            .unwrap_err()
            .contains("permission_denied"));
    }

    #[test]
    fn approval_is_server_held_typed_and_non_transferable() {
        let (manager, project_id) = fixture();
        let (profile_id, connection_id) = pair_and_authenticate(&manager);
        manager
            .set_project_grant(
                &profile_id,
                ProjectGrant {
                    project_id: project_id.clone(),
                    inspect: true,
                    analyze: true,
                    modify_workspace: true,
                    modify_data: true,
                },
            )
            .unwrap();
        manager.lock().unwrap().sql_snapshots.insert(
            "snapshot-1".into(),
            SqlSnapshot {
                connection_id: connection_id.clone(),
                project_id: project_id.clone(),
                sql: "DELETE FROM orders".into(),
                classification: tarik_engine_protocol::AgentSqlClassification {
                    decision: tarik_engine_protocol::AgentSqlDecision::CriticalConfirmation,
                    reason_code: "agent.unfiltered_delete_critical".into(),
                    statement_type: "delete".into(),
                    catalog_revision: "agent-v1-test".into(),
                    affected_objects: vec!["orders".into()],
                    has_top_level_filter: Some(false),
                },
                created_at: Instant::now(),
            },
        );
        let proposed = manager.propose_sql(&connection_id, "snapshot-1").unwrap();
        assert_eq!(proposed.state, ApprovalState::Pending);
        assert!(manager
            .decide_approval(&proposed.approval_id, true, Some("wrong"))
            .is_err());
        let phrase = manager.list_pending_approvals().unwrap()[0]
            .critical_phrase
            .clone()
            .unwrap();
        assert!(manager
            .decide_approval(&proposed.approval_id, true, Some(&phrase))
            .unwrap());
        assert_eq!(
            manager
                .approval_status(&connection_id, &proposed.approval_id)
                .unwrap()
                .state,
            ApprovalState::Approved
        );
        assert!(manager
            .approval_status("another-connection", &proposed.approval_id)
            .is_err());
    }

    #[test]
    fn real_sidecar_workflow_classifies_pages_approves_and_commits_once() {
        let engine_binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/debug")
            .join(if cfg!(windows) {
                "tarik-engine-duckdb.exe"
            } else {
                "tarik-engine-duckdb"
            });
        assert!(engine_binary.exists(), "build the DuckDB engine first");
        let root = std::env::temp_dir().join(format!("tarik-agent-real-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let project_path = root.join("agent.duckdb");
        let database = MetadataDb::open_in_memory().unwrap();
        let project = crate::metadata::projects::ProjectsRepository::new(database.clone())
            .upsert(
                "Agent real",
                &project_path,
                crate::metadata::projects::ProjectOwnership::External,
            )
            .unwrap();
        let engine = Arc::new(EngineManager::new(engine_binary, root.join("results")));
        engine.open_session(&project_path).unwrap();
        let projects = ProjectManager::new(database.clone(), root.clone(), engine.clone());
        projects.set_active_for_test(crate::projects::ActiveProject {
            id: project.id.clone(),
            name: project.name,
            duckdb_path: project_path.clone(),
        });
        let logger = Arc::new(AppLogger::open(root.join("logs")));
        let manager = AgentAccessManager::new(database, projects, engine.clone(), logger);
        let (profile_id, connection_id) = pair_and_authenticate(&manager);
        manager
            .set_project_grant(
                &profile_id,
                ProjectGrant {
                    project_id: project.id.clone(),
                    inspect: true,
                    analyze: true,
                    modify_workspace: true,
                    modify_data: true,
                },
            )
            .unwrap();

        let create = manager
            .classify_sql(
                &connection_id,
                &project.id,
                "CREATE TABLE orders(id INTEGER, amount INTEGER)",
            )
            .unwrap();
        assert_eq!(
            create.classification.decision,
            tarik_engine_protocol::AgentSqlDecision::ApprovalRequired
        );
        let approval = manager
            .propose_sql(&connection_id, &create.snapshot_id)
            .unwrap();
        manager
            .decide_approval(&approval.approval_id, true, None)
            .unwrap();
        assert_eq!(
            manager
                .execute_approved(&connection_id, &approval.approval_id)
                .unwrap()
                .state,
            "succeeded"
        );
        assert!(manager
            .execute_approved(&connection_id, &approval.approval_id)
            .is_err());

        let insert = manager
            .classify_sql(
                &connection_id,
                &project.id,
                "INSERT INTO orders VALUES (1, 10), (2, 20)",
            )
            .unwrap();
        let insert_approval = manager
            .propose_sql(&connection_id, &insert.snapshot_id)
            .unwrap();
        manager
            .decide_approval(&insert_approval.approval_id, true, None)
            .unwrap();
        manager
            .execute_approved(&connection_id, &insert_approval.approval_id)
            .unwrap();

        let read = manager
            .classify_sql(
                &connection_id,
                &project.id,
                "SELECT id, amount FROM orders ORDER BY id",
            )
            .unwrap();
        assert_eq!(
            read.classification.decision,
            tarik_engine_protocol::AgentSqlDecision::SafeRead
        );
        let queued = manager
            .start_query(&connection_id, &read.snapshot_id)
            .unwrap();
        let terminal = (0..400)
            .find_map(|_| {
                let status = manager
                    .query_status(&connection_id, &queued.execution_id)
                    .unwrap();
                if status.state == "succeeded" {
                    Some(status)
                } else if matches!(status.state.as_str(), "failed" | "cancelled") {
                    panic!("agent read ended as {}", status.state);
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                    None
                }
            })
            .expect("agent query reached terminal state");
        let result_id = terminal.result_id.unwrap();
        let page = manager
            .result_page(&connection_id, &result_id, 0, 10)
            .unwrap();
        assert_eq!(page.rows.as_array().unwrap().len(), 2);
        manager.release_result(&connection_id, &result_id).unwrap();

        let critical = manager
            .classify_sql(&connection_id, &project.id, "DELETE FROM orders")
            .unwrap();
        assert_eq!(
            critical.classification.decision,
            tarik_engine_protocol::AgentSqlDecision::CriticalConfirmation
        );
        let critical = manager
            .propose_sql(&connection_id, &critical.snapshot_id)
            .unwrap();
        let phrase = manager
            .list_pending_approvals()
            .unwrap()
            .into_iter()
            .find(|approval| approval.id == critical.approval_id)
            .unwrap()
            .critical_phrase
            .unwrap();
        manager
            .decide_approval(&critical.approval_id, true, Some(&phrase))
            .unwrap();
        manager
            .execute_approved(&connection_id, &critical.approval_id)
            .unwrap();
        assert!(engine
            .result_page("missing", 0, 1)
            .unwrap_err()
            .contains("result does not exist"));
        assert_eq!(
            manager
                .repository
                .list_audit(&project.id, 10)
                .unwrap()
                .len(),
            3
        );

        manager.shutdown();
        engine.shutdown();
        let _ = std::fs::remove_dir_all(root);
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

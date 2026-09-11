//! Owner-bound guarded exports for authenticated local MCP clients.
//!
//! This module is the only join between server-held SafeRead snapshots,
//! private destination paths, visible Tarik approvals, and E9 streaming.
//! Public responses contain relative file names only.

use std::{
    collections::{HashMap, VecDeque},
    path::Path,
    sync::{Arc, Mutex, MutexGuard, Weak},
    thread,
    time::Duration,
};

use tarik_agent_protocol::{
    AgentExportDecision, AgentExportIntent, AgentExportPartSummary, AgentExportReleaseResult,
    AgentExportState, AgentExportView, AgentParquetCompression, ApprovalState, ExportFormat,
    MAX_AGENT_EXPORT_MANIFEST_BYTES,
};
use tarik_engine_protocol::{
    CsvExportOptions, ErrorEnvelope, ExportOptions, ExportOverwritePolicy, ExportState,
    ParquetCompression, ParquetExportOptions,
};

use crate::{
    agent_access::{AgentAccessManager, AgentResourceCleaner, ConsumedSafeReadSnapshot},
    agent_destinations::AgentDestinationManager,
    export::{ExportCoordinator, ExportView},
    metadata::{
        agent::{AgentAuditRecord, AgentRepository},
        agent_destinations::ExportDestinationRecord,
        MetadataDb,
    },
};

const MAX_ACTIVE_AGENT_EXPORTS: usize = 4;
const MAX_TERMINAL_AGENT_EXPORTS: usize = 256;
const MAX_COLLISION_SCAN_ENTRIES: usize = 10_000;
const POLL_INTERVAL: Duration = Duration::from_millis(150);

struct AgentExportRecord {
    connection_id: String,
    client_id: String,
    project_id: String,
    destination_id: String,
    destination_path: String,
    destination_revision: u64,
    snapshot: Option<ConsumedSafeReadSnapshot>,
    snapshot_hash: String,
    intent: AgentExportIntent,
    decision: AgentExportDecision,
    approval_id: Option<String>,
    approval_claimed: bool,
    overwrite: ExportOverwritePolicy,
    state: AgentExportState,
    coordinator_export_id: Option<String>,
    duration_ms: u64,
    rows_written: u64,
    files_written: u64,
    bytes_written: u64,
    current_part: Option<u64>,
    completed_parts: Vec<AgentExportPartSummary>,
    error: Option<ErrorEnvelope>,
    audit_written: bool,
}

impl AgentExportRecord {
    fn view(&self, export_id: &str) -> AgentExportView {
        AgentExportView {
            export_id: export_id.to_string(),
            project_id: self.project_id.clone(),
            destination_id: self.destination_id.clone(),
            decision: self.decision,
            approval_id: self.approval_id.clone(),
            state: self.state,
            complete_query: true,
            duration_ms: self.duration_ms,
            rows_written: self.rows_written,
            files_written: self.files_written,
            bytes_written: self.bytes_written,
            current_part: self.current_part,
            completed_parts: self.completed_parts.clone(),
            error: self.error.clone(),
        }
    }

    fn active(&self) -> bool {
        matches!(
            self.state,
            AgentExportState::AwaitingApproval
                | AgentExportState::Queued
                | AgentExportState::Running
        )
    }

    fn terminal(&self) -> bool {
        matches!(
            self.state,
            AgentExportState::Succeeded
                | AgentExportState::Failed
                | AgentExportState::Cancelled
                | AgentExportState::RecoveryRequired
        )
    }
}

#[derive(Default)]
struct AgentExportRegistry {
    records: HashMap<String, AgentExportRecord>,
    terminal_order: VecDeque<String>,
}

pub struct AgentExportManager {
    access: Arc<AgentAccessManager>,
    destinations: Arc<AgentDestinationManager>,
    coordinator: Arc<ExportCoordinator>,
    repository: AgentRepository,
    state: Mutex<AgentExportRegistry>,
    proposal_lane: Mutex<()>,
    start_lane: Mutex<()>,
}

impl AgentExportManager {
    pub fn new(
        database: MetadataDb,
        access: Arc<AgentAccessManager>,
        destinations: Arc<AgentDestinationManager>,
        coordinator: Arc<ExportCoordinator>,
    ) -> Self {
        Self {
            access,
            destinations,
            coordinator,
            repository: AgentRepository::new(database),
            state: Mutex::new(AgentExportRegistry::default()),
            proposal_lane: Mutex::new(()),
            start_lane: Mutex::new(()),
        }
    }

    pub fn propose(
        self: &Arc<Self>,
        connection_id: &str,
        intent: AgentExportIntent,
    ) -> Result<AgentExportView, String> {
        intent
            .validate()
            .map_err(|error| format!("agent.invalid_export_intent: {error}"))?;
        let _proposal = self
            .proposal_lane
            .lock()
            .map_err(|_| "agent.export_state_unavailable".to_string())?;
        self.ensure_capacity(connection_id)?;
        let snapshot = self
            .access
            .consume_safe_read_export_snapshot(connection_id, &intent.snapshot_id)?;
        let destination = self.destinations.resolve_for_export(
            &snapshot.client_id,
            &snapshot.project_id,
            &intent.destination_id,
        )?;
        let export_id = uuid::Uuid::new_v4().to_string();
        let (decision, overwrite, reason) = decide(&destination, &intent)?;
        let approval = if decision == AgentExportDecision::Delegated {
            None
        } else {
            let target_summary = format!(
                "{} / {}-part-*.{}",
                destination.display_label,
                intent.base_name,
                format_extension(intent.format)
            );
            Some(self.access.create_export_approval(
                &snapshot,
                &export_id,
                match decision {
                    AgentExportDecision::ApprovalRequired => {
                        tarik_engine_protocol::AgentSqlDecision::ApprovalRequired
                    }
                    AgentExportDecision::CriticalConfirmation => {
                        tarik_engine_protocol::AgentSqlDecision::CriticalConfirmation
                    }
                    AgentExportDecision::Delegated => unreachable!("delegated has no approval"),
                },
                reason,
                &target_summary,
            )?)
        };
        let state = if approval.is_some() {
            AgentExportState::AwaitingApproval
        } else {
            AgentExportState::Queued
        };
        let record = AgentExportRecord {
            connection_id: connection_id.to_string(),
            client_id: snapshot.client_id.clone(),
            project_id: snapshot.project_id.clone(),
            destination_id: destination.id.clone(),
            destination_path: destination.canonical_path.clone(),
            destination_revision: destination.revision,
            snapshot_hash: snapshot.snapshot_hash.clone(),
            snapshot: Some(snapshot),
            intent,
            decision,
            approval_id: approval.map(|approval| approval.approval_id),
            approval_claimed: false,
            overwrite,
            state,
            coordinator_export_id: None,
            duration_ms: 0,
            rows_written: 0,
            files_written: 0,
            bytes_written: 0,
            current_part: None,
            completed_parts: Vec::new(),
            error: None,
            audit_written: false,
        };
        self.lock()?.records.insert(export_id.clone(), record);
        if decision == AgentExportDecision::Delegated {
            if let Err(error) = self.start(&export_id) {
                self.fail_before_start(&export_id, &error);
            }
        } else if let Err(error) = self.spawn_approval_watcher(&export_id) {
            self.fail_before_start(&export_id, &error);
        }
        self.status(connection_id, &export_id)
    }

    pub fn status(
        self: &Arc<Self>,
        connection_id: &str,
        export_id: &str,
    ) -> Result<AgentExportView, String> {
        self.require_owner(connection_id, export_id)?;
        self.access.require_authenticated_identity(connection_id)?;
        self.maybe_start_approved(export_id)?;
        self.sync(export_id)?;
        let view = self
            .lock()?
            .records
            .get(export_id)
            .map(|record| record.view(export_id))
            .ok_or_else(export_missing)?;
        ensure_response_budget(&view)?;
        Ok(view)
    }

    pub fn cancel(
        self: &Arc<Self>,
        connection_id: &str,
        export_id: &str,
    ) -> Result<AgentExportView, String> {
        self.require_owner(connection_id, export_id)?;
        self.access.require_authenticated_identity(connection_id)?;
        let _start = self
            .start_lane
            .lock()
            .map_err(|_| "agent.export_state_unavailable".to_string())?;
        let coordinator_id = {
            let mut state = self.lock()?;
            let record = state
                .records
                .get_mut(export_id)
                .ok_or_else(export_missing)?;
            match record.state {
                AgentExportState::AwaitingApproval => {
                    if let Some(approval_id) = &record.approval_id {
                        self.access.cancel_export_approval(approval_id, export_id);
                    }
                    record.state = AgentExportState::Cancelled;
                    None
                }
                AgentExportState::Queued | AgentExportState::Running => {
                    record.coordinator_export_id.clone()
                }
                _ => None,
            }
        };
        if let Some(coordinator_id) = coordinator_id {
            self.coordinator.cancel(&coordinator_id)?;
            self.sync(export_id)?;
        } else {
            self.finish_terminal(export_id)?;
        }
        drop(_start);
        self.status(connection_id, export_id)
    }

    pub fn release(
        &self,
        connection_id: &str,
        export_id: &str,
    ) -> Result<AgentExportReleaseResult, String> {
        self.require_owner(connection_id, export_id)?;
        self.access.require_authenticated_identity(connection_id)?;
        let coordinator_id = {
            let state = self.lock()?;
            let record = state.records.get(export_id).ok_or_else(export_missing)?;
            if !record.terminal() {
                return Err("agent.export_active: Cancel the export before releasing it.".into());
            }
            record.coordinator_export_id.clone()
        };
        if let Some(coordinator_id) = coordinator_id {
            let _ = self.coordinator.release(&coordinator_id)?;
        }
        let removed = {
            let mut state = self.lock()?;
            state.terminal_order.retain(|id| id != export_id);
            state.records.remove(export_id).is_some()
        };
        Ok(AgentExportReleaseResult {
            export_id: export_id.to_string(),
            released: removed,
        })
    }

    pub(crate) fn invalidate_destination(&self, destination_id: &str) {
        self.cleanup_matching(|record| record.destination_id == destination_id);
    }

    fn ensure_capacity(&self, connection_id: &str) -> Result<(), String> {
        let state = self.lock()?;
        if state
            .records
            .values()
            .any(|record| record.connection_id == connection_id && record.active())
        {
            return Err("agent.export_limit: This connection already has an active export.".into());
        }
        if state
            .records
            .values()
            .filter(|record| record.active())
            .count()
            >= MAX_ACTIVE_AGENT_EXPORTS
        {
            return Err("agent.export_limit: Too many agent exports are active.".into());
        }
        Ok(())
    }

    fn start(self: &Arc<Self>, export_id: &str) -> Result<(), String> {
        let _start = self
            .start_lane
            .lock()
            .map_err(|_| "agent.export_state_unavailable".to_string())?;
        self.start_locked(export_id)
    }

    fn start_locked(self: &Arc<Self>, export_id: &str) -> Result<(), String> {
        let (snapshot, intent, expected_destination_revision, overwrite) = {
            let mut state = self.lock()?;
            let record = state
                .records
                .get_mut(export_id)
                .ok_or_else(export_missing)?;
            if !matches!(
                record.state,
                AgentExportState::Queued | AgentExportState::AwaitingApproval
            ) {
                return Err("agent.export_cancelled: Export authority was revoked.".into());
            }
            let snapshot = record.snapshot.take().ok_or_else(|| {
                "agent.snapshot_missing: Export snapshot was already used.".to_string()
            })?;
            (
                snapshot,
                record.intent.clone(),
                record.destination_revision,
                record.overwrite,
            )
        };
        self.access.revalidate_safe_read_export(
            &snapshot.connection_id,
            &snapshot.project_id,
            &snapshot.sql,
            &snapshot.catalog_revision,
        )?;
        let destination = self.destinations.resolve_for_export(
            &snapshot.client_id,
            &snapshot.project_id,
            &intent.destination_id,
        )?;
        if destination.revision != expected_destination_revision {
            return Err(
                "agent.destination_stale: Destination policy changed; propose the export again."
                    .into(),
            );
        }
        let (current_decision, current_overwrite, _) = decide(&destination, &intent)?;
        let expected_decision = self
            .lock()?
            .records
            .get(export_id)
            .map(|record| record.decision)
            .ok_or_else(export_missing)?;
        if current_decision != expected_decision || current_overwrite != overwrite {
            return Err(
                "agent.export_stale: Destination contents or policy changed; propose the export again."
                    .into(),
            );
        }
        let options = to_engine_options(&destination, &intent, overwrite)?;
        let queued = self.coordinator.execute_bounded(
            &snapshot.project_id,
            &snapshot.sql,
            options,
            destination.maximum_total_bytes,
        )?;
        {
            let mut state = self.lock()?;
            let record = state
                .records
                .get_mut(export_id)
                .ok_or_else(export_missing)?;
            record.coordinator_export_id = Some(queued.export_id.clone());
            record.state = AgentExportState::Queued;
        }
        let manager = Arc::clone(self);
        let id = export_id.to_string();
        if let Err(error) = thread::Builder::new()
            .name(format!("tarik-agent-export-{export_id}"))
            .spawn(move || manager.poll_until_terminal(&id))
        {
            let _ = self.coordinator.cancel(&queued.export_id);
            return Err(format!("agent.export_poller: {error}"));
        }
        Ok(())
    }

    fn maybe_start_approved(self: &Arc<Self>, export_id: &str) -> Result<(), String> {
        let _start = self
            .start_lane
            .lock()
            .map_err(|_| "agent.export_state_unavailable".to_string())?;
        let approval = {
            let state = self.lock()?;
            let record = state.records.get(export_id).ok_or_else(export_missing)?;
            if record.state != AgentExportState::AwaitingApproval {
                return Ok(());
            }
            (
                record.connection_id.clone(),
                record.approval_id.clone().ok_or_else(export_missing)?,
            )
        };
        match self
            .access
            .export_approval_state(&approval.0, &approval.1, export_id)?
        {
            ApprovalState::Approved => {
                self.access
                    .claim_export_approval(&approval.0, &approval.1, export_id)?;
                if let Some(record) = self.lock()?.records.get_mut(export_id) {
                    record.approval_claimed = true;
                }
                if let Err(error) = self.start_locked(export_id) {
                    self.fail_before_start(export_id, &error);
                }
            }
            ApprovalState::Denied | ApprovalState::Expired | ApprovalState::Failed => {
                self.fail_before_start(
                    export_id,
                    "agent.export_not_approved: Tarik did not approve this export.",
                );
            }
            ApprovalState::Pending | ApprovalState::Used => {}
        }
        Ok(())
    }

    fn spawn_approval_watcher(self: &Arc<Self>, export_id: &str) -> Result<(), String> {
        let manager = Arc::clone(self);
        let id = export_id.to_string();
        thread::Builder::new()
            .name(format!("tarik-agent-export-approval-{export_id}"))
            .spawn(move || loop {
                thread::sleep(POLL_INTERVAL);
                if manager.maybe_start_approved(&id).is_err() {
                    manager.fail_before_start(
                        &id,
                        "agent.export_approval_lost: Export approval could not be observed.",
                    );
                    return;
                }
                let waiting = manager
                    .state
                    .lock()
                    .ok()
                    .and_then(|state| {
                        state
                            .records
                            .get(&id)
                            .map(|record| record.state == AgentExportState::AwaitingApproval)
                    })
                    .unwrap_or(false);
                if !waiting {
                    return;
                }
            })
            .map(|_| ())
            .map_err(|error| format!("agent.export_approval_watcher: {error}"))
    }

    fn poll_until_terminal(self: Arc<Self>, export_id: &str) {
        loop {
            thread::sleep(POLL_INTERVAL);
            if self.sync(export_id).is_err() {
                return;
            }
            let terminal = self
                .state
                .lock()
                .ok()
                .and_then(|state| {
                    state
                        .records
                        .get(export_id)
                        .map(AgentExportRecord::terminal)
                })
                .unwrap_or(true);
            if terminal {
                return;
            }
        }
    }

    fn sync(&self, export_id: &str) -> Result<(), String> {
        let coordinator_id = {
            let state = self.lock()?;
            state
                .records
                .get(export_id)
                .ok_or_else(export_missing)?
                .coordinator_export_id
                .clone()
        };
        let Some(coordinator_id) = coordinator_id else {
            return Ok(());
        };
        let Some(status) = self.coordinator.status(&coordinator_id) else {
            self.fail_before_start(
                export_id,
                "agent.export_lost: Tarik no longer tracks this export.",
            );
            return Ok(());
        };
        let became_terminal = {
            let mut state = self.lock()?;
            let record = state
                .records
                .get_mut(export_id)
                .ok_or_else(export_missing)?;
            if let Err(error) = apply_status(record, &status) {
                record.state = AgentExportState::RecoveryRequired;
                record.error = Some(path_free_error("agent.export_manifest_invalid", &error));
            }
            record.terminal() && !record.audit_written
        };
        if became_terminal {
            self.finish_terminal(export_id)?;
        }
        Ok(())
    }

    fn fail_before_start(&self, export_id: &str, error: &str) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(record) = state.records.get_mut(export_id) {
                record.state = AgentExportState::Failed;
                record.error = Some(path_free_error("agent.export_rejected", error));
            }
        }
        let _ = self.finish_terminal(export_id);
    }

    fn finish_terminal(&self, export_id: &str) -> Result<(), String> {
        let audit = {
            let mut state = self.lock()?;
            let record = state
                .records
                .get_mut(export_id)
                .ok_or_else(export_missing)?;
            if record.audit_written {
                return Ok(());
            }
            record.audit_written = true;
            AgentAuditRecord {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: record.project_id.clone(),
                client_id: record.client_id.clone(),
                connection_id: record.connection_id.clone(),
                approval_id: record.approval_id.clone().unwrap_or_default(),
                tool: "tarik_propose_export".into(),
                risk: match record.decision {
                    AgentExportDecision::Delegated => "delegated",
                    AgentExportDecision::ApprovalRequired => "approval_required",
                    AgentExportDecision::CriticalConfirmation => "critical_confirmation",
                }
                .into(),
                snapshot_hash: record.snapshot_hash.clone(),
                decision: match (record.decision, record.approval_claimed) {
                    (AgentExportDecision::Delegated, _) => "delegated",
                    (AgentExportDecision::ApprovalRequired, true) => "approved",
                    (AgentExportDecision::CriticalConfirmation, true) => "critical_approved",
                    (AgentExportDecision::ApprovalRequired, false)
                    | (AgentExportDecision::CriticalConfirmation, false) => "denied",
                }
                .into(),
                outcome: match record.state {
                    AgentExportState::Succeeded => "succeeded",
                    AgentExportState::Cancelled => "cancelled",
                    AgentExportState::RecoveryRequired => "recovery_required",
                    _ => "failed",
                }
                .into(),
                affected_objects: vec![record.destination_id.clone()],
                rows_affected: Some(record.rows_written),
                rollback_state: if record.state == AgentExportState::RecoveryRequired {
                    "unknown"
                } else {
                    "not_needed"
                }
                .into(),
                error_code: record.error.as_ref().map(|error| error.code.clone()),
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        };
        if let Err(error) = self.repository.add_audit(&audit) {
            {
                let mut state = self.lock()?;
                if let Some(record) = state.records.get_mut(export_id) {
                    record.state = AgentExportState::RecoveryRequired;
                    record.error = Some(path_free_error(
                        "agent.export_audit_failed",
                        &format!("Terminal audit could not be stored: {error}"),
                    ));
                }
            }
            self.remember_terminal(export_id);
            return Ok(());
        }
        self.remember_terminal(export_id);
        Ok(())
    }

    fn remember_terminal(&self, export_id: &str) {
        let evicted = {
            let Ok(mut state) = self.state.lock() else {
                return;
            };
            state.terminal_order.retain(|id| id != export_id);
            state.terminal_order.push_back(export_id.to_string());
            let mut evicted = Vec::new();
            while state.terminal_order.len() > MAX_TERMINAL_AGENT_EXPORTS {
                if let Some(id) = state.terminal_order.pop_front() {
                    evicted.push(id);
                }
            }
            evicted
        };
        for id in evicted {
            let coordinator_id = self
                .state
                .lock()
                .ok()
                .and_then(|mut state| state.records.remove(&id))
                .and_then(|record| record.coordinator_export_id);
            if let Some(coordinator_id) = coordinator_id {
                let _ = self.coordinator.release(&coordinator_id);
            }
        }
    }

    fn require_owner(&self, connection_id: &str, export_id: &str) -> Result<(), String> {
        let state = self.lock()?;
        state
            .records
            .get(export_id)
            .filter(|record| record.connection_id == connection_id)
            .map(|_| ())
            .ok_or_else(export_missing)
    }

    fn cleanup_matching(&self, predicate: impl Fn(&AgentExportRecord) -> bool) {
        let Ok(_start) = self.start_lane.lock() else {
            return;
        };
        let work = self
            .state
            .lock()
            .map(|mut state| {
                state
                    .records
                    .iter_mut()
                    .filter(|(_, record)| predicate(record) && record.active())
                    .map(|(id, record)| {
                        if let Some(approval_id) = &record.approval_id {
                            self.access.cancel_export_approval(approval_id, id);
                        }
                        let awaiting = record.state == AgentExportState::AwaitingApproval;
                        if awaiting {
                            record.state = AgentExportState::Cancelled;
                        }
                        (id.clone(), record.coordinator_export_id.clone(), awaiting)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for (id, coordinator_id, awaiting) in work {
            if let Some(coordinator_id) = coordinator_id {
                let _ = self.coordinator.cancel(&coordinator_id);
                let _ = self.sync(&id);
            } else if awaiting {
                let _ = self.finish_terminal(&id);
            }
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, AgentExportRegistry>, String> {
        self.state
            .lock()
            .map_err(|_| "agent.export_state_unavailable".to_string())
    }
}

pub(crate) struct AgentExportCleaner {
    manager: Weak<AgentExportManager>,
}

impl AgentExportCleaner {
    pub(crate) fn new(manager: &Arc<AgentExportManager>) -> Self {
        Self {
            manager: Arc::downgrade(manager),
        }
    }
}

impl AgentResourceCleaner for AgentExportCleaner {
    fn cleanup_connection(&self, connection_id: &str) {
        if let Some(manager) = self.manager.upgrade() {
            manager.cleanup_matching(|record| record.connection_id == connection_id);
        }
    }

    fn cleanup_client_project(&self, client_id: &str, project_id: &str) {
        if let Some(manager) = self.manager.upgrade() {
            manager.cleanup_matching(|record| {
                record.client_id == client_id && record.project_id == project_id
            });
        }
    }

    fn cleanup_project(&self, project_id: &str) {
        if let Some(manager) = self.manager.upgrade() {
            manager.cleanup_matching(|record| record.project_id == project_id);
        }
    }

    fn cleanup_all(&self) {
        if let Some(manager) = self.manager.upgrade() {
            manager.cleanup_matching(|_| true);
        }
    }
}

fn decide(
    destination: &ExportDestinationRecord,
    intent: &AgentExportIntent,
) -> Result<(AgentExportDecision, ExportOverwritePolicy, &'static str), String> {
    let collision = has_canonical_collision(destination, intent)?;
    if collision {
        return Ok((
            AgentExportDecision::CriticalConfirmation,
            ExportOverwritePolicy::Replace,
            "agent.export_replace_critical",
        ));
    }
    let format_allowed = match intent.format {
        ExportFormat::Csv => destination.allow_csv,
        ExportFormat::Parquet => destination.allow_parquet,
    };
    if !format_allowed || intent.rows_per_part > destination.maximum_rows_per_part {
        return Ok((
            AgentExportDecision::ApprovalRequired,
            ExportOverwritePolicy::FailIfExists,
            "agent.export_policy_approval",
        ));
    }
    Ok((
        AgentExportDecision::Delegated,
        ExportOverwritePolicy::FailIfExists,
        "agent.export_delegated",
    ))
}

fn has_canonical_collision(
    destination: &ExportDestinationRecord,
    intent: &AgentExportIntent,
) -> Result<bool, String> {
    let directory = Path::new(&destination.canonical_path);
    let prefix = format!("{}-part-", intent.base_name);
    let suffix = format!(".{}", format_extension(intent.format));
    let entries = std::fs::read_dir(directory)
        .map_err(|_| "agent.destination_missing: Export folder is unavailable.".to_string())?;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_COLLISION_SCAN_ENTRIES {
            return Err(
                "agent.export_blocked: Export folder contains too many entries to inspect safely."
                    .into(),
            );
        }
        let entry = entry.map_err(|_| {
            "agent.destination_unsafe: Export folder could not be inspected.".to_string()
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let canonical = name
            .strip_prefix(&prefix)
            .and_then(|rest| rest.strip_suffix(&suffix))
            .filter(|sequence| {
                !sequence.is_empty() && sequence.bytes().all(|byte| byte.is_ascii_digit())
            })
            .and_then(|sequence| sequence.parse::<u64>().ok())
            .is_some_and(|sequence| {
                sequence > 0
                    && name
                        == format!(
                            "{}-part-{sequence:05}.{}",
                            intent.base_name,
                            format_extension(intent.format)
                        )
            });
        if canonical {
            let metadata = std::fs::symlink_metadata(entry.path()).map_err(|_| {
                "agent.destination_unsafe: Existing export target could not be inspected."
                    .to_string()
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(
                    "agent.export_blocked: Existing export targets must be regular files.".into(),
                );
            }
            return Ok(true);
        }
    }
    Ok(false)
}

fn to_engine_options(
    destination: &ExportDestinationRecord,
    intent: &AgentExportIntent,
    overwrite: ExportOverwritePolicy,
) -> Result<ExportOptions, String> {
    intent
        .validate()
        .map_err(|error| format!("agent.invalid_export_intent: {error}"))?;
    Ok(ExportOptions {
        format: match intent.format {
            ExportFormat::Csv => tarik_engine_protocol::ExportFormat::Csv,
            ExportFormat::Parquet => tarik_engine_protocol::ExportFormat::Parquet,
        },
        output_directory: destination.canonical_path.clone(),
        base_name: intent.base_name.clone(),
        rows_per_part: intent.rows_per_part,
        overwrite,
        csv: intent.csv.as_ref().map(|csv| CsvExportOptions {
            delimiter: csv.delimiter.clone(),
            include_header: csv.include_header,
        }),
        parquet: intent.parquet.as_ref().map(|parquet| ParquetExportOptions {
            compression: match parquet.compression {
                AgentParquetCompression::Uncompressed => ParquetCompression::Uncompressed,
                AgentParquetCompression::Snappy => ParquetCompression::Snappy,
                AgentParquetCompression::Gzip => ParquetCompression::Gzip,
                AgentParquetCompression::Zstd => ParquetCompression::Zstd,
            },
        }),
    })
}

fn apply_status(record: &mut AgentExportRecord, status: &ExportView) -> Result<(), String> {
    record.duration_ms = status.duration_ms;
    record.rows_written = status.rows_written;
    record.files_written = status.files_written;
    record.bytes_written = status.bytes_written;
    record.current_part = status.current_part;
    record.completed_parts = status
        .completed_parts
        .iter()
        .map(|part| {
            if part.part_number == 0 {
                return Err(
                    "agent.export_manifest_invalid: Export part identity is invalid.".to_string(),
                );
            }
            let file_name = format!(
                "{}-part-{:05}.{}",
                record.intent.base_name,
                part.part_number,
                format_extension(record.intent.format)
            );
            let expected = Path::new(&record.destination_path).join(&file_name);
            if Path::new(&part.path) != expected {
                return Err(
                    "agent.export_manifest_invalid: Export part identity is invalid.".to_string(),
                );
            }
            Ok(AgentExportPartSummary {
                part_number: part.part_number,
                file_name,
                rows: part.rows,
                bytes: part.bytes,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    record.error = status
        .error
        .as_ref()
        .map(|error| path_free_error(&error.code, &error.message));
    record.state = match status.state {
        ExportState::Queued => AgentExportState::Queued,
        ExportState::Running => AgentExportState::Running,
        ExportState::Succeeded => AgentExportState::Succeeded,
        ExportState::Failed
            if status
                .error
                .as_ref()
                .is_some_and(|error| error.code.contains("recovery")) =>
        {
            AgentExportState::RecoveryRequired
        }
        ExportState::Failed => AgentExportState::Failed,
        ExportState::Cancelled => AgentExportState::Cancelled,
    };
    Ok(())
}

fn path_free_error(code: &str, message: &str) -> ErrorEnvelope {
    let safe_message = if code.starts_with("agent.destination_stale") {
        "Destination policy changed; propose the export again."
    } else if code.starts_with("agent.export_not_approved") {
        "Tarik did not approve this export."
    } else if code.starts_with("agent.export_audit_failed") {
        "The export outcome requires review in visible Tarik."
    } else if code.starts_with("agent.export_manifest_invalid") {
        "Tarik rejected an invalid export manifest."
    } else if code.starts_with("agent.export_rejected") {
        if message.contains("policy changed") {
            "Destination policy changed; propose the export again."
        } else if message.contains("contents or policy changed") {
            "Destination contents changed; propose the export again."
        } else {
            "Tarik rejected the guarded export before completion."
        }
    } else {
        match code {
            "export.quota_exceeded" => "The export exceeded its delegated byte quota.",
            "export.collision" => "An export target appeared before publication.",
            "export.recovery_required" => "Export publication requires recovery in visible Tarik.",
            "export.cancelled" => "The export was cancelled.",
            "export.io" | "export.write" | "duckdb.error" => {
                "The guarded export failed. Review visible Tarik for local details."
            }
            _ => "The guarded export did not complete.",
        }
    };
    ErrorEnvelope::new(code, safe_message)
}

fn ensure_response_budget(view: &AgentExportView) -> Result<(), String> {
    if serde_json::to_vec(view)
        .map_err(|_| "agent.export_manifest_invalid".to_string())?
        .len()
        > MAX_AGENT_EXPORT_MANIFEST_BYTES
    {
        Err("agent.export_manifest_too_large: Release this export in Tarik.".into())
    } else {
        Ok(())
    }
}

fn format_extension(format: ExportFormat) -> &'static str {
    match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Parquet => "parquet",
    }
}

fn export_missing() -> String {
    "agent.export_missing: This export is unknown or belongs to another connection.".into()
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, time::Instant};

    use tarik_agent_protocol::{AgentCsvExportOptions, AgentParquetExportOptions, ProjectGrant};

    use crate::{
        agent_access::{challenge_proof, derive_verifier, hex, parse_hex_array},
        agent_destinations::DestinationPolicyInput,
        engine_manager::EngineManager,
        metadata::projects::{ProjectOwnership, ProjectsRepository},
        observability::AppLogger,
        projects::{ActiveProject, ProjectManager},
    };

    use super::*;

    struct Fixture {
        root: PathBuf,
        output: PathBuf,
        database: MetadataDb,
        access: Arc<AgentAccessManager>,
        destinations: Arc<AgentDestinationManager>,
        coordinator: Arc<ExportCoordinator>,
        manager: Arc<AgentExportManager>,
        engine: Arc<EngineManager>,
        project_id: String,
        client_id: String,
        connection_id: String,
        destination_id: String,
    }

    impl Fixture {
        fn new(label: &str, policy: DestinationPolicyInput) -> Self {
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
            let nonce = uuid::Uuid::new_v4();
            let root = std::env::temp_dir().join(format!("tarik-agent-export-{label}-{nonce}"));
            let output =
                std::env::temp_dir().join(format!("tarik-agent-export-output-{label}-{nonce}"));
            std::fs::create_dir_all(&root).unwrap();
            std::fs::create_dir_all(&output).unwrap();
            let project_path = root.join("project.duckdb");
            let database = MetadataDb::open_in_memory().unwrap();
            let project = ProjectsRepository::new(database.clone())
                .upsert("Agent export", &project_path, ProjectOwnership::External)
                .unwrap();
            let engine = Arc::new(EngineManager::new(engine_binary, root.join("results")));
            engine.open_session(&project_path).unwrap();
            let projects =
                ProjectManager::new(database.clone(), root.join("projects"), engine.clone());
            projects.set_active_for_test(ActiveProject {
                id: project.id.clone(),
                name: project.name,
                duckdb_path: project_path,
            });
            let logger = Arc::new(AppLogger::open(root.join("logs")));
            let access = Arc::new(AgentAccessManager::new(
                database.clone(),
                projects.clone(),
                engine.clone(),
                logger,
            ));
            access.set_enabled(true).unwrap();
            let key = [7u8; 32];
            let hello = access.hello("", "Pi", Some(&hex(&key))).unwrap();
            let client_id = access
                .approve_pairing(hello.pairing_request_id.as_deref().unwrap())
                .unwrap();
            let salt = parse_hex_array::<32>(&hello.salt).unwrap();
            let challenge = parse_hex_array::<32>(&hello.challenge).unwrap();
            let verifier = derive_verifier(&key, &salt);
            let proof = challenge_proof(&verifier, &hello.connection_id, &challenge).unwrap();
            access
                .authenticate(&hello.connection_id, &client_id, &hex(&proof))
                .unwrap();
            access
                .set_project_grant(
                    &client_id,
                    ProjectGrant {
                        project_id: project.id.clone(),
                        inspect: true,
                        analyze: true,
                        modify_workspace: false,
                        modify_data: false,
                    },
                )
                .unwrap();
            let destinations = Arc::new(AgentDestinationManager::new(
                database.clone(),
                projects,
                Vec::new(),
            ));
            let destination = destinations
                .create(&client_id, &project.id, output.to_str().unwrap(), policy)
                .unwrap();
            let coordinator = Arc::new(ExportCoordinator::new(engine.clone(), database.clone()));
            let manager = Arc::new(AgentExportManager::new(
                database.clone(),
                access.clone(),
                destinations.clone(),
                coordinator.clone(),
            ));
            access.set_resource_cleaner(Arc::new(AgentExportCleaner::new(&manager)));
            Self {
                root,
                output,
                database,
                access,
                destinations,
                coordinator,
                manager,
                engine,
                project_id: project.id,
                client_id,
                connection_id: hello.connection_id,
                destination_id: destination.destination_id,
            }
        }

        fn classify(&self, sql: &str) -> String {
            let result = self
                .access
                .classify_sql(&self.connection_id, &self.project_id, sql)
                .unwrap();
            assert_eq!(
                result.classification.decision,
                tarik_engine_protocol::AgentSqlDecision::SafeRead,
                "classification failed: {}",
                result.classification.reason_code
            );
            result.snapshot_id
        }

        fn csv_intent(&self, snapshot_id: String, base_name: &str) -> AgentExportIntent {
            AgentExportIntent {
                snapshot_id,
                destination_id: self.destination_id.clone(),
                format: ExportFormat::Csv,
                base_name: base_name.into(),
                rows_per_part: 2_000,
                csv: Some(AgentCsvExportOptions {
                    delimiter: ",".into(),
                    include_header: true,
                }),
                parquet: None,
            }
        }

        fn wait_terminal(&self, export_id: &str) -> AgentExportView {
            for _ in 0..800 {
                let status = self.manager.status(&self.connection_id, export_id).unwrap();
                if matches!(
                    status.state,
                    AgentExportState::Succeeded
                        | AgentExportState::Failed
                        | AgentExportState::Cancelled
                        | AgentExportState::RecoveryRequired
                ) {
                    return status;
                }
                thread::sleep(Duration::from_millis(10));
            }
            panic!("agent export did not terminate");
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.manager.cleanup_matching(|_| true);
            self.coordinator.cancel_all();
            self.engine.shutdown();
            let _ = std::fs::remove_dir_all(&self.output);
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn policy() -> DestinationPolicyInput {
        DestinationPolicyInput {
            display_label: "Complete exports".into(),
            allow_csv: true,
            allow_parquet: true,
            maximum_rows_per_part: 1_000_000,
            maximum_total_bytes: 100 * 1024 * 1024,
        }
    }

    #[test]
    fn delegated_export_runs_complete_query_redacts_paths_audits_and_releases() {
        let fixture = Fixture::new("complete", policy());
        let snapshot = fixture.classify("SELECT i, 'row-' || i AS label FROM range(0, 6001) t(i)");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot.clone(), "complete_orders"),
            )
            .unwrap();
        assert_eq!(proposed.decision, AgentExportDecision::Delegated);
        assert_eq!(proposed.state, AgentExportState::Queued);
        let terminal = fixture.wait_terminal(&proposed.export_id);
        assert!(fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "replayed"),
            )
            .unwrap_err()
            .contains("snapshot_missing"));
        assert_eq!(terminal.state, AgentExportState::Succeeded);
        assert!(terminal.complete_query);
        assert_eq!(terminal.rows_written, 6_001);
        assert_eq!(terminal.files_written, 4);
        assert_eq!(
            terminal
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![2_000, 2_000, 2_000, 1]
        );
        let encoded = serde_json::to_string(&terminal).unwrap();
        assert!(!encoded.contains(fixture.output.to_str().unwrap()));
        assert!(terminal
            .completed_parts
            .iter()
            .all(|part| !part.file_name.contains('/') && !part.file_name.contains('\\')));

        let audit = AgentRepository::new(fixture.database.clone())
            .list_audit(&fixture.project_id, 10)
            .unwrap();
        assert_eq!(audit.len(), 1);
        assert_eq!(audit[0].tool, "tarik_propose_export");
        assert_eq!(audit[0].decision, "delegated");
        assert_eq!(audit[0].rows_affected, Some(6_001));
        let audit_json = serde_json::to_string(&audit).unwrap();
        assert!(!audit_json.contains(fixture.output.to_str().unwrap()));
        assert!(!audit_json.contains("SELECT i"));

        let release = fixture
            .manager
            .release(&fixture.connection_id, &proposed.export_id)
            .unwrap();
        assert!(release.released);
        assert!(fixture
            .output
            .join("complete_orders-part-00001.csv")
            .is_file());
        assert!(fixture
            .manager
            .status(&fixture.connection_id, &proposed.export_id)
            .unwrap_err()
            .contains("export_missing"));
    }

    #[test]
    fn delegated_parquet_export_uses_typed_options_and_exact_parts() {
        let fixture = Fixture::new("parquet", policy());
        let snapshot = fixture.classify("SELECT i, i * 10 AS amount FROM range(0, 5) t(i)");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                AgentExportIntent {
                    snapshot_id: snapshot,
                    destination_id: fixture.destination_id.clone(),
                    format: ExportFormat::Parquet,
                    base_name: "typed_parquet".into(),
                    rows_per_part: 2,
                    csv: None,
                    parquet: Some(AgentParquetExportOptions {
                        compression: AgentParquetCompression::Zstd,
                    }),
                },
            )
            .unwrap();
        let terminal = fixture.wait_terminal(&proposed.export_id);
        assert_eq!(terminal.state, AgentExportState::Succeeded);
        assert_eq!(terminal.rows_written, 5);
        assert_eq!(terminal.files_written, 3);
        assert_eq!(
            terminal
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![2, 2, 1]
        );
        assert!(terminal
            .completed_parts
            .iter()
            .all(|part| part.file_name.ends_with(".parquet")));
        let first = std::fs::read(fixture.output.join("typed_parquet-part-00001.parquet")).unwrap();
        assert_eq!(&first[..4], b"PAR1");
        assert_eq!(&first[first.len() - 4..], b"PAR1");
        assert!(!serde_json::to_string(&terminal)
            .unwrap()
            .contains(fixture.output.to_str().unwrap()));
    }

    #[test]
    fn zero_rows_create_no_files() {
        let fixture = Fixture::new("empty", policy());
        let snapshot = fixture.classify("SELECT i FROM range(0) t(i)");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "empty_result"),
            )
            .unwrap();
        let terminal = fixture.wait_terminal(&proposed.export_id);
        assert_eq!(terminal.state, AgentExportState::Succeeded);
        assert_eq!(terminal.rows_written, 0);
        assert_eq!(terminal.files_written, 0);
        assert_eq!(terminal.bytes_written, 0);
        assert!(terminal.completed_parts.is_empty());
        assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 0);
    }

    #[test]
    fn collision_routes_to_critical_tarik_approval_before_replacement() {
        let fixture = Fixture::new("collision", policy());
        std::fs::write(fixture.output.join("replace_me-part-00001.csv"), "old\n").unwrap();
        let snapshot = fixture.classify("SELECT 42 AS answer");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "replace_me"),
            )
            .unwrap();
        assert_eq!(proposed.decision, AgentExportDecision::CriticalConfirmation);
        assert_eq!(proposed.state, AgentExportState::AwaitingApproval);
        assert_eq!(
            std::fs::read_to_string(fixture.output.join("replace_me-part-00001.csv")).unwrap(),
            "old\n"
        );
        let approval = fixture.access.list_pending_approvals().unwrap();
        assert_eq!(approval.len(), 1);
        assert_eq!(approval[0].action, "export");
        let phrase = approval[0].critical_phrase.clone().unwrap();
        fixture
            .access
            .decide_approval(&approval[0].id, true, Some(&phrase))
            .unwrap();
        let terminal = fixture.wait_terminal(&proposed.export_id);
        assert_eq!(terminal.state, AgentExportState::Succeeded);
        assert_eq!(
            std::fs::read_to_string(fixture.output.join("replace_me-part-00001.csv")).unwrap(),
            "answer\n42\n"
        );
    }

    #[test]
    fn policy_exception_waits_for_ordinary_tarik_approval_and_denial_runs_nothing() {
        let mut limited = policy();
        limited.maximum_rows_per_part = 10;
        let fixture = Fixture::new("policy", limited);
        let snapshot = fixture.classify("SELECT i FROM range(0, 20) t(i)");
        let mut intent = fixture.csv_intent(snapshot, "policy_exception");
        intent.rows_per_part = 20;
        let proposed = fixture
            .manager
            .propose(&fixture.connection_id, intent)
            .unwrap();
        assert_eq!(proposed.decision, AgentExportDecision::ApprovalRequired);
        assert_eq!(proposed.state, AgentExportState::AwaitingApproval);
        let approval = fixture.access.list_pending_approvals().unwrap();
        fixture
            .access
            .decide_approval(&approval[0].id, false, None)
            .unwrap();
        let terminal = fixture
            .manager
            .status(&fixture.connection_id, &proposed.export_id)
            .unwrap();
        assert_eq!(terminal.state, AgentExportState::Failed);
        assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 0);
        let audit = AgentRepository::new(fixture.database.clone())
            .list_audit(&fixture.project_id, 10)
            .unwrap();
        assert_eq!(audit[0].decision, "denied");
    }

    #[test]
    fn foreign_connection_and_destination_revision_drift_fail_closed() {
        let fixture = Fixture::new("ownership", policy());
        let snapshot = fixture.classify("SELECT i FROM range(0, 5) t(i)");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "owned"),
            )
            .unwrap();
        assert!(fixture
            .manager
            .status("other-connection", &proposed.export_id)
            .unwrap_err()
            .contains("export_missing"));
        fixture.wait_terminal(&proposed.export_id);

        std::fs::write(fixture.output.join("stale-part-00001.csv"), "old\n").unwrap();
        let snapshot = fixture.classify("SELECT 1 AS i");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "stale"),
            )
            .unwrap();
        let approval = fixture.access.list_pending_approvals().unwrap();
        let phrase = approval[0].critical_phrase.clone().unwrap();
        fixture
            .access
            .decide_approval(&approval[0].id, true, Some(&phrase))
            .unwrap();
        fixture
            .destinations
            .update_policy(
                &fixture.client_id,
                &fixture.project_id,
                &fixture.destination_id,
                DestinationPolicyInput {
                    display_label: "Changed".into(),
                    ..policy()
                },
            )
            .unwrap();
        let terminal = fixture
            .manager
            .status(&fixture.connection_id, &proposed.export_id)
            .unwrap();
        assert_eq!(terminal.state, AgentExportState::Failed);
        assert!(terminal
            .error
            .as_ref()
            .is_some_and(|error| error.message.contains("policy changed")));
        assert_eq!(
            std::fs::read_to_string(fixture.output.join("stale-part-00001.csv")).unwrap(),
            "old\n"
        );
    }

    #[test]
    fn status_cancel_release_are_owner_bound_and_active_release_fails() {
        let fixture = Fixture::new("lifecycle", policy());
        let snapshot = fixture
            .classify("SELECT i, lpad('x', 1000, 'x') AS payload FROM range(0, 1000000) t(i)");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "lifecycle"),
            )
            .unwrap();
        assert!(fixture
            .manager
            .release(&fixture.connection_id, &proposed.export_id)
            .unwrap_err()
            .contains("export_active"));
        assert!(fixture
            .manager
            .cancel("foreign", &proposed.export_id)
            .unwrap_err()
            .contains("export_missing"));
        let cancelled = fixture
            .manager
            .cancel(&fixture.connection_id, &proposed.export_id)
            .unwrap();
        let terminal = if cancelled.state == AgentExportState::Cancelled {
            cancelled
        } else {
            fixture.wait_terminal(&proposed.export_id)
        };
        assert_eq!(terminal.state, AgentExportState::Cancelled);
        let released = fixture
            .manager
            .release(&fixture.connection_id, &proposed.export_id)
            .unwrap();
        assert!(released.released);
    }

    #[test]
    fn byte_quota_and_disconnect_cancel_without_path_disclosure() {
        let mut tiny = policy();
        tiny.maximum_total_bytes = 1;
        let fixture = Fixture::new("quota", tiny);
        let snapshot = fixture.classify("SELECT i, lpad('x', 100, 'x') FROM range(0, 10) t(i)");
        let proposed = fixture
            .manager
            .propose(
                &fixture.connection_id,
                fixture.csv_intent(snapshot, "quota"),
            )
            .unwrap();
        let terminal = fixture.wait_terminal(&proposed.export_id);
        assert_eq!(terminal.state, AgentExportState::Failed);
        assert_eq!(terminal.files_written, 0);
        assert_eq!(std::fs::read_dir(&fixture.output).unwrap().count(), 0);
        assert!(!serde_json::to_string(&terminal)
            .unwrap()
            .contains(fixture.output.to_str().unwrap()));

        let cancellation = Fixture::new("cancel", policy());
        let snapshot = cancellation
            .classify("SELECT i, lpad('x', 1000, 'x') AS payload FROM range(0, 1000000) t(i)");
        let proposed = cancellation
            .manager
            .propose(
                &cancellation.connection_id,
                cancellation.csv_intent(snapshot, "cancelled"),
            )
            .unwrap();
        let started_at = Instant::now();
        while started_at.elapsed() < Duration::from_secs(2) {
            let status = cancellation
                .manager
                .status(&cancellation.connection_id, &proposed.export_id)
                .unwrap();
            if status.state == AgentExportState::Running {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        cancellation
            .access
            .disconnect(&cancellation.connection_id)
            .unwrap();
        for _ in 0..400 {
            let state = cancellation
                .manager
                .lock()
                .unwrap()
                .records
                .get(&proposed.export_id)
                .map(|record| record.state);
            if state == Some(AgentExportState::Cancelled) {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            cancellation
                .manager
                .lock()
                .unwrap()
                .records
                .get(&proposed.export_id)
                .unwrap()
                .state,
            AgentExportState::Cancelled
        );
    }
}

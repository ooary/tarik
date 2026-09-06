use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tarik_engine_protocol::{CatalogSnapshot, ExecutionState, ExecutionStatus};
use tauri::State;

use crate::{
    engine_manager::EngineManager,
    metadata::{
        quality::{
            CheckOptions, CheckOutcome, CheckRevision, CheckRunDraft, NullPolicy,
            QualityCheckDefinition, QualityRepository, QualityTarget,
        },
        MetadataDb,
    },
    projects::ProjectManager,
};

const POLL_INTERVAL: Duration = Duration::from_millis(150);
const MAX_POLL_FAILURES: u32 = 3;
const MAX_TERMINAL_EXECUTIONS: usize = 256;
const MAX_TRACKED_PREVIEWS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledCheck {
    pub count_sql: String,
    pub preview_sql: String,
    pub custom: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckExecutionState {
    Queued,
    Running,
    Passed,
    Failed,
    Error,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckExecutionError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckExecutionView {
    pub run_id: String,
    pub project_id: String,
    pub check_id: String,
    pub revision_id: String,
    pub state: CheckExecutionState,
    pub failure_count: Option<u64>,
    pub duration_ms: u64,
    pub sql: String,
    pub error: Option<CheckExecutionError>,
    pub observation_scope: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityRunDetail {
    pub run: crate::metadata::quality::CheckRun,
    pub check_name: String,
    pub revision_number: u32,
    pub current_revision_number: u32,
    pub definition: crate::metadata::quality::QualityCheckDraft,
    pub count_sql: String,
    pub failure_sql: String,
    pub custom: bool,
    pub is_latest_revision: bool,
}

struct CheckExecutionRecord {
    project_id: String,
    check_id: String,
    revision_id: String,
    state: CheckExecutionState,
    failure_count: Option<u64>,
    duration_ms: u64,
    count_sql: String,
    error: Option<CheckExecutionError>,
    history_written: bool,
}

impl CheckExecutionRecord {
    fn view(&self, run_id: &str) -> CheckExecutionView {
        CheckExecutionView {
            run_id: run_id.into(),
            project_id: self.project_id.clone(),
            check_id: self.check_id.clone(),
            revision_id: self.revision_id.clone(),
            state: self.state,
            failure_count: self.failure_count,
            duration_ms: self.duration_ms,
            sql: self.count_sql.clone(),
            error: self.error.clone(),
            observation_scope: "per_check",
        }
    }
}

pub trait QualityEngine: Send + Sync + 'static {
    fn validate_read_only(&self, sql: &str) -> Result<(), String>;
    fn execute(&self, execution_id: &str, sql: &str) -> Result<(), String>;
    fn status(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String>;
    fn cancel(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String>;
    fn page(&self, result_id: &str) -> Result<Value, String>;
    fn release(&self, result_id: &str) -> Result<(), String>;
}

impl QualityEngine for EngineManager {
    fn validate_read_only(&self, sql: &str) -> Result<(), String> {
        self.validate_quality_read_only(sql)
    }

    fn execute(&self, execution_id: &str, sql: &str) -> Result<(), String> {
        self.execute_query(execution_id, sql)
    }

    fn status(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String> {
        self.query_status(execution_id)
    }

    fn cancel(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String> {
        self.cancel_query(execution_id)
    }

    fn page(&self, result_id: &str) -> Result<Value, String> {
        self.result_page(result_id, 0, 1)
    }

    fn release(&self, result_id: &str) -> Result<(), String> {
        self.release_result(result_id)
    }
}

pub struct QualityCoordinator {
    engine: Arc<dyn QualityEngine>,
    database: MetadataDb,
    executions: Mutex<HashMap<String, CheckExecutionRecord>>,
    terminal_order: Mutex<VecDeque<String>>,
    previews: Mutex<VecDeque<String>>,
    poll_interval: Duration,
}

impl QualityCoordinator {
    pub fn new(engine: Arc<dyn QualityEngine>, database: MetadataDb) -> Self {
        Self {
            engine,
            database,
            executions: Mutex::new(HashMap::new()),
            terminal_order: Mutex::new(VecDeque::new()),
            previews: Mutex::new(VecDeque::new()),
            poll_interval: POLL_INTERVAL,
        }
    }

    #[cfg(test)]
    fn with_poll_interval(mut self, poll_interval: Duration) -> Self {
        self.poll_interval = poll_interval;
        self
    }

    pub fn preview(
        &self,
        draft: &crate::metadata::quality::QualityCheckDraft,
        catalog: &CatalogSnapshot,
    ) -> Result<CompiledCheckPreview, String> {
        let draft =
            crate::metadata::quality::validate_draft(draft).map_err(|error| error.to_string())?;
        validate_catalog_target(&draft, catalog)?;
        let compiled = compile_check(&draft)?;
        if compiled.custom {
            self.engine.validate_read_only(&compiled.preview_sql)?;
            self.engine.validate_read_only(&compiled.count_sql)?;
        }
        Ok(CompiledCheckPreview {
            count_sql: compiled.count_sql,
            failure_sql: compiled.preview_sql,
            custom: compiled.custom,
        })
    }

    pub fn run_check(
        self: &Arc<Self>,
        project_id: &str,
        check_id: &str,
    ) -> Result<CheckExecutionView, String> {
        let repository = QualityRepository::new(self.database.clone());
        let definition = repository
            .get(project_id, check_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "quality.check_missing".to_string())?;
        let revision = repository
            .revision(project_id, &definition.latest_revision_id)
            .map_err(|error| error.to_string())?;
        let revision_id = definition.latest_revision_id.clone();
        match revision {
            Some(revision) => match self.submit(definition.clone(), revision) {
                Ok(view) => Ok(view),
                Err(error) => self.record_submission_error(definition, revision_id, error),
            },
            None => self.record_submission_error(
                definition,
                revision_id,
                "quality.revision_missing".into(),
            ),
        }
    }

    pub fn run_revision(
        self: &Arc<Self>,
        project_id: &str,
        run_id: &str,
    ) -> Result<CheckExecutionView, String> {
        let detail = self.run_detail(project_id, run_id)?;
        let definition = QualityRepository::new(self.database.clone())
            .get(project_id, &detail.run.check_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "quality.check_missing".to_string())?;
        let revision = QualityRepository::new(self.database.clone())
            .revision(project_id, &detail.run.revision_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "quality.revision_missing".to_string())?;
        let revision_id = revision.id.clone();
        match self.submit(definition.clone(), revision) {
            Ok(view) => Ok(view),
            Err(error) => self.record_submission_error(definition, revision_id, error),
        }
    }

    pub fn run_detail(&self, project_id: &str, run_id: &str) -> Result<QualityRunDetail, String> {
        let repository = QualityRepository::new(self.database.clone());
        let run = repository
            .run(project_id, run_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "quality.run_missing".to_string())?;
        let definition = repository
            .get(project_id, &run.check_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "quality.check_missing".to_string())?;
        let revision = repository
            .revision(project_id, &run.revision_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "quality.revision_missing".to_string())?;
        let compiled = compile_check(&revision.definition)?;
        Ok(QualityRunDetail {
            check_name: revision.definition.name.clone(),
            revision_number: revision.revision_number,
            current_revision_number: definition.revision_number,
            definition: revision.definition,
            count_sql: compiled.count_sql,
            failure_sql: compiled.preview_sql,
            custom: compiled.custom,
            is_latest_revision: run.revision_id == definition.latest_revision_id,
            run,
        })
    }

    pub fn run_suite(
        self: &Arc<Self>,
        project_id: &str,
    ) -> Result<Vec<CheckExecutionView>, String> {
        let checks = QualityRepository::new(self.database.clone())
            .list(project_id)
            .map_err(|error| error.to_string())?;
        let mut views = Vec::new();
        // Sidecar query jobs are FIFO per session, so submission order is suite order.
        // One invalid/stale definition becomes its own truthful error run and cannot
        // prevent later enabled checks from starting.
        for check in checks.into_iter().filter(|check| check.enabled) {
            let revision = QualityRepository::new(self.database.clone())
                .revision(project_id, &check.latest_revision_id)
                .map_err(|error| error.to_string())?;
            match revision {
                Some(revision) => {
                    let revision_id = revision.id.clone();
                    match self.submit(check.clone(), revision) {
                        Ok(view) => views.push(view),
                        Err(error) => views.push(self.record_submission_error(
                            check.clone(),
                            revision_id,
                            error,
                        )?),
                    }
                }
                None => views.push(self.record_submission_error(
                    check.clone(),
                    check.latest_revision_id,
                    "quality.revision_missing".into(),
                )?),
            }
        }
        Ok(views)
    }

    fn record_submission_error(
        &self,
        definition: QualityCheckDefinition,
        revision_id: String,
        message: String,
    ) -> Result<CheckExecutionView, String> {
        let run_id = uuid::Uuid::new_v4().to_string();
        self.executions
            .lock()
            .map_err(|_| "quality execution registry poisoned".to_string())?
            .insert(
                run_id.clone(),
                CheckExecutionRecord {
                    project_id: definition.project_id,
                    check_id: definition.id,
                    revision_id,
                    state: CheckExecutionState::Queued,
                    failure_count: None,
                    duration_ms: 0,
                    count_sql: String::new(),
                    error: None,
                    history_written: false,
                },
            );
        self.mark_terminal(
            &run_id,
            CheckExecutionState::Error,
            None,
            Some(CheckExecutionError {
                code: "quality.invalid".into(),
                message,
            }),
        );
        self.status(&run_id)
            .ok_or_else(|| "quality execution registry poisoned".to_string())
    }

    fn submit(
        self: &Arc<Self>,
        definition: QualityCheckDefinition,
        revision: CheckRevision,
    ) -> Result<CheckExecutionView, String> {
        let compiled = compile_check(&revision.definition)?;
        if compiled.custom {
            self.engine.validate_read_only(&compiled.preview_sql)?;
            self.engine.validate_read_only(&compiled.count_sql)?;
        }
        let run_id = uuid::Uuid::new_v4().to_string();
        let execution_id = execution_id(&run_id);
        self.executions
            .lock()
            .map_err(|_| "quality execution registry poisoned".to_string())?
            .insert(
                run_id.clone(),
                CheckExecutionRecord {
                    project_id: definition.project_id,
                    check_id: definition.id,
                    revision_id: revision.id,
                    state: CheckExecutionState::Queued,
                    failure_count: None,
                    duration_ms: 0,
                    count_sql: compiled.count_sql.clone(),
                    error: None,
                    history_written: false,
                },
            );
        if let Err(error) = self.engine.execute(&execution_id, &compiled.count_sql) {
            self.mark_terminal(
                &run_id,
                CheckExecutionState::Error,
                None,
                Some(CheckExecutionError {
                    code: "quality.rejected".into(),
                    message: error,
                }),
            );
        } else {
            self.spawn_poller(run_id.clone());
        }
        self.status(&run_id)
            .ok_or_else(|| "quality execution registry poisoned".to_string())
    }

    pub fn status(&self, run_id: &str) -> Option<CheckExecutionView> {
        self.executions
            .lock()
            .ok()?
            .get(run_id)
            .map(|run| run.view(run_id))
    }

    pub fn cancel(self: &Arc<Self>, run_id: &str) -> Result<CheckExecutionView, String> {
        self.engine.cancel(&execution_id(run_id))?;
        self.status(run_id)
            .ok_or_else(|| "quality run does not exist".to_string())
    }

    pub fn cancel_all(self: &Arc<Self>) -> u64 {
        let ids = self
            .executions
            .lock()
            .map(|runs| {
                runs.iter()
                    .filter(|(_, run)| {
                        matches!(
                            run.state,
                            CheckExecutionState::Queued | CheckExecutionState::Running
                        )
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in &ids {
            let _ = self.engine.cancel(&execution_id(id));
        }
        let previews = self
            .previews
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        for id in &previews {
            let _ = self.engine.cancel(id);
        }
        (ids.len() + previews.len()) as u64
    }

    pub fn has_active_check(&self, project_id: &str, check_id: &str) -> bool {
        self.executions
            .lock()
            .map(|runs| {
                runs.values().any(|run| {
                    run.project_id == project_id
                        && run.check_id == check_id
                        && matches!(
                            run.state,
                            CheckExecutionState::Queued | CheckExecutionState::Running
                        )
                })
            })
            .unwrap_or(true)
    }

    pub fn has_active(&self) -> bool {
        let checks_active = self
            .executions
            .lock()
            .map(|runs| {
                runs.values().any(|run| {
                    matches!(
                        run.state,
                        CheckExecutionState::Queued | CheckExecutionState::Running
                    )
                })
            })
            .unwrap_or(false);
        let previews = self
            .previews
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        let mut previews_active = false;
        for id in previews {
            match self.engine.status(&id) {
                Ok(Some(status))
                    if matches!(
                        status.state,
                        ExecutionState::Queued | ExecutionState::Running
                    ) =>
                {
                    previews_active = true;
                }
                Ok(_) => {}
                Err(_) => previews_active = true,
            }
        }
        checks_active || previews_active
    }

    pub fn preview_status(&self, result_id: &str) -> Result<Option<ExecutionStatus>, String> {
        if !result_id.starts_with("quality-preview-") {
            return Err("quality.preview_invalid".into());
        }
        self.engine.status(result_id)
    }

    pub fn cancel_preview(&self, result_id: &str) -> Result<Option<ExecutionStatus>, String> {
        if !result_id.starts_with("quality-preview-") {
            return Err("quality.preview_invalid".into());
        }
        self.engine.cancel(result_id)
    }

    pub fn start_failure_preview(
        &self,
        project_id: &str,
        run_id: &str,
    ) -> Result<FailurePreview, String> {
        let detail = self.run_detail(project_id, run_id)?;
        if detail.run.outcome != CheckOutcome::Fail {
            return Err("quality.preview_requires_failed_run".into());
        }
        {
            let previews = self
                .previews
                .lock()
                .map_err(|_| "quality preview registry poisoned".to_string())?;
            if previews.len() >= MAX_TRACKED_PREVIEWS {
                return Err("quality.preview_limit: release an open failure preview first".into());
            }
        }
        let result_id = format!("quality-preview-{run_id}-{}", uuid::Uuid::new_v4());
        self.engine.execute(&result_id, &detail.failure_sql)?;
        self.previews
            .lock()
            .map_err(|_| "quality preview registry poisoned".to_string())?
            .push_back(result_id.clone());
        Ok(FailurePreview {
            result_id,
            project_id: project_id.into(),
            revision_id: detail.run.revision_id,
            sql: detail.failure_sql,
            state: ExecutionState::Queued,
        })
    }

    pub fn release_preview(&self, result_id: &str) -> Result<(), String> {
        if !result_id.starts_with("quality-preview-") {
            return Err("quality.preview_invalid".into());
        }
        self.engine.release(result_id)?;
        self.remove_preview(result_id);
        Ok(())
    }

    fn remove_preview(&self, result_id: &str) {
        if let Ok(mut previews) = self.previews.lock() {
            previews.retain(|id| id != result_id);
        }
    }

    fn spawn_poller(self: &Arc<Self>, run_id: String) {
        let coordinator = Arc::clone(self);
        if thread::Builder::new()
            .name(format!("tarik-quality-poll-{run_id}"))
            .spawn({
                let id = run_id.clone();
                move || coordinator.poll_until_terminal(&id)
            })
            .is_err()
        {
            self.mark_terminal(
                &run_id,
                CheckExecutionState::Error,
                None,
                Some(CheckExecutionError {
                    code: "quality.poller".into(),
                    message: "could not start quality status poller".into(),
                }),
            );
        }
    }

    fn poll_until_terminal(&self, run_id: &str) {
        let engine_id = execution_id(run_id);
        let mut failures = 0u32;
        loop {
            thread::sleep(self.poll_interval);
            match self.engine.status(&engine_id) {
                Ok(Some(status)) => match status.state {
                    ExecutionState::Queued | ExecutionState::Running => {
                        failures = 0;
                        if let Ok(mut runs) = self.executions.lock() {
                            if let Some(run) = runs.get_mut(run_id) {
                                run.state = if status.state == ExecutionState::Queued {
                                    CheckExecutionState::Queued
                                } else {
                                    CheckExecutionState::Running
                                };
                                run.duration_ms = status.duration_ms;
                            }
                        }
                    }
                    ExecutionState::Succeeded => {
                        self.finish_success(run_id, &engine_id, &status);
                        break;
                    }
                    ExecutionState::Failed => {
                        let _ = self.engine.release(&engine_id);
                        let error = status.error.map(|error| CheckExecutionError {
                            code: error.code,
                            message: error.message,
                        });
                        self.mark_terminal(run_id, CheckExecutionState::Error, None, error);
                        break;
                    }
                    ExecutionState::Cancelled => {
                        let _ = self.engine.release(&engine_id);
                        self.mark_terminal(run_id, CheckExecutionState::Cancelled, None, None);
                        break;
                    }
                },
                Ok(None) => {
                    self.mark_terminal(
                        run_id,
                        CheckExecutionState::Error,
                        None,
                        Some(CheckExecutionError {
                            code: "quality.lost".into(),
                            message: "the engine no longer knows this quality execution".into(),
                        }),
                    );
                    break;
                }
                Err(error) => {
                    failures += 1;
                    if failures >= MAX_POLL_FAILURES {
                        self.mark_terminal(
                            run_id,
                            CheckExecutionState::Error,
                            None,
                            Some(CheckExecutionError {
                                code: "engine.unreachable".into(),
                                message: error,
                            }),
                        );
                        break;
                    }
                }
            }
        }
    }

    fn finish_success(&self, run_id: &str, execution_id: &str, status: &ExecutionStatus) {
        if let Ok(mut runs) = self.executions.lock() {
            if let Some(run) = runs.get_mut(run_id) {
                run.duration_ms = status.duration_ms;
            }
        }
        let failure_count = self
            .engine
            .page(execution_id)
            .ok()
            .and_then(|page| page.get("rows").cloned())
            .and_then(|rows| rows.as_array().cloned())
            .and_then(|rows| rows.first().cloned())
            .and_then(|row| row.as_array().cloned())
            .and_then(|row| row.first().cloned())
            .and_then(json_u64);
        let _ = self.engine.release(execution_id);
        match failure_count {
            Some(0) => self.mark_terminal(run_id, CheckExecutionState::Passed, Some(0), None),
            Some(count) => {
                self.mark_terminal(run_id, CheckExecutionState::Failed, Some(count), None)
            }
            None => self.mark_terminal(
                run_id,
                CheckExecutionState::Error,
                None,
                Some(CheckExecutionError {
                    code: "quality.count_decode".into(),
                    message: "quality check did not return one non-negative failure count".into(),
                }),
            ),
        }
    }

    fn mark_terminal(
        &self,
        run_id: &str,
        state: CheckExecutionState,
        failure_count: Option<u64>,
        error: Option<CheckExecutionError>,
    ) {
        let history = {
            let mut runs = match self.executions.lock() {
                Ok(runs) => runs,
                Err(_) => return,
            };
            let Some(run) = runs.get_mut(run_id) else {
                return;
            };
            if run.history_written {
                return;
            }
            run.state = state;
            run.failure_count = failure_count;
            run.error = error;
            run.history_written = true;
            CheckRunDraft {
                id: run_id.into(),
                project_id: run.project_id.clone(),
                check_id: run.check_id.clone(),
                revision_id: run.revision_id.clone(),
                outcome: match state {
                    CheckExecutionState::Passed => CheckOutcome::Pass,
                    CheckExecutionState::Failed => CheckOutcome::Fail,
                    CheckExecutionState::Cancelled => CheckOutcome::Cancelled,
                    CheckExecutionState::Error => CheckOutcome::Error,
                    CheckExecutionState::Queued | CheckExecutionState::Running => return,
                },
                failure_count,
                duration_ms: run.duration_ms.max(1),
                observed_at: chrono::Utc::now().to_rfc3339(),
                error_code: run.error.as_ref().map(|error| error.code.clone()),
            }
        };
        if let Err(error) = QualityRepository::new(self.database.clone()).add_run(&history) {
            if let Ok(mut runs) = self.executions.lock() {
                if let Some(run) = runs.get_mut(run_id) {
                    // The analytical terminal outcome happened, but Tarik cannot
                    // claim a durable terminal run until aggregate history exists.
                    run.state = CheckExecutionState::Error;
                    run.history_written = false;
                    run.error = Some(CheckExecutionError {
                        code: "quality.history".into(),
                        message: error.to_string(),
                    });
                }
            }
            return;
        }
        self.remember_terminal(run_id);
    }

    fn remember_terminal(&self, run_id: &str) {
        let stale = {
            let mut order = match self.terminal_order.lock() {
                Ok(order) => order,
                Err(_) => return,
            };
            order.retain(|id| id != run_id);
            order.push_back(run_id.into());
            let excess = order.len().saturating_sub(MAX_TERMINAL_EXECUTIONS);
            order.drain(..excess).collect::<Vec<_>>()
        };
        if stale.is_empty() {
            return;
        }
        if let Ok(mut runs) = self.executions.lock() {
            for id in stale {
                if runs.get(&id).is_some_and(|run| {
                    !matches!(
                        run.state,
                        CheckExecutionState::Queued | CheckExecutionState::Running
                    )
                }) {
                    runs.remove(&id);
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledCheckPreview {
    pub count_sql: String,
    pub failure_sql: String,
    pub custom: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailurePreview {
    pub result_id: String,
    pub project_id: String,
    pub revision_id: String,
    pub sql: String,
    pub state: ExecutionState,
}

fn validate_catalog_target(
    draft: &crate::metadata::quality::QualityCheckDraft,
    catalog: &CatalogSnapshot,
) -> Result<(), String> {
    validate_target_columns(&draft.target, catalog)?;
    if let CheckOptions::Relationship {
        parent,
        parent_columns,
    } = &draft.options
    {
        validate_target_columns(parent, catalog)?;
        for (child, parent_name) in draft.target.columns.iter().zip(parent_columns) {
            let child_type = catalog_type(&draft.target, child, catalog)?;
            let parent_type = catalog_type(parent, parent_name, catalog)?;
            if child_type != parent_type {
                return Err(format!(
                    "quality.relationship_type_mismatch: {child_type} does not match {parent_type}"
                ));
            }
        }
    }
    if matches!(draft.options, CheckOptions::Freshness { .. }) {
        let data_type = catalog_type(&draft.target, &draft.target.columns[0], catalog)?;
        let upper = data_type.to_ascii_uppercase();
        if !upper.starts_with("DATE") && !upper.starts_with("TIME") {
            return Err("quality.freshness_type: freshness requires DATE or TIMESTAMP".into());
        }
    }
    if matches!(draft.options, CheckOptions::Range { .. }) {
        let data_type = catalog_type(&draft.target, &draft.target.columns[0], catalog)?;
        let upper = data_type.to_ascii_uppercase();
        const RANGE_PREFIXES: &[&str] = &[
            "TINYINT",
            "SMALLINT",
            "INTEGER",
            "BIGINT",
            "HUGEINT",
            "UTINYINT",
            "USMALLINT",
            "UINTEGER",
            "UBIGINT",
            "UHUGEINT",
            "FLOAT",
            "DOUBLE",
            "REAL",
            "DECIMAL",
            "NUMERIC",
            "DATE",
            "TIME",
            "TIMESTAMP",
        ];
        if !RANGE_PREFIXES
            .iter()
            .any(|prefix| upper.starts_with(prefix))
        {
            return Err("quality.range_type: range requires numeric or temporal data".into());
        }
    }
    Ok(())
}

fn validate_target_columns(
    target: &QualityTarget,
    catalog: &CatalogSnapshot,
) -> Result<(), String> {
    let exists = catalog.objects.iter().any(|object| {
        object.database == target.database
            && object.schema == target.schema
            && object.name == target.object
            && matches!(object.kind.as_str(), "table" | "view")
    });
    if !exists {
        return Err("quality.catalog_stale: target table or view is missing".into());
    }
    for column in &target.columns {
        catalog_type(target, column, catalog)?;
    }
    Ok(())
}

fn catalog_type<'a>(
    target: &QualityTarget,
    column_name: &str,
    catalog: &'a CatalogSnapshot,
) -> Result<&'a str, String> {
    catalog
        .columns
        .iter()
        .find(|column| {
            column.database == target.database
                && column.schema == target.schema
                && column.object == target.object
                && column.name == column_name
        })
        .map(|column| column.data_type.as_str())
        .ok_or_else(|| format!("quality.catalog_stale: column {column_name} is missing"))
}

pub fn compile_check(
    draft: &crate::metadata::quality::QualityCheckDraft,
) -> Result<CompiledCheck, String> {
    let target = qualified(&draft.target)?;
    let nulls_fail = draft.null_policy == NullPolicy::FailOnNull;
    let predicate = match &draft.options {
        CheckOptions::NotEmpty => {
            return Ok(CompiledCheck {
                count_sql: format!("SELECT CASE WHEN count(*) = 0 THEN 1 ELSE 0 END AS failure_count FROM {target}"),
                preview_sql: format!("SELECT * FROM {target} WHERE false"),
                custom: false,
            });
        }
        CheckOptions::NotNull => format!("{} IS NULL", column(&draft.target, 0)?),
        CheckOptions::Unique => {
            let columns = draft
                .target
                .columns
                .iter()
                .map(|name| quote(name))
                .collect::<Result<Vec<_>, _>>()?;
            let joined = columns.join(", ");
            let any_null = columns
                .iter()
                .map(|column| format!("{column} IS NULL"))
                .collect::<Vec<_>>()
                .join(" OR ");
            let all_non_null = columns
                .iter()
                .map(|column| format!("{column} IS NOT NULL"))
                .collect::<Vec<_>>()
                .join(" AND ");
            let duplicate_groups = format!(
                "SELECT {joined} FROM {target} WHERE {all_non_null} GROUP BY {joined} HAVING count(*) > 1"
            );
            let duplicate_predicate = columns
                .iter()
                .map(|column| format!("source.{column} IS NOT DISTINCT FROM duplicates.{column}"))
                .collect::<Vec<_>>()
                .join(" AND ");
            let duplicate_rows = format!(
                "EXISTS (SELECT 1 FROM ({duplicate_groups}) duplicates WHERE {duplicate_predicate})"
            );
            let failure_predicate = if nulls_fail {
                format!("({any_null}) OR {duplicate_rows}")
            } else {
                format!("({all_non_null}) AND {duplicate_rows}")
            };
            return Ok(CompiledCheck {
                count_sql: format!(
                    "SELECT count(*)::UBIGINT AS failure_count FROM {target} source WHERE {failure_predicate}"
                ),
                preview_sql: format!("SELECT source.* FROM {target} source WHERE {failure_predicate}"),
                custom: false,
            });
        }
        CheckOptions::AcceptedValues { values } => {
            let column = column(&draft.target, 0)?;
            let values = values
                .iter()
                .map(sql_literal)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ");
            null_predicate(&column, &format!("{column} NOT IN ({values})"), nulls_fail)
        }
        CheckOptions::Range {
            minimum,
            maximum,
            inclusive_minimum,
            inclusive_maximum,
        } => {
            let column = column(&draft.target, 0)?;
            let mut outside = Vec::new();
            if let Some(minimum) = minimum {
                outside.push(format!(
                    "{column} {} {}",
                    if *inclusive_minimum { "<" } else { "<=" },
                    sql_literal(minimum)?
                ));
            }
            if let Some(maximum) = maximum {
                outside.push(format!(
                    "{column} {} {}",
                    if *inclusive_maximum { ">" } else { ">=" },
                    sql_literal(maximum)?
                ));
            }
            null_predicate(&column, &format!("({})", outside.join(" OR ")), nulls_fail)
        }
        CheckOptions::Relationship {
            parent,
            parent_columns,
        } => {
            let child_columns = draft
                .target
                .columns
                .iter()
                .map(|name| quote(name))
                .collect::<Result<Vec<_>, _>>()?;
            let parent_columns = parent_columns
                .iter()
                .map(|name| quote(name))
                .collect::<Result<Vec<_>, _>>()?;
            let parent_target = qualified(parent)?;
            let parent_projection = parent_columns.join(", ");
            let parent_source = format!(
                "(SELECT DISTINCT {parent_projection}, true AS __tarik_match FROM {parent_target})"
            );
            let join = child_columns
                .iter()
                .zip(&parent_columns)
                .map(|(child, parent)| {
                    format!("child.{child} IS NOT DISTINCT FROM parent.{parent}")
                })
                .collect::<Vec<_>>()
                .join(" AND ");
            let child_non_null = child_columns
                .iter()
                .map(|column| format!("child.{column} IS NOT NULL"))
                .collect::<Vec<_>>()
                .join(" AND ");
            let child_has_null = child_columns
                .iter()
                .map(|column| format!("child.{column} IS NULL"))
                .collect::<Vec<_>>()
                .join(" OR ");
            let unmatched = "parent.__tarik_match IS NULL";
            let where_clause = if nulls_fail {
                format!("({child_has_null}) OR {unmatched}")
            } else {
                format!("({child_non_null}) AND {unmatched}")
            };
            let source = format!("{target} child LEFT JOIN {parent_source} parent ON {join}");
            return Ok(CompiledCheck {
                count_sql: format!(
                    "SELECT count(*)::UBIGINT AS failure_count FROM {source} WHERE {where_clause}"
                ),
                preview_sql: format!("SELECT child.* FROM {source} WHERE {where_clause}"),
                custom: false,
            });
        }
        CheckOptions::Freshness {
            maximum_age_seconds,
        } => {
            let column = column(&draft.target, 0)?;
            let stale = format!(
                "max({column}) < current_timestamp - INTERVAL '{} seconds'",
                maximum_age_seconds
            );
            let null_failure = if nulls_fail {
                format!("count(*) FILTER (WHERE {column} IS NULL) > 0 OR ")
            } else {
                String::new()
            };
            let aggregate_failure = format!("count({column}) > 0 AND ({null_failure}{stale})");
            return Ok(CompiledCheck {
                count_sql: format!(
                    "SELECT CASE WHEN count({column}) = 0 THEN NULL WHEN {null_failure}{stale} THEN 1 ELSE 0 END::UBIGINT AS failure_count FROM {target}"
                ),
                preview_sql: format!(
                    "SELECT source.* FROM {target} source WHERE (SELECT {aggregate_failure} FROM {target}) AND (source.{column} IS NULL OR source.{column} < current_timestamp - INTERVAL '{} seconds')",
                    maximum_age_seconds
                ),
                custom: false,
            });
        }
        CheckOptions::CustomSql { sql } => {
            let sql = sql.trim().trim_end_matches(';').trim();
            return Ok(CompiledCheck {
                count_sql: format!(
                    "SELECT count(*)::UBIGINT AS failure_count FROM ({sql}) quality_failures"
                ),
                preview_sql: sql.into(),
                custom: true,
            });
        }
    };
    Ok(CompiledCheck {
        count_sql: format!(
            "SELECT count(*)::UBIGINT AS failure_count FROM {target} WHERE {predicate}"
        ),
        preview_sql: format!("SELECT * FROM {target} WHERE {predicate}"),
        custom: false,
    })
}

fn null_predicate(column: &str, failure: &str, nulls_fail: bool) -> String {
    if nulls_fail {
        format!("{column} IS NULL OR {failure}")
    } else {
        format!("{column} IS NOT NULL AND {failure}")
    }
}

fn qualified(target: &QualityTarget) -> Result<String, String> {
    Ok(format!(
        "{}.{}.{}",
        quote(&target.database)?,
        quote(&target.schema)?,
        quote(&target.object)?
    ))
}

fn column(target: &QualityTarget, index: usize) -> Result<String, String> {
    quote(
        target
            .columns
            .get(index)
            .ok_or_else(|| "quality.column_missing".to_string())?,
    )
}

fn quote(identifier: &str) -> Result<String, String> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return Err("quality.identifier_empty".into());
    }
    Ok(format!("\"{}\"", identifier.replace('"', "\"\"")))
}

fn sql_literal(value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok("NULL".into()),
        Value::Bool(value) => Ok(value.to_string().to_ascii_uppercase()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(format!("'{}'", value.replace('\'', "''"))),
        Value::Array(_) | Value::Object(_) => Err("quality.literal_requires_scalar".into()),
    }
}

fn json_u64(value: Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn execution_id(run_id: &str) -> String {
    format!("quality-count-{run_id}")
}

#[tauri::command]
pub fn preview_quality_check_sql(
    draft: crate::metadata::quality::QualityCheckDraft,
    quality: State<'_, Arc<QualityCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<CompiledCheckPreview, String> {
    require_project(&draft.project_id, &projects)?;
    let catalog = projects.catalog().map_err(|error| error.to_string())?;
    quality.preview(&draft, &catalog)
}

#[tauri::command]
pub fn run_quality_check(
    project_id: String,
    check_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<CheckExecutionView, String> {
    require_project(&project_id, &projects)?;
    quality.run_check(&project_id, &check_id)
}

#[tauri::command]
pub fn get_quality_run_detail(
    project_id: String,
    run_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<QualityRunDetail, String> {
    require_project(&project_id, &projects)?;
    quality.run_detail(&project_id, &run_id)
}

#[tauri::command]
pub fn rerun_quality_revision(
    project_id: String,
    run_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<CheckExecutionView, String> {
    require_project(&project_id, &projects)?;
    quality.run_revision(&project_id, &run_id)
}

#[tauri::command]
pub fn run_quality_suite(
    project_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<Vec<CheckExecutionView>, String> {
    require_project(&project_id, &projects)?;
    quality.run_suite(&project_id)
}

#[tauri::command]
pub fn get_quality_run_status(
    run_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
) -> Option<CheckExecutionView> {
    quality.status(&run_id)
}

#[tauri::command]
pub fn cancel_quality_run(
    run_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
) -> Result<CheckExecutionView, String> {
    quality.cancel(&run_id)
}

#[tauri::command]
pub fn start_quality_failure_preview(
    project_id: String,
    run_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<FailurePreview, String> {
    require_project(&project_id, &projects)?;
    quality.start_failure_preview(&project_id, &run_id)
}

#[tauri::command]
pub fn get_quality_failure_preview_status(
    result_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
) -> Result<Option<ExecutionStatus>, String> {
    quality.preview_status(&result_id)
}

#[tauri::command]
pub fn release_quality_failure_preview(
    result_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
) -> Result<(), String> {
    quality.release_preview(&result_id)
}

#[tauri::command]
pub fn cancel_quality_failure_preview(
    result_id: String,
    quality: State<'_, Arc<QualityCoordinator>>,
) -> Result<Option<ExecutionStatus>, String> {
    quality.cancel_preview(&result_id)
}

fn require_project(project_id: &str, projects: &ProjectManager) -> Result<(), String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "quality.no_active_project".to_string())?;
    if active.id != project_id {
        return Err(format!(
            "quality.project_mismatch: active project is {}, request was {project_id}",
            active.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, path::Path, sync::Mutex as StdMutex};

    use super::*;
    use crate::metadata::{
        projects::{ProjectOwnership, ProjectsRepository},
        quality::{CheckSeverity, QualityCheckDraft},
    };

    struct FakeEngine {
        statuses: StdMutex<VecDeque<ExecutionStatus>>,
        page: Value,
        submitted: StdMutex<Vec<(String, String)>>,
        released: StdMutex<Vec<String>>,
        cancel_called: StdMutex<bool>,
    }

    impl FakeEngine {
        fn new(statuses: Vec<ExecutionStatus>, failure_count: u64) -> Self {
            Self {
                statuses: StdMutex::new(statuses.into()),
                page: serde_json::json!({ "rows": [[failure_count]] }),
                submitted: StdMutex::new(Vec::new()),
                released: StdMutex::new(Vec::new()),
                cancel_called: StdMutex::new(false),
            }
        }
    }

    impl QualityEngine for FakeEngine {
        fn validate_read_only(&self, _sql: &str) -> Result<(), String> {
            Ok(())
        }
        fn execute(&self, execution_id: &str, sql: &str) -> Result<(), String> {
            self.submitted
                .lock()
                .unwrap()
                .push((execution_id.into(), sql.into()));
            Ok(())
        }
        fn status(&self, _execution_id: &str) -> Result<Option<ExecutionStatus>, String> {
            Ok(self.statuses.lock().unwrap().pop_front())
        }
        fn cancel(&self, _execution_id: &str) -> Result<Option<ExecutionStatus>, String> {
            *self.cancel_called.lock().unwrap() = true;
            Ok(None)
        }
        fn page(&self, _result_id: &str) -> Result<Value, String> {
            Ok(self.page.clone())
        }
        fn release(&self, result_id: &str) -> Result<(), String> {
            self.released.lock().unwrap().push(result_id.into());
            Ok(())
        }
    }

    fn status(state: ExecutionState, duration_ms: u64) -> ExecutionStatus {
        ExecutionStatus {
            execution_id: "engine".into(),
            state,
            duration_ms,
            rows_produced: Some(1),
            rows_affected: None,
            error: None,
            result: None,
        }
    }

    fn stored_check(database: &MetadataDb) -> (String, QualityCheckDefinition) {
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Quality",
                Path::new("/data/quality.duckdb"),
                ProjectOwnership::External,
            )
            .unwrap();
        let mut candidate = draft(CheckOptions::NotNull, NullPolicy::FailOnNull);
        candidate.project_id = project.id.clone();
        let check = QualityRepository::new(database.clone())
            .create(&candidate)
            .unwrap();
        (project.id, check)
    }

    fn target() -> QualityTarget {
        QualityTarget {
            database: "db".into(),
            schema: "main".into(),
            object: "odd table".into(),
            columns: vec!["customer id".into()],
        }
    }

    fn draft(options: CheckOptions, null_policy: NullPolicy) -> QualityCheckDraft {
        QualityCheckDraft {
            project_id: "p".into(),
            name: "check".into(),
            target: target(),
            options,
            null_policy,
            severity: CheckSeverity::Warning,
            enabled: true,
        }
    }

    #[test]
    fn preview_compiles_without_executing() {
        let database = MetadataDb::open_in_memory().unwrap();
        let engine = Arc::new(FakeEngine::new(Vec::new(), 0));
        let coordinator = QualityCoordinator::new(engine.clone(), database);
        let check = draft(CheckOptions::NotNull, NullPolicy::FailOnNull);
        let catalog = CatalogSnapshot {
            revision: "catalog-1".into(),
            objects: vec![tarik_engine_protocol::CatalogObject {
                database: "db".into(),
                schema: "main".into(),
                name: "odd table".into(),
                kind: "table".into(),
                estimated_row_count: None,
            }],
            columns: vec![tarik_engine_protocol::CatalogColumn {
                database: "db".into(),
                schema: "main".into(),
                object: "odd table".into(),
                name: "customer id".into(),
                data_type: "BIGINT".into(),
                position: 0,
                nullable: true,
            }],
        };
        let preview = coordinator.preview(&check, &catalog).unwrap();
        assert!(preview.count_sql.contains("failure_count"));
        assert!(preview.failure_sql.contains("IS NULL"));
        assert!(!preview.custom);
        assert!(engine.submitted.lock().unwrap().is_empty());
    }

    #[test]
    fn compiler_quotes_identifiers_literals_and_null_policies() {
        let compiled = compile_check(&draft(
            CheckOptions::AcceptedValues {
                values: vec![Value::String("O'Reilly".into()), Value::from(2)],
            },
            NullPolicy::FailOnNull,
        ))
        .unwrap();
        assert!(compiled.count_sql.contains("\"db\".\"main\".\"odd table\""));
        assert!(compiled.count_sql.contains("\"customer id\" IS NULL OR"));
        assert!(compiled.count_sql.contains("'O''Reilly', 2"));
    }

    #[test]
    fn coordinator_persists_exactly_one_terminal_run_and_releases_count_result() {
        let database = MetadataDb::open_in_memory().unwrap();
        let (project_id, check) = stored_check(&database);
        let engine = Arc::new(FakeEngine::new(
            vec![
                status(ExecutionState::Running, 2),
                status(ExecutionState::Succeeded, 7),
            ],
            3,
        ));
        let coordinator = Arc::new(
            QualityCoordinator::new(engine.clone(), database.clone())
                .with_poll_interval(Duration::from_millis(1)),
        );
        let submitted = coordinator.run_check(&project_id, &check.id).unwrap();
        for _ in 0..200 {
            if coordinator
                .status(&submitted.run_id)
                .is_some_and(|view| view.state == CheckExecutionState::Failed)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let terminal = coordinator.status(&submitted.run_id).unwrap();
        assert_eq!(terminal.state, CheckExecutionState::Failed);
        assert_eq!(terminal.failure_count, Some(3));
        let history = QualityRepository::new(database)
            .history(&project_id, Some(&check.id), 0, 20)
            .unwrap();
        assert_eq!(history.entries.len(), 1);
        assert_eq!(history.entries[0].failure_count, Some(3));
        assert_eq!(history.entries[0].duration_ms, 7);
        assert_eq!(engine.released.lock().unwrap().len(), 1);
    }

    #[test]
    fn failed_run_starts_an_explicit_immutable_preview_only() {
        let database = MetadataDb::open_in_memory().unwrap();
        let (project_id, check) = stored_check(&database);
        let engine = Arc::new(FakeEngine::new(
            vec![
                status(ExecutionState::Succeeded, 1),
                status(ExecutionState::Queued, 0),
            ],
            2,
        ));
        let coordinator = Arc::new(
            QualityCoordinator::new(engine.clone(), database)
                .with_poll_interval(Duration::from_millis(1)),
        );
        let submitted = coordinator.run_check(&project_id, &check.id).unwrap();
        assert_eq!(engine.submitted.lock().unwrap().len(), 1);
        for _ in 0..200 {
            if coordinator
                .status(&submitted.run_id)
                .is_some_and(|view| view.state == CheckExecutionState::Failed)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        let preview = coordinator
            .start_failure_preview(&project_id, &submitted.run_id)
            .unwrap();
        assert_eq!(preview.revision_id, check.latest_revision_id);
        assert!(preview.result_id.starts_with("quality-preview-"));
        assert_eq!(engine.submitted.lock().unwrap().len(), 2);
        assert!(coordinator.has_active());
        coordinator.cancel_preview(&preview.result_id).unwrap();
        assert!(*engine.cancel_called.lock().unwrap());
    }

    #[test]
    fn persisted_run_detail_preview_and_rerun_use_the_historical_revision_after_restart() {
        let database = MetadataDb::open_in_memory().unwrap();
        let (project_id, check) = stored_check(&database);
        QualityRepository::new(database.clone())
            .add_run(&CheckRunDraft {
                id: "historical-run".into(),
                project_id: project_id.clone(),
                check_id: check.id.clone(),
                revision_id: check.latest_revision_id.clone(),
                outcome: CheckOutcome::Fail,
                failure_count: Some(2),
                duration_ms: 9,
                observed_at: "2026-09-06T12:00:00Z".into(),
                error_code: None,
            })
            .unwrap();
        let mut changed = draft(CheckOptions::NotNull, NullPolicy::FailOnNull);
        changed.project_id = project_id.clone();
        changed.name = "changed check".into();
        let updated = QualityRepository::new(database.clone())
            .update(&check.id, &changed)
            .unwrap();
        let engine = Arc::new(FakeEngine::new(Vec::new(), 0));
        let restarted = Arc::new(QualityCoordinator::new(engine.clone(), database));

        let detail = restarted.run_detail(&project_id, "historical-run").unwrap();
        assert_eq!(detail.revision_number, 1);
        assert_eq!(detail.current_revision_number, 2);
        assert!(!detail.is_latest_revision);
        assert_eq!(detail.check_name, "check");
        assert_eq!(detail.run.revision_id, check.latest_revision_id);

        let preview = restarted
            .start_failure_preview(&project_id, "historical-run")
            .unwrap();
        assert_eq!(preview.revision_id, check.latest_revision_id);
        assert_eq!(engine.submitted.lock().unwrap()[0].1, detail.failure_sql);
        restarted.release_preview(&preview.result_id).unwrap();
        assert_eq!(
            engine.released.lock().unwrap().as_slice(),
            &[preview.result_id]
        );

        let rerun = restarted
            .run_revision(&project_id, "historical-run")
            .unwrap();
        assert_eq!(rerun.revision_id, check.latest_revision_id);
        assert_ne!(rerun.revision_id, updated.latest_revision_id);
        assert_eq!(engine.submitted.lock().unwrap()[1].1, detail.count_sql);
    }

    #[test]
    fn coordinator_retains_only_the_bounded_terminal_execution_window() {
        let database = MetadataDb::open_in_memory().unwrap();
        let (project_id, check) = stored_check(&database);
        let coordinator =
            QualityCoordinator::new(Arc::new(FakeEngine::new(Vec::new(), 0)), database);
        for index in 0..=MAX_TERMINAL_EXECUTIONS {
            coordinator
                .record_submission_error(
                    check.clone(),
                    check.latest_revision_id.clone(),
                    format!("invalid-{index}"),
                )
                .unwrap();
        }
        let runs = coordinator.executions.lock().unwrap();
        assert_eq!(runs.len(), MAX_TERMINAL_EXECUTIONS);
        assert!(runs.values().all(|run| run.project_id == project_id));
        assert!(runs
            .values()
            .all(|run| run.state == CheckExecutionState::Error));
    }

    fn real_engine_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/debug")
            .join(if cfg!(windows) {
                "tarik-engine-duckdb.exe"
            } else {
                "tarik-engine-duckdb"
            })
    }

    #[test]
    fn real_sidecar_runs_failure_preview_release_repair_and_pass() {
        let engine_path = real_engine_path();
        if !engine_path.exists() {
            eprintln!("real sidecar not built; skipping quality integration test");
            return;
        }
        let root =
            std::env::temp_dir().join(format!("tarik-quality-real-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let duckdb = root.join("quality.duckdb");
        let engine = Arc::new(EngineManager::new(engine_path, root.join("results")));
        engine.open_session(&duckdb).unwrap();
        engine
            .execute_query(
                "quality-fixture",
                "CREATE TABLE orders(id BIGINT); INSERT INTO orders VALUES (1), (NULL), (NULL)",
            )
            .unwrap();
        for _ in 0..400 {
            if engine
                .query_status("quality-fixture")
                .unwrap()
                .is_some_and(|status| {
                    matches!(
                        status.state,
                        ExecutionState::Succeeded | ExecutionState::Failed
                    )
                })
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        engine
            .execute_query(
                "quality-matrix-fixture",
                "ALTER TABLE orders ADD COLUMN code VARCHAR; ALTER TABLE orders ADD COLUMN amount INTEGER; ALTER TABLE orders ADD COLUMN parent_id BIGINT; ALTER TABLE orders ADD COLUMN seen_at TIMESTAMP; UPDATE orders SET code = CASE WHEN id = 1 THEN 'ok' ELSE 'bad' END, amount = CASE WHEN id = 1 THEN 5 ELSE 11 END, parent_id = CASE WHEN id = 1 THEN 1 ELSE 9 END, seen_at = CASE WHEN id = 1 THEN current_timestamp ELSE current_timestamp - INTERVAL '2 day' END; CREATE TABLE parents(id BIGINT); INSERT INTO parents VALUES (1)",
            )
            .unwrap();
        for _ in 0..400 {
            if engine
                .query_status("quality-matrix-fixture")
                .unwrap()
                .is_some_and(|status| status.state == ExecutionState::Succeeded)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let catalog = engine.catalog().unwrap();
        let object = catalog
            .objects
            .iter()
            .find(|object| object.name == "orders")
            .unwrap();
        let matrix_target = QualityTarget {
            database: object.database.clone(),
            schema: object.schema.clone(),
            object: object.name.clone(),
            columns: Vec::new(),
        };
        let matrix_cases = vec![
            (
                CheckOptions::NotNull,
                vec!["id"],
                NullPolicy::FailOnNull,
                2u64,
            ),
            (
                CheckOptions::Unique,
                vec!["id"],
                NullPolicy::PassOnNull,
                0u64,
            ),
            (
                CheckOptions::AcceptedValues {
                    values: vec![Value::String("ok".into())],
                },
                vec!["code"],
                NullPolicy::FailOnNull,
                2u64,
            ),
            (
                CheckOptions::Range {
                    minimum: Some(Value::from(0)),
                    maximum: Some(Value::from(10)),
                    inclusive_minimum: true,
                    inclusive_maximum: true,
                },
                vec!["amount"],
                NullPolicy::FailOnNull,
                2u64,
            ),
            (
                CheckOptions::Relationship {
                    parent: QualityTarget {
                        database: object.database.clone(),
                        schema: object.schema.clone(),
                        object: "parents".into(),
                        columns: vec![],
                    },
                    parent_columns: vec!["id".into()],
                },
                vec!["parent_id"],
                NullPolicy::PassOnNull,
                2u64,
            ),
            (
                CheckOptions::Freshness {
                    maximum_age_seconds: 86_400,
                },
                vec!["seen_at"],
                NullPolicy::FailOnNull,
                0u64,
            ),
            (
                CheckOptions::CustomSql {
                    sql: format!(
                        "SELECT * FROM {}.{}.{} WHERE code = 'bad'",
                        quote(&object.database).unwrap(),
                        quote(&object.schema).unwrap(),
                        quote(&object.name).unwrap()
                    ),
                },
                vec![],
                NullPolicy::PassOnNull,
                2u64,
            ),
        ];
        for (index, (options, columns, null_policy, expected)) in
            matrix_cases.into_iter().enumerate()
        {
            let mut candidate = draft(options, null_policy);
            candidate.target = matrix_target.clone();
            candidate.target.columns = columns.into_iter().map(str::to_string).collect();
            let compiled = compile_check(&candidate).unwrap();
            let count_id = format!("quality-matrix-count-{index}");
            engine
                .execute_query(&count_id, &compiled.count_sql)
                .unwrap();
            for _ in 0..400 {
                if engine
                    .query_status(&count_id)
                    .unwrap()
                    .is_some_and(|status| status.state == ExecutionState::Succeeded)
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            let count_page = engine.result_page(&count_id, 0, 1).unwrap();
            assert_eq!(
                json_u64(count_page["rows"][0][0].clone()),
                Some(expected),
                "{}",
                compiled.count_sql
            );
            engine.release_result(&count_id).unwrap();
            let preview_id = format!("quality-matrix-preview-{index}");
            engine
                .execute_query(&preview_id, &compiled.preview_sql)
                .unwrap();
            let preview_terminal = (0..400)
                .find_map(|_| {
                    let status = engine.query_status(&preview_id).unwrap()?;
                    if status.state == ExecutionState::Succeeded {
                        Some(status)
                    } else {
                        std::thread::sleep(Duration::from_millis(5));
                        None
                    }
                })
                .unwrap();
            assert_eq!(
                preview_terminal.result.unwrap().row_count,
                expected,
                "{}",
                compiled.preview_sql
            );
            engine.release_result(&preview_id).unwrap();
        }

        let database = MetadataDb::open(root.join("metadata.sqlite")).unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert("Quality", &duckdb, ProjectOwnership::External)
            .unwrap();
        let check = QualityRepository::new(database.clone())
            .create(&QualityCheckDraft {
                project_id: project.id.clone(),
                name: "Order id required".into(),
                target: QualityTarget {
                    database: object.database.clone(),
                    schema: object.schema.clone(),
                    object: object.name.clone(),
                    columns: vec!["id".into()],
                },
                options: CheckOptions::NotNull,
                null_policy: NullPolicy::FailOnNull,
                severity: CheckSeverity::Critical,
                enabled: true,
            })
            .unwrap();
        let coordinator = Arc::new(
            QualityCoordinator::new(engine.clone(), database.clone())
                .with_poll_interval(Duration::from_millis(5)),
        );
        let submitted = coordinator.run_check(&project.id, &check.id).unwrap();
        for _ in 0..400 {
            if coordinator
                .status(&submitted.run_id)
                .is_some_and(|run| run.state == CheckExecutionState::Failed)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let failed = coordinator.status(&submitted.run_id).unwrap();
        assert_eq!(failed.failure_count, Some(2));
        let preview = coordinator
            .start_failure_preview(&project.id, &submitted.run_id)
            .unwrap();
        let preview_status = (0..400)
            .find_map(|_| {
                let status = coordinator.preview_status(&preview.result_id).unwrap()?;
                if status.state == ExecutionState::Succeeded {
                    Some(status)
                } else {
                    std::thread::sleep(Duration::from_millis(5));
                    None
                }
            })
            .unwrap();
        assert_eq!(preview_status.result.as_ref().unwrap().row_count, 2);
        let page = engine.result_page(&preview.result_id, 0, 500).unwrap();
        assert_eq!(page["rows"].as_array().unwrap().len(), 2);
        coordinator.release_preview(&preview.result_id).unwrap();

        engine
            .execute_query(
                "quality-repair",
                "UPDATE orders SET id = 2 WHERE id IS NULL",
            )
            .unwrap();
        for _ in 0..400 {
            if engine
                .query_status("quality-repair")
                .unwrap()
                .is_some_and(|status| status.state == ExecutionState::Succeeded)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let rerun = coordinator.run_check(&project.id, &check.id).unwrap();
        for _ in 0..400 {
            if coordinator
                .status(&rerun.run_id)
                .is_some_and(|run| run.state == CheckExecutionState::Passed)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(
            coordinator.status(&rerun.run_id).unwrap().failure_count,
            Some(0)
        );
        let history = QualityRepository::new(database)
            .history(&project.id, Some(&check.id), 0, 20)
            .unwrap();
        assert_eq!(history.entries.len(), 2);
        engine.close_session().unwrap();
        engine.shutdown();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn compiler_covers_all_first_party_check_shapes() {
        let cases = vec![
            draft(CheckOptions::NotNull, NullPolicy::FailOnNull),
            draft(CheckOptions::Unique, NullPolicy::PassOnNull),
            draft(
                CheckOptions::Range {
                    minimum: Some(Value::from(1)),
                    maximum: Some(Value::from(10)),
                    inclusive_minimum: true,
                    inclusive_maximum: false,
                },
                NullPolicy::PassOnNull,
            ),
            draft(
                CheckOptions::Freshness {
                    maximum_age_seconds: 3600,
                },
                NullPolicy::FailOnNull,
            ),
            draft(
                CheckOptions::CustomSql {
                    sql: "SELECT 1 WHERE false".into(),
                },
                NullPolicy::PassOnNull,
            ),
        ];
        for case in cases {
            let compiled = compile_check(&case).unwrap();
            assert!(compiled
                .count_sql
                .to_ascii_lowercase()
                .starts_with("select"));
            assert!(!compiled.preview_sql.is_empty());
        }
    }
}

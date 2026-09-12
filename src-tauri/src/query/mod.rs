//! Typed query execution lifecycle.
//!
//! The desktop submits immutable SQL snapshots to the engine as asynchronous
//! jobs, observes lifecycle transitions through short `query.status` polls,
//! and writes exactly one durable history entry per terminal state.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;
use tarik_engine_protocol::{ExecutionState, ExecutionStatus};

pub mod commands;
use crate::metadata::{
    queries::{ExecutionStatus as HistoryStatus, QueriesRepository, QueryHistoryEntry},
    MetadataDb,
};

/// How often the coordinator asks the engine for a job status.
const POLL_INTERVAL: Duration = Duration::from_millis(150);
/// Consecutive engine failures tolerated before an execution is marked lost.
const MAX_POLL_FAILURES: u32 = 3;

/// Engine operations the coordinator depends on. The real manager talks to
/// the sidecar; tests substitute scripted engines without changing the graph.
pub trait EngineExecutor: Send + Sync + 'static {
    fn execute(&self, execution_id: &str, sql: &str) -> Result<(), String>;
    fn status(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String>;
    fn cancel(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionErrorView {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionView {
    pub execution_id: String,
    pub project_id: String,
    pub tab_id: String,
    pub sql: String,
    pub state: ExecutionState,
    pub duration_ms: u64,
    pub rows_produced: Option<u64>,
    pub rows_affected: Option<u64>,
    pub error: Option<ExecutionErrorView>,
    /// Bounded result metadata once the execution succeeded with a row set.
    pub result_id: Option<String>,
    pub row_total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityExecutionSummary {
    pub execution_id: String,
    pub project_id: String,
    pub tab_id: String,
    pub state: ExecutionState,
    pub duration_ms: u64,
    pub rows_produced: Option<u64>,
    pub rows_affected: Option<u64>,
    pub error: Option<ExecutionErrorView>,
    pub result_id: Option<String>,
    pub row_total: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopQueryDetail {
    pub execution_id: String,
    pub sql: String,
}

struct ExecutionRecord {
    project_id: String,
    tab_id: String,
    sql: String,
    state: ExecutionState,
    duration_ms: u64,
    rows_produced: Option<u64>,
    rows_affected: Option<u64>,
    error: Option<ExecutionErrorView>,
    result_id: Option<String>,
    row_total: Option<u64>,
    history_written: bool,
}

impl ExecutionRecord {
    fn view(&self, execution_id: &str) -> ExecutionView {
        ExecutionView {
            execution_id: execution_id.to_string(),
            project_id: self.project_id.clone(),
            tab_id: self.tab_id.clone(),
            sql: self.sql.clone(),
            state: self.state,
            duration_ms: self.duration_ms,
            rows_produced: self.rows_produced,
            rows_affected: self.rows_affected,
            error: self.error.clone(),
            result_id: self.result_id.clone(),
            row_total: self.row_total,
        }
    }
}

pub struct QueryCoordinator {
    engine: Arc<dyn EngineExecutor>,
    database: MetadataDb,
    executions: Mutex<HashMap<String, ExecutionRecord>>,
    latest_by_tab: Mutex<HashMap<(String, String), String>>,
    poll_interval: Duration,
}

impl QueryCoordinator {
    pub fn new(engine: Arc<dyn EngineExecutor>, database: MetadataDb) -> Self {
        Self {
            engine,
            database,
            executions: Mutex::new(HashMap::new()),
            latest_by_tab: Mutex::new(HashMap::new()),
            poll_interval: POLL_INTERVAL,
        }
    }

    #[cfg(test)]
    fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    /// Submit an immutable SQL snapshot. Returns the queued view; the poller
    /// thread drives the execution to a terminal state and persists history.
    pub fn execute(
        self: &Arc<Self>,
        project_id: &str,
        tab_id: &str,
        sql: &str,
    ) -> Result<ExecutionView, String> {
        if sql.trim().is_empty() {
            return Err("query.empty".to_string());
        }
        // Do not submit an engine job whose terminal history cannot satisfy
        // SQLite's project foreign key. The command layer also checks the
        // active project; this keeps direct/test callers safe.
        let project_exists =
            crate::metadata::projects::ProjectsRepository::new(self.database.clone())
                .find(project_id)
                .map_err(|error| error.to_string())?
                .is_some();
        if !project_exists {
            return Err(format!("query.project_missing: {project_id}"));
        }
        let execution_id = uuid::Uuid::new_v4().to_string();
        let mut executions = self
            .executions
            .lock()
            .map_err(|_| "execution registry poisoned".to_string())?;
        executions.insert(
            execution_id.clone(),
            ExecutionRecord {
                project_id: project_id.to_string(),
                tab_id: tab_id.to_string(),
                sql: sql.to_string(),
                state: ExecutionState::Queued,
                duration_ms: 0,
                rows_produced: None,
                rows_affected: None,
                error: None,
                result_id: None,
                row_total: None,
                history_written: false,
            },
        );
        drop(executions);
        self.latest_by_tab
            .lock()
            .map_err(|_| "latest execution registry poisoned".to_string())?
            .insert(
                (project_id.to_string(), tab_id.to_string()),
                execution_id.clone(),
            );

        let submitted = match self.engine.execute(&execution_id, sql) {
            Ok(()) => true,
            Err(error) => {
                self.mark_terminal(
                    &execution_id,
                    ExecutionState::Failed,
                    Some(ExecutionErrorView {
                        code: "query.rejected".into(),
                        message: error,
                    }),
                );
                false
            }
        };
        // A rejected submission is already terminal. Polling it would observe
        // a missing engine job and attempt to persist the same history ID a
        // second time.
        if submitted {
            self.spawn_poller(execution_id.clone());
        }
        self.view(&execution_id)
            .ok_or_else(|| "execution registry poisoned".to_string())
    }

    pub fn status(&self, execution_id: &str) -> Option<ExecutionView> {
        self.view(execution_id)
    }

    pub fn activity_detail(&self, execution_id: &str) -> Result<DesktopQueryDetail, String> {
        let executions = self
            .executions
            .lock()
            .map_err(|_| "execution registry poisoned".to_string())?;
        let record = executions
            .get(execution_id)
            .ok_or_else(|| "query.execution_missing: Refresh Activity.".to_string())?;
        Ok(DesktopQueryDetail {
            execution_id: execution_id.to_string(),
            sql: truncate_utf8(&record.sql, 256 * 1024),
        })
    }

    pub fn activity(&self, project_id: &str) -> Vec<ActivityExecutionSummary> {
        let Ok(executions) = self.executions.lock() else {
            return Vec::new();
        };
        let mut rows = executions
            .iter()
            .filter(|(_, record)| record.project_id == project_id)
            .map(|(id, record)| ActivityExecutionSummary {
                execution_id: id.clone(),
                project_id: record.project_id.clone(),
                tab_id: record.tab_id.clone(),
                state: record.state,
                duration_ms: record.duration_ms,
                rows_produced: record.rows_produced,
                rows_affected: record.rows_affected,
                error: record.error.clone(),
                result_id: record.result_id.clone(),
                row_total: record.row_total,
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| right.execution_id.cmp(&left.execution_id));
        rows.truncate(64);
        rows
    }

    pub fn latest_for_tab(&self, project_id: &str, tab_id: &str) -> Option<ExecutionView> {
        let execution_id = self
            .latest_by_tab
            .lock()
            .ok()?
            .get(&(project_id.to_string(), tab_id.to_string()))?
            .clone();
        self.view(&execution_id)
    }

    /// Request cancellation; the poller observes the resulting terminal state.
    pub fn cancel(self: &Arc<Self>, execution_id: &str) -> Result<ExecutionView, String> {
        self.engine.cancel(execution_id)?;
        self.view(execution_id)
            .ok_or_else(|| "execution registry poisoned".to_string())
    }

    pub fn cancel_all(self: &Arc<Self>) -> u64 {
        let ids = self
            .executions
            .lock()
            .map(|executions| {
                executions
                    .iter()
                    .filter(|(_, record)| {
                        matches!(
                            record.state,
                            ExecutionState::Queued | ExecutionState::Running
                        )
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in &ids {
            let _ = self.engine.cancel(id);
        }
        ids.len() as u64
    }

    pub fn has_active(&self) -> bool {
        self.executions
            .lock()
            .map(|executions| {
                executions.values().any(|record| {
                    matches!(
                        record.state,
                        ExecutionState::Queued | ExecutionState::Running
                    )
                })
            })
            .unwrap_or(false)
    }

    /// Drop the tracked execution for a closed editor tab. Terminal history
    /// already persisted is untouched.
    pub fn forget(&self, tab_id: &str) -> Result<(), String> {
        let mut executions = self
            .executions
            .lock()
            .map_err(|_| "execution registry poisoned".to_string())?;
        executions.retain(|_, record| record.tab_id != tab_id);
        drop(executions);
        self.latest_by_tab
            .lock()
            .map_err(|_| "latest execution registry poisoned".to_string())?
            .retain(|(_, tracked_tab_id), _| tracked_tab_id != tab_id);
        Ok(())
    }

    fn view(&self, execution_id: &str) -> Option<ExecutionView> {
        let executions = self.executions.lock().ok()?;
        executions
            .get(execution_id)
            .map(|record| record.view(execution_id))
    }

    fn spawn_poller(self: &Arc<Self>, execution_id: String) {
        let coordinator = Arc::clone(self);
        let result = thread::Builder::new()
            .name(format!("tarik-poll-{execution_id}"))
            .spawn({
                let execution_id = execution_id.clone();
                move || coordinator.poll_until_terminal(&execution_id)
            });
        if let Err(error) = result {
            // Without a poller the execution can never terminate; record that.
            let _ = error;
            self.mark_terminal(
                &execution_id,
                ExecutionState::Failed,
                Some(ExecutionErrorView {
                    code: "query.poller".into(),
                    message: "could not start status poller".into(),
                }),
            );
        }
    }

    fn poll_until_terminal(self: &Arc<Self>, execution_id: &str) {
        let mut failures: u32 = 0;
        loop {
            thread::sleep(self.poll_interval);
            match self.engine.status(execution_id) {
                Ok(Some(status)) => match status.state {
                    ExecutionState::Queued | ExecutionState::Running => {
                        failures = 0;
                        if let Ok(mut executions) = self.executions.lock() {
                            if let Some(record) = executions.get_mut(execution_id) {
                                // A late queued/running poll cannot roll a
                                // terminal execution backwards.
                                if !record.history_written {
                                    record.state = status.state;
                                    record.duration_ms = status.duration_ms;
                                }
                            }
                        }
                    }
                    ExecutionState::Succeeded => {
                        if let Ok(mut executions) = self.executions.lock() {
                            if let Some(record) = executions.get_mut(execution_id) {
                                record.rows_produced = status.rows_produced;
                                record.rows_affected = status.rows_affected;
                                record.result_id =
                                    status.result.as_ref().map(|r| r.result_id.clone());
                                record.row_total = status.result.as_ref().map(|r| r.row_count);
                            }
                        }
                        self.mark_terminal(execution_id, ExecutionState::Succeeded, None);
                        break;
                    }
                    ExecutionState::Failed | ExecutionState::Cancelled => {
                        let error = status.error.as_ref().map(|error| ExecutionErrorView {
                            code: error.code.clone(),
                            message: error.message.clone(),
                        });
                        self.mark_terminal(execution_id, status.state, error);
                        break;
                    }
                },
                Ok(None) => {
                    self.mark_terminal(
                        execution_id,
                        ExecutionState::Failed,
                        Some(ExecutionErrorView {
                            code: "execution.lost".into(),
                            message: "the engine no longer knows this execution".into(),
                        }),
                    );
                    break;
                }
                Err(_) => {
                    failures += 1;
                    if failures >= MAX_POLL_FAILURES {
                        self.mark_terminal(
                            execution_id,
                            ExecutionState::Failed,
                            Some(ExecutionErrorView {
                                code: "engine.unreachable".into(),
                                message: "the engine stopped answering status polls".into(),
                            }),
                        );
                        break;
                    }
                }
            }
        }
    }

    fn mark_terminal(
        &self,
        execution_id: &str,
        state: ExecutionState,
        error: Option<ExecutionErrorView>,
    ) {
        let history = {
            let mut executions = match self.executions.lock() {
                Ok(executions) => executions,
                Err(_) => return,
            };
            let Some(record) = executions.get_mut(execution_id) else {
                return;
            };
            // First terminal transition owns the durable history write. Later
            // status observations and repeated cancellation are idempotent and
            // cannot overwrite the terminal view or insert the same ID again.
            if record.history_written {
                return;
            }
            let status = match state {
                ExecutionState::Succeeded => HistoryStatus::Succeeded,
                ExecutionState::Failed => HistoryStatus::Failed,
                ExecutionState::Cancelled => HistoryStatus::Cancelled,
                ExecutionState::Queued | ExecutionState::Running => return,
            };
            record.state = state;
            record.error = error;
            record.history_written = true;
            let error = record.error.clone();
            QueryHistoryEntry {
                id: execution_id.to_string(),
                project_id: record.project_id.clone(),
                sql_text: record.sql.clone(),
                status,
                duration_ms: Some(record.duration_ms.max(1)),
                returned_rows: if state == ExecutionState::Succeeded {
                    record.rows_produced
                } else {
                    None
                },
                error_code: error.as_ref().map(|e| e.code.clone()),
                error_message: error.as_ref().map(|e| e.message.clone()),
                executed_at: chrono::Utc::now().to_rfc3339(),
            }
        };
        let repository = QueriesRepository::new(self.database.clone());
        if let Err(error) = repository.add_history(&history) {
            eprintln!("tarik: could not persist query history: {error}");
        }
    }
}

fn truncate_utf8(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_string();
    }
    let mut end = maximum_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n-- SQL detail truncated by Tarik", &value[..end])
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, sync::Mutex as StdMutex, time::Duration};

    use tarik_engine_protocol::{ErrorEnvelope, ExecutionState};

    use super::*;

    struct FakeEngine {
        execute_error: Option<String>,
        statuses: StdMutex<VecDeque<ExecutionStatus>>,
        last: StdMutex<Option<ExecutionStatus>>,
        cancel_called: StdMutex<bool>,
    }

    impl FakeEngine {
        fn new(statuses: Vec<ExecutionStatus>) -> Self {
            Self {
                execute_error: None,
                statuses: StdMutex::new(statuses.into()),
                last: StdMutex::new(None),
                cancel_called: StdMutex::new(false),
            }
        }

        fn with_execute_error(mut self, error: &str) -> Self {
            self.execute_error = Some(error.to_string());
            self
        }
    }

    impl EngineExecutor for FakeEngine {
        fn execute(&self, _execution_id: &str, _sql: &str) -> Result<(), String> {
            match &self.execute_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }

        fn status(&self, execution_id: &str) -> Result<Option<ExecutionStatus>, String> {
            let mut statuses = self.statuses.lock().unwrap();
            let status = if let Some(next) = statuses.pop_front() {
                *self.last.lock().unwrap() = Some(next.clone());
                next
            } else {
                match self.last.lock().unwrap().clone() {
                    Some(status) => status,
                    None => {
                        return Ok(None);
                    }
                }
            };
            let _ = execution_id;
            Ok(Some(status))
        }

        fn cancel(&self, _execution_id: &str) -> Result<Option<ExecutionStatus>, String> {
            *self.cancel_called.lock().unwrap() = true;
            Ok(self.last.lock().unwrap().clone())
        }
    }

    fn status(execution_id: &str, state: ExecutionState) -> ExecutionStatus {
        ExecutionStatus {
            execution_id: execution_id.to_string(),
            state,
            duration_ms: 5,
            rows_produced: (state == ExecutionState::Succeeded).then_some(42),
            rows_affected: None,
            error: (state == ExecutionState::Failed)
                .then(|| ErrorEnvelope::new("sql.parse", "syntax error near FROM")),
            result: None,
        }
    }

    fn coordinator(engine: FakeEngine) -> (Arc<QueryCoordinator>, String) {
        let database = crate::metadata::MetadataDb::open_in_memory().unwrap();
        // History rows reference projects; use the generated project id.
        let project = crate::metadata::projects::ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                std::path::Path::new("/tmp/test.duckdb"),
                crate::metadata::projects::ProjectOwnership::External,
            )
            .unwrap();
        let coordinator = Arc::new(
            QueryCoordinator::new(Arc::new(engine), database)
                .with_poll_interval(Duration::from_millis(5)),
        );
        (coordinator, project.id)
    }

    fn coordinator_for(engine: Arc<FakeEngine>) -> (Arc<QueryCoordinator>, String) {
        let database = crate::metadata::MetadataDb::open_in_memory().unwrap();
        let project = crate::metadata::projects::ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                std::path::Path::new("/tmp/test.duckdb"),
                crate::metadata::projects::ProjectOwnership::External,
            )
            .unwrap();
        let coordinator = Arc::new(
            QueryCoordinator::new(engine, database).with_poll_interval(Duration::from_millis(5)),
        );
        (coordinator, project.id)
    }

    fn wait_terminal(coordinator: &Arc<QueryCoordinator>, execution_id: &str) -> ExecutionView {
        for _ in 0..400 {
            if let Some(view) = coordinator.status(execution_id) {
                if matches!(
                    view.state,
                    ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
                ) {
                    return view;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("execution {execution_id} did not reach a terminal state");
    }

    fn history_count(
        coordinator: &Arc<QueryCoordinator>,
        project_id: &str,
    ) -> Vec<QueryHistoryEntry> {
        QueriesRepository::new(coordinator.database.clone())
            .list_history(project_id, None, 10)
            .unwrap()
    }

    #[test]
    fn succeeded_execution_writes_exactly_one_history_entry() {
        let engine = FakeEngine::new(vec![
            status("e1", ExecutionState::Queued),
            status("e1", ExecutionState::Running),
            status("e1", ExecutionState::Succeeded),
        ]);
        let (coordinator, project_id) = coordinator(engine);
        let view = coordinator
            .execute(&project_id, "tab1", "SELECT 1")
            .unwrap();
        assert_eq!(view.state, ExecutionState::Queued);

        let view = wait_terminal(&coordinator, view.execution_id.as_str());
        assert_eq!(view.state, ExecutionState::Succeeded);
        assert_eq!(view.rows_produced, Some(42));

        let history = history_count(&coordinator, &project_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].sql_text, "SELECT 1");
        assert_eq!(history[0].status, HistoryStatus::Succeeded);
        assert_eq!(history[0].returned_rows, Some(42));
        assert!(history[0].duration_ms.unwrap() >= 1);

        // The view stays queryable after the terminal state.
        assert_eq!(
            coordinator
                .status(view.execution_id.as_str())
                .unwrap()
                .state,
            ExecutionState::Succeeded
        );
    }

    #[test]
    fn latest_execution_is_restorable_by_project_and_tab() {
        let engine = FakeEngine::new(vec![status("e1", ExecutionState::Succeeded)]);
        let (coordinator, project_id) = coordinator(engine);
        let first = coordinator
            .execute(&project_id, "tab1", "SELECT 1")
            .unwrap();
        let restored = coordinator
            .latest_for_tab(&project_id, "tab1")
            .expect("latest execution should remain coordinator-owned");
        assert_eq!(restored.execution_id, first.execution_id);
        assert_eq!(restored.sql, "SELECT 1");
        assert!(coordinator.latest_for_tab(&project_id, "missing").is_none());
        coordinator.forget("tab1").unwrap();
        assert!(coordinator.latest_for_tab(&project_id, "tab1").is_none());
    }

    #[test]
    fn failed_execution_persists_structured_error() {
        let engine = FakeEngine::new(vec![
            status("e1", ExecutionState::Queued),
            status("e1", ExecutionState::Failed),
        ]);
        let (coordinator, project_id) = coordinator(engine);
        let view = coordinator
            .execute(&project_id, "tab1", "SELECT FROM")
            .unwrap();
        let view = wait_terminal(&coordinator, view.execution_id.as_str());

        assert_eq!(view.state, ExecutionState::Failed);
        assert_eq!(view.error.as_ref().unwrap().code, "sql.parse");
        let history = history_count(&coordinator, &project_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, HistoryStatus::Failed);
        assert_eq!(history[0].error_code.as_deref(), Some("sql.parse"));
        assert_eq!(history[0].returned_rows, None);
    }

    #[test]
    fn rejected_submission_records_failure_without_polling() {
        let engine = FakeEngine::new(vec![]).with_execute_error("engine queue is closed");
        let (coordinator, project_id) = coordinator(engine);
        let view = coordinator
            .execute(&project_id, "tab1", "SELECT 1")
            .unwrap();

        assert_eq!(view.state, ExecutionState::Failed);
        assert_eq!(view.error.as_ref().unwrap().code, "query.rejected");
        let history = history_count(&coordinator, &project_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, HistoryStatus::Failed);
    }

    #[test]
    fn empty_sql_is_rejected_without_history() {
        let engine = FakeEngine::new(vec![]);
        let (coordinator, project_id) = coordinator(engine);
        let error = coordinator.execute(&project_id, "tab1", "   ").unwrap_err();
        assert_eq!(error, "query.empty");
        assert!(history_count(&coordinator, &project_id).is_empty());
    }

    #[test]
    fn unknown_project_is_rejected_before_engine_submission() {
        let engine = Arc::new(FakeEngine::new(vec![]));
        let (coordinator, _) = coordinator_for(engine.clone());

        let error = coordinator
            .execute("not-a-project", "tab1", "SELECT 1")
            .unwrap_err();

        assert_eq!(error, "query.project_missing: not-a-project");
        assert!(engine.last.lock().unwrap().is_none());
    }

    #[test]
    fn repeated_terminal_observation_writes_history_exactly_once() {
        let engine = FakeEngine::new(vec![status("e1", ExecutionState::Running)]);
        let (coordinator, project_id) = coordinator(engine);
        let view = coordinator
            .execute(&project_id, "tab1", "SELECT 1")
            .unwrap();

        coordinator.mark_terminal(&view.execution_id, ExecutionState::Succeeded, None);
        coordinator.mark_terminal(
            &view.execution_id,
            ExecutionState::Failed,
            Some(ExecutionErrorView {
                code: "execution.late".into(),
                message: "late duplicate terminal observation".into(),
            }),
        );

        let history = history_count(&coordinator, &project_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, HistoryStatus::Succeeded);
        assert_eq!(history[0].error_code, None);
        assert_eq!(
            coordinator.status(&view.execution_id).unwrap().state,
            ExecutionState::Succeeded
        );
    }

    #[test]
    fn cancelled_execution_reaches_history_via_poller() {
        let engine = Arc::new(FakeEngine::new(vec![
            status("e1", ExecutionState::Queued),
            status("e1", ExecutionState::Running),
            status("e1", ExecutionState::Cancelled),
        ]));
        let (coordinator, project_id) = coordinator_for(engine.clone());
        let view = coordinator
            .execute(&project_id, "tab1", "SELECT 1")
            .unwrap();
        let cancelled = coordinator.cancel(view.execution_id.as_str()).unwrap();
        let view = wait_terminal(&coordinator, view.execution_id.as_str());

        assert!(matches!(
            cancelled.state,
            ExecutionState::Queued | ExecutionState::Running
        ));
        assert_eq!(view.state, ExecutionState::Cancelled);
        assert!(*engine.cancel_called.lock().unwrap());
        let history = history_count(&coordinator, &project_id);
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].status, HistoryStatus::Cancelled);
    }

    #[test]
    fn forgetting_a_tab_drops_its_executions() {
        let engine = FakeEngine::new(vec![status("e1", ExecutionState::Succeeded)]);
        let (coordinator, project_id) = coordinator(engine);
        let view = coordinator
            .execute(&project_id, "tab1", "SELECT 1")
            .unwrap();
        wait_terminal(&coordinator, view.execution_id.as_str());
        coordinator.forget("tab1").unwrap();
        assert!(coordinator.status(view.execution_id.as_str()).is_none());
    }
}

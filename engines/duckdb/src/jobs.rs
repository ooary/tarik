//! Asynchronous query jobs.
//!
//! `query.execute` clones the session's DuckDB connection, spawns one worker
//! thread per execution, and returns immediately. `query.status` and
//! `query.cancel` stay short so the protocol loop can always service them.
//! Cancellation uses DuckDB's thread-safe `InterruptHandle`; the DuckDB Arrow
//! iterator reports an interrupt by panicking during fetch, which the worker
//! converts into a clean cancelled terminal state.

use std::{
    collections::{HashMap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use duckdb::arrow::array::{Array, Int32Array, Int64Array, UInt64Array};
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;
use tarik_engine_protocol::{ErrorEnvelope, ExecutionState, ExecutionStatus};

use crate::{error::EngineError, sql::split_statements};

/// Terminal job snapshots kept for `query.status` before pruning.
const TERMINAL_HISTORY_LIMIT: usize = 32;

struct JobRecord {
    session_id: String,
    sql: String,
    connection: Option<Connection>,
    state: ExecutionState,
    queued_at: Instant,
    started_at: Option<Instant>,
    interrupt: Option<Arc<duckdb::InterruptHandle>>,
    cancel_requested: bool,
    status: Option<ExecutionStatus>,
}

#[derive(Default)]
struct SessionQueue {
    pending: VecDeque<String>,
    running: Option<String>,
    worker_alive: bool,
}

#[derive(Default)]
struct Registry {
    jobs: HashMap<String, JobRecord>,
    queues: HashMap<String, SessionQueue>,
    terminal_order: VecDeque<String>,
}

/// Terminal outcome of one executed snapshot.
struct ExecutionOutcome {
    row_count: Option<u64>,
    rows_affected: Option<u64>,
}

impl Registry {
    fn prune_terminal(&mut self) {
        self.terminal_order.pop_front();
        while self.terminal_order.len() > TERMINAL_HISTORY_LIMIT {
            let Some(oldest) = self.terminal_order.pop_front() else {
                break;
            };
            self.jobs.remove(&oldest);
        }
    }
}

/// Registry of asynchronous engine jobs keyed by execution id.
pub struct JobRegistry {
    inner: Mutex<Registry>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Registry::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Registry>, EngineError> {
        self.inner.lock().map_err(|_| EngineError::RegistryPoisoned)
    }

    /// Enqueue one immutable SQL snapshot and spawn its worker immediately.
    /// Returns once the job is queued; the query itself runs asynchronously.
    pub fn execute(
        self: &Arc<Self>,
        session_id: &str,
        execution_id: &str,
        sql: &str,
        connection: Connection,
    ) -> Result<(), EngineError> {
        if sql.trim().is_empty() {
            return Err(EngineError::InvalidQuery("sql text contains no statements"));
        }
        if crate::sql::split_statements(sql).is_empty() {
            return Err(EngineError::InvalidQuery("sql text contains no statements"));
        }
        let mut inner = self.lock()?;
        if inner.jobs.contains_key(execution_id) {
            return Err(EngineError::ExecutionExists(execution_id.to_string()));
        }
        inner.jobs.insert(
            execution_id.to_string(),
            JobRecord {
                session_id: session_id.to_string(),
                sql: sql.to_string(),
                connection: Some(connection),
                state: ExecutionState::Queued,
                queued_at: Instant::now(),
                started_at: None,
                interrupt: None,
                cancel_requested: false,
                status: None,
            },
        );
        let spawn_worker = {
            let queue = inner.queues.entry(session_id.to_string()).or_default();
            queue.pending.push_back(execution_id.to_string());
            !queue.worker_alive
        };
        if spawn_worker {
            if let Some(queue) = inner.queues.get_mut(session_id) {
                queue.worker_alive = true;
            }
            let registry = Arc::clone(self);
            let session = session_id.to_string();
            std::thread::Builder::new()
                .name(format!("tarik-job-{execution_id}"))
                .spawn(move || worker_loop(registry, &session))
                .map_err(|error| EngineError::WorkerSpawn(error.to_string()))?;
        }
        Ok(())
    }

    pub fn status(&self, execution_id: &str) -> Result<ExecutionStatus, EngineError> {
        let inner = self.lock()?;
        let Some(job) = inner.jobs.get(execution_id) else {
            return Err(EngineError::ExecutionMissing(execution_id.to_string()));
        };
        if let Some(status) = &job.status {
            return Ok(status.clone());
        }
        let started = job.started_at.unwrap_or(job.queued_at);
        Ok(ExecutionStatus {
            execution_id: execution_id.to_string(),
            state: job.state,
            duration_ms: started.elapsed().as_millis() as u64,
            rows_produced: None,
            rows_affected: None,
            error: None,
        })
    }

    /// Request cancellation. Queued jobs are removed and become cancelled
    /// immediately; running jobs are interrupted. Repeating a cancel on a
    /// terminal job returns its terminal status unchanged.
    pub fn cancel(&self, execution_id: &str) -> Result<ExecutionStatus, EngineError> {
        {
            let mut inner = self.lock()?;
            // Owned session id for queued removal, taken while the job borrow
            // is live; queue mutation happens after that borrow ends.
            let queued_session: Option<String> = {
                let Some(job) = inner.jobs.get_mut(execution_id) else {
                    return Err(EngineError::ExecutionMissing(execution_id.to_string()));
                };
                match job.state {
                    ExecutionState::Queued => {
                        job.state = ExecutionState::Cancelled;
                        job.status = Some(ExecutionStatus {
                            execution_id: execution_id.to_string(),
                            state: ExecutionState::Cancelled,
                            duration_ms: job.queued_at.elapsed().as_millis() as u64,
                            rows_produced: None,
                            rows_affected: None,
                            error: None,
                        });
                        Some(job.session_id.clone())
                    }
                    ExecutionState::Running => {
                        job.cancel_requested = true;
                        if let Some(interrupt) = job.interrupt.as_ref() {
                            interrupt.interrupt();
                        }
                        None
                    }
                    _ => None,
                }
            };
            if let Some(session_id) = queued_session {
                if let Some(queue) = inner.queues.get_mut(&session_id) {
                    queue.pending.retain(|id| id != execution_id);
                }
                inner.terminal_order.push_back(execution_id.to_string());
                inner.prune_terminal();
            }
        }
        self.status(execution_id)
    }

    /// Cancel queued and running jobs belonging to a session before it closes.
    pub fn cancel_session(&self, session_id: &str) {
        let Ok(inner) = self.inner.lock() else {
            return;
        };
        let execution_ids: Vec<String> = inner
            .jobs
            .iter()
            .filter(|(_, job)| job.session_id == session_id)
            .map(|(id, _)| id.clone())
            .collect();
        drop(inner);
        for execution_id in execution_ids {
            let _ = self.cancel(&execution_id);
        }
    }

    fn set_interrupt(&self, execution_id: &str, interrupt: Arc<duckdb::InterruptHandle>) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(job) = inner.jobs.get_mut(execution_id) {
                job.interrupt = Some(interrupt);
                if job.cancel_requested {
                    if let Some(handle) = job.interrupt.as_ref() {
                        handle.interrupt();
                    }
                }
            }
        }
    }

    fn is_cancel_requested(&self, execution_id: &str) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| inner.jobs.get(execution_id).map(|job| job.cancel_requested))
            .unwrap_or(false)
    }

    fn mark_terminal(
        &self,
        execution_id: &str,
        state: ExecutionState,
        error: Option<ErrorEnvelope>,
        row_count: Option<u64>,
        rows_affected: Option<u64>,
    ) {
        let Ok(mut inner) = self.lock() else {
            return;
        };
        let Some(job) = inner.jobs.get(execution_id) else {
            return;
        };
        let started = job.started_at.unwrap_or(job.queued_at);
        let session_id = job.session_id.clone();
        let status = ExecutionStatus {
            execution_id: execution_id.to_string(),
            state,
            duration_ms: started.elapsed().as_millis() as u64,
            rows_produced: row_count,
            rows_affected,
            error,
        };
        if let Some(job) = inner.jobs.get_mut(execution_id) {
            job.state = state;
            job.status = Some(status);
        }
        inner.terminal_order.push_back(execution_id.to_string());
        inner.prune_terminal();
        if let Some(queue) = inner.queues.get_mut(&session_id) {
            if queue.running.as_deref() == Some(execution_id) {
                queue.running = None;
            }
        }
    }
}

fn worker_loop(registry: Arc<JobRegistry>, session_id: &str) {
    loop {
        let claimed = {
            let mut inner = match registry.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            let Some(queue) = inner.queues.get_mut(session_id) else {
                return;
            };
            let Some(execution_id) = queue.pending.pop_front() else {
                queue.worker_alive = false;
                return;
            };
            queue.running = Some(execution_id.clone());
            match inner.jobs.get_mut(&execution_id) {
                Some(job) => {
                    job.state = ExecutionState::Running;
                    job.started_at = Some(Instant::now());
                    let sql = job.sql.clone();
                    let connection = job.connection.take();
                    (execution_id, sql, connection)
                }
                None => continue,
            }
        };
        let (execution_id, sql, connection) = claimed;
        let Some(mut connection) = connection else {
            registry.mark_terminal(
                &execution_id,
                ExecutionState::Failed,
                Some(ErrorEnvelope::new(
                    "query.rejected",
                    "engine lost the query connection",
                )),
                None,
                None,
            );
            continue;
        };
        run_job(&registry, &execution_id, &sql, &mut connection);
    }
}

fn run_job(registry: &JobRegistry, execution_id: &str, sql: &str, connection: &mut Connection) {
    let interrupt = connection.interrupt_handle();
    registry.set_interrupt(execution_id, interrupt);

    // The DuckDB Arrow iterator panics on fetch failure (including interrupt),
    // so the whole execution runs inside catch_unwind.
    let outcome = catch_unwind(AssertUnwindSafe(|| execute_snapshot(connection, sql)));
    match outcome {
        Ok(Ok(completed)) => {
            registry.mark_terminal(
                execution_id,
                ExecutionState::Succeeded,
                None,
                completed.row_count,
                completed.rows_affected,
            );
        }
        Ok(Err(error)) => {
            registry.mark_terminal(
                execution_id,
                ExecutionState::Failed,
                Some(ErrorEnvelope::new(error.code(), error.to_string())),
                None,
                None,
            );
        }
        Err(payload) => {
            // A requested cancel is a clean cancelled terminal; any other
            // panic is an interrupted execution.
            if registry.is_cancel_requested(execution_id) {
                registry.mark_terminal(execution_id, ExecutionState::Cancelled, None, None, None);
            } else {
                let message = panic_message(payload);
                registry.mark_terminal(
                    execution_id,
                    ExecutionState::Failed,
                    Some(ErrorEnvelope::new("query.interrupted", message)),
                    None,
                    None,
                );
            }
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_string()))
        .unwrap_or_else(|| "engine fetch failure".to_string())
}

/// Execute every statement of one snapshot. The last row-returning statement
/// provides the result; every statement is executed by streaming so large
/// row sets are never materialized up front.
fn execute_snapshot(connection: &Connection, sql: &str) -> Result<ExecutionOutcome, EngineError> {
    let statements = split_statements(sql);
    if statements.is_empty() {
        return Err(EngineError::InvalidQuery("sql text contains no statements"));
    }
    let mut last_rows: Option<u64> = None;
    let mut rows_affected: u64 = 0;
    for statement in statements {
        let mut statement = connection.prepare(&statement)?;
        let mut batches = statement.stream_arrow([])?;
        for batch in batches.by_ref() {
            // DuckDB reports DML as a one-row "Count" result set; surface it
            // as rowsAffected instead of a browsable result.
            if let Some(changed) = dml_count_row(&batch) {
                rows_affected += changed;
            } else {
                last_rows = Some(last_rows.unwrap_or(0) + batch.num_rows() as u64);
            }
        }
    }
    Ok(ExecutionOutcome {
        row_count: last_rows,
        rows_affected: if rows_affected > 0 {
            Some(rows_affected)
        } else {
            None
        },
    })
}

/// Extract the changed-row count from a DuckDB DML count result.
fn dml_count_row(batch: &RecordBatch) -> Option<u64> {
    if batch.num_rows() != 1
        || batch.num_columns() != 1
        || batch.schema().field(0).name() != "Count"
    {
        return None;
    }
    let column = batch.column(0);
    if let Some(values) = column.as_any().downcast_ref::<Int64Array>() {
        return (!values.is_null(0)).then(|| u64::try_from(values.value(0).max(0)).unwrap_or(0));
    }
    if let Some(values) = column.as_any().downcast_ref::<UInt64Array>() {
        return (!values.is_null(0)).then_some(values.value(0));
    }
    if let Some(values) = column.as_any().downcast_ref::<Int32Array>() {
        return (!values.is_null(0))
            .then(|| u64::try_from(i64::from(values.value(0).max(0))).unwrap_or(0));
    }
    None
}

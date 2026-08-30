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
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use duckdb::arrow::array::{Array, Int32Array, Int64Array, UInt64Array};
use duckdb::arrow::record_batch::RecordBatch;
use duckdb::Connection;
use serde_json::{Map, Value};
use tarik_engine_protocol::{ErrorEnvelope, ExecutionState, ExecutionStatus, ResultInfo};

use crate::{error::EngineError, pages, sql::split_statements};

/// Terminal job snapshots kept for `query.status` before pruning.
const TERMINAL_HISTORY_LIMIT: usize = 32;

struct JobRecord {
    session_id: String,
    sql: String,
    connection: Option<Connection>,
    result_root: Option<PathBuf>,
    state: ExecutionState,
    queued_at: Instant,
    started_at: Option<Instant>,
    interrupt: Option<Arc<duckdb::InterruptHandle>>,
    cancel_requested: bool,
    status: Option<ExecutionStatus>,
}

/// Metadata for one published bounded result.
pub struct PublishedResult {
    pub page_dir: PathBuf,
    pub columns: Vec<Map<String, Value>>,
    pub row_count: u64,
    pub row_count_exact: bool,
    pub page_rows: u32,
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
    results: HashMap<String, PublishedResult>,
}

/// Terminal outcome of one executed snapshot.
struct ExecutionOutcome {
    row_count: Option<u64>,
    rows_affected: Option<u64>,
    result: Option<PublishedResult>,
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
        result_root: Option<&Path>,
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
                result_root: result_root.map(Path::to_path_buf),
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
            result: None,
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
                            result: None,
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
        result: Option<PublishedResult>,
    ) {
        let Ok(mut inner) = self.lock() else {
            return;
        };
        let Some(job) = inner.jobs.get(execution_id) else {
            return;
        };
        let started = job.started_at.unwrap_or(job.queued_at);
        let session_id = job.session_id.clone();
        let result_info = result.as_ref().map(|result| ResultInfo {
            result_id: execution_id.to_string(),
            columns: result
                .columns
                .iter()
                .map(|column| {
                    serde_json::from_value::<tarik_engine_protocol::ColumnInfo>(Value::Object(
                        column.clone(),
                    ))
                    .expect("column metadata shape")
                })
                .collect(),
            row_count: result.row_count,
            row_count_exact: result.row_count_exact,
            page_dir: result.page_dir.to_string_lossy().to_string(),
        });
        let status = ExecutionStatus {
            execution_id: execution_id.to_string(),
            state,
            duration_ms: started.elapsed().as_millis() as u64,
            rows_produced: row_count,
            rows_affected,
            error,
            result: result_info,
        };
        if let Some(job) = inner.jobs.get_mut(execution_id) {
            job.state = state;
            job.status = Some(status);
        }
        if let Some(result) = result {
            inner.results.insert(execution_id.to_string(), result);
        }
        inner.terminal_order.push_back(execution_id.to_string());
        inner.prune_terminal();
        if let Some(queue) = inner.queues.get_mut(&session_id) {
            if queue.running.as_deref() == Some(execution_id) {
                queue.running = None;
            }
        }
    }

    /// Read a bounded window of one published result as JSON-safe cells.
    pub fn get_page(
        &self,
        result_id: &str,
        offset: u64,
        max_rows: u32,
    ) -> Result<(PublishedResult, pages::PageRead), EngineError> {
        let (page_dir, page_rows, row_count) = {
            let inner = self.lock()?;
            let Some(result) = inner.results.get(result_id) else {
                return Err(EngineError::ResultMissing(result_id.to_string()));
            };
            (result.page_dir.clone(), result.page_rows, result.row_count)
        };
        let page = pages::read_page_window(&page_dir, page_rows, offset, max_rows, row_count)?;
        let inner = self.lock()?;
        let Some(result) = inner.results.get(result_id) else {
            return Err(EngineError::ResultMissing(result_id.to_string()));
        };
        Ok((clone_result_meta(result), page))
    }

    /// Delete a published result and its page artifacts.
    pub fn release_result(&self, result_id: &str) -> Result<(), EngineError> {
        let page_dir = {
            let mut inner = self.lock()?;
            let Some(result) = inner.results.remove(result_id) else {
                return Err(EngineError::ResultMissing(result_id.to_string()));
            };
            result.page_dir
        };
        pages::discard_dir(&page_dir);
        Ok(())
    }

    /// Remove the temporary directory of a finished (failed/cancelled) job.
    fn cleanup_tmp(&self, execution_id: &str) {
        let tmp = self.lock().ok().and_then(|inner| {
            inner
                .jobs
                .get(execution_id)
                .and_then(|job| job.result_root.as_ref())
                .map(|root| root.join(format!("{execution_id}.tmp")))
        });
        if let Some(tmp) = tmp {
            pages::discard_dir(&tmp);
        }
    }
}

fn clone_result_meta(result: &PublishedResult) -> PublishedResult {
    PublishedResult {
        page_dir: result.page_dir.clone(),
        columns: result.columns.clone(),
        row_count: result.row_count,
        row_count_exact: result.row_count_exact,
        page_rows: result.page_rows,
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
                    let result_root = job.result_root.clone();
                    (execution_id, sql, connection, result_root)
                }
                None => continue,
            }
        };
        let (execution_id, sql, connection, result_root) = claimed;
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
                None,
            );
            continue;
        };
        run_job(
            &registry,
            &execution_id,
            &sql,
            &mut connection,
            result_root.as_deref(),
        );
    }
}

fn run_job(
    registry: &JobRegistry,
    execution_id: &str,
    sql: &str,
    connection: &mut Connection,
    result_root: Option<&Path>,
) {
    let interrupt = connection.interrupt_handle();
    registry.set_interrupt(execution_id, interrupt);

    // The DuckDB Arrow iterator panics on fetch failure (including interrupt),
    // so the whole execution runs inside catch_unwind.
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        execute_snapshot(execution_id, connection, sql, result_root)
    }));
    match outcome {
        Ok(Ok(completed)) => {
            registry.mark_terminal(
                execution_id,
                ExecutionState::Succeeded,
                None,
                completed.row_count,
                completed.rows_affected,
                completed.result,
            );
        }
        Ok(Err(error)) => {
            // An interrupt can surface either as a panic in the Arrow fetch
            // path or as a DuckDB error; both become a clean cancel when we
            // asked for it. Partial page artifacts are discarded.
            if registry.is_cancel_requested(execution_id) {
                registry.cleanup_tmp(execution_id);
                registry.mark_terminal(
                    execution_id,
                    ExecutionState::Cancelled,
                    None,
                    None,
                    None,
                    None,
                );
            } else {
                registry.cleanup_tmp(execution_id);
                registry.mark_terminal(
                    execution_id,
                    ExecutionState::Failed,
                    Some(ErrorEnvelope::new(error.code(), error.to_string())),
                    None,
                    None,
                    None,
                );
            }
        }
        Err(payload) => {
            // A requested cancel is a clean cancelled terminal; any other
            // panic is an interrupted execution.
            registry.cleanup_tmp(execution_id);
            if registry.is_cancel_requested(execution_id) {
                registry.mark_terminal(
                    execution_id,
                    ExecutionState::Cancelled,
                    None,
                    None,
                    None,
                    None,
                );
            } else {
                let message = panic_message(payload);
                registry.mark_terminal(
                    execution_id,
                    ExecutionState::Failed,
                    Some(ErrorEnvelope::new("query.interrupted", message)),
                    None,
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
/// provides the published result; every statement is executed by streaming so
/// large row sets are never materialized up front, and record batches flow
/// straight into bounded page artifacts.
fn execute_snapshot(
    execution_id: &str,
    connection: &Connection,
    sql: &str,
    result_root: Option<&Path>,
) -> Result<ExecutionOutcome, EngineError> {
    let statements = split_statements(sql);
    if statements.is_empty() {
        return Err(EngineError::InvalidQuery("sql text contains no statements"));
    }
    let mut rows_affected: u64 = 0;
    let mut outcome: Option<(u64, PublishedResult)> = None;
    let mut last_counted_rows: u64 = 0;
    let mut any_rows = false;
    for statement in statements {
        let mut statement = connection.prepare(&statement)?;
        let mut batches = statement.stream_arrow([])?;
        let schema = batches.get_schema();
        let is_count_schema = schema.fields().len() == 1 && schema.field(0).name() == "Count";

        // Only the last row-returning statement is published; discard any
        // earlier statement's published pages.
        let mut writer: Option<pages::PageWriter> = if is_count_schema {
            None
        } else if let Some(root) = result_root {
            if let Some((_, previous)) = outcome.take() {
                pages::discard_dir(&previous.page_dir);
            }
            let tmp = root.join(format!("{execution_id}.tmp"));
            Some(pages::PageWriter::create(tmp)?)
        } else {
            None
        };
        let mut statement_rows: u64 = 0;

        for batch in batches.by_ref() {
            // DuckDB reports DML as a one-row "Count" result set; surface it
            // as rowsAffected instead of a browsable result.
            if let Some(changed) = dml_count_row(&batch) {
                rows_affected += changed;
                continue;
            }
            if let Some(page_writer) = writer.as_mut() {
                page_writer.write(&batch)?;
            }
            statement_rows += batch.num_rows() as u64;
        }
        if statement_rows > 0 {
            last_counted_rows = statement_rows;
            any_rows = true;
        }

        if let Some(mut page_writer) = writer {
            let row_count = page_writer.total_rows();
            let page_rows = page_writer.page_rows();
            let columns = pages::column_metadata(&schema);
            if let Some(root) = result_root {
                let final_dir = root.join(execution_id);
                page_writer.publish(&final_dir)?;
                outcome = Some((
                    row_count,
                    PublishedResult {
                        page_dir: final_dir,
                        columns,
                        row_count,
                        row_count_exact: true,
                        page_rows,
                    },
                ));
            } else {
                page_writer.finish()?;
            }
        }
    }
    let (row_count, result) = match outcome {
        Some((row_count, result)) => (Some(row_count), Some(result)),
        // Without a result root only row counts are tracked.
        None if any_rows => (Some(last_counted_rows), None),
        None => (None, None),
    };
    Ok(ExecutionOutcome {
        row_count,
        rows_affected: if rows_affected > 0 {
            Some(rows_affected)
        } else {
            None
        },
        result,
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

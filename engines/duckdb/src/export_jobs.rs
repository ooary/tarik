//! Asynchronous export lifecycle and bounded progress registry.

use std::{
    collections::{HashMap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use duckdb::Connection;
use tarik_engine_protocol::{
    ErrorEnvelope, ExportOptions, ExportPartSummary, ExportState, ExportStatus,
    ValidatedExportOptions, MAX_REPORTED_EXPORT_PARTS,
};

use crate::{
    error::EngineError,
    export::{self, ExportObserver},
};

const TERMINAL_EXPORT_LIMIT: usize = 32;

struct ExportRecord {
    session_id: String,
    sql: String,
    options: Option<ValidatedExportOptions>,
    connection: Option<Connection>,
    state: ExportState,
    queued_at: Instant,
    started_at: Option<Instant>,
    interrupt: Option<Arc<duckdb::InterruptHandle>>,
    cancel_requested: bool,
    rows_written: u64,
    files_written: u64,
    bytes_written: u64,
    current_part: Option<u64>,
    completed_parts: VecDeque<ExportPartSummary>,
    error: Option<ErrorEnvelope>,
}

impl ExportRecord {
    fn status(&self, export_id: &str) -> ExportStatus {
        ExportStatus {
            export_id: export_id.to_string(),
            state: self.state,
            duration_ms: self
                .started_at
                .unwrap_or(self.queued_at)
                .elapsed()
                .as_millis() as u64,
            rows_written: self.rows_written,
            files_written: self.files_written,
            bytes_written: self.bytes_written,
            current_part: self.current_part,
            completed_parts: self.completed_parts.iter().cloned().collect(),
            error: self.error.clone(),
        }
    }
}

#[derive(Default)]
struct SessionQueue {
    pending: VecDeque<String>,
    running: Option<String>,
    worker_alive: bool,
}

#[derive(Default)]
struct Registry {
    exports: HashMap<String, ExportRecord>,
    queues: HashMap<String, SessionQueue>,
    terminal_order: VecDeque<String>,
}

pub struct ExportRegistry {
    inner: Mutex<Registry>,
}

impl ExportRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Registry::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Registry>, EngineError> {
        self.inner.lock().map_err(|_| EngineError::RegistryPoisoned)
    }

    /// Validate before enqueueing. Validation neither executes SQL nor creates
    /// a file, so rejected options cannot leave side effects.
    pub fn execute(
        self: &Arc<Self>,
        session_id: &str,
        export_id: &str,
        sql: &str,
        connection: Connection,
        options: ExportOptions,
    ) -> Result<(), EngineError> {
        if sql.trim().is_empty() || crate::sql::split_statements(sql).is_empty() {
            return Err(EngineError::InvalidQuery("sql text contains no statements"));
        }
        let options = options
            .validate()
            .map_err(|error| EngineError::ExportInvalid(error.to_string()))?;
        let mut inner = self.lock()?;
        if inner.exports.contains_key(export_id) {
            return Err(EngineError::ExportExists(export_id.to_string()));
        }
        inner.exports.insert(
            export_id.to_string(),
            ExportRecord {
                session_id: session_id.to_string(),
                sql: sql.to_string(),
                options: Some(options),
                connection: Some(connection),
                state: ExportState::Queued,
                queued_at: Instant::now(),
                started_at: None,
                interrupt: None,
                cancel_requested: false,
                rows_written: 0,
                files_written: 0,
                bytes_written: 0,
                current_part: None,
                completed_parts: VecDeque::new(),
                error: None,
            },
        );
        let spawn_worker = {
            let queue = inner.queues.entry(session_id.to_string()).or_default();
            queue.pending.push_back(export_id.to_string());
            !queue.worker_alive
        };
        if spawn_worker {
            if let Some(queue) = inner.queues.get_mut(session_id) {
                queue.worker_alive = true;
            }
            let registry = Arc::clone(self);
            let session = session_id.to_string();
            if let Err(error) = std::thread::Builder::new()
                .name(format!("tarik-export-session-{session_id}"))
                .spawn(move || worker_loop(registry, &session))
            {
                if let Some(queue) = inner.queues.get_mut(session_id) {
                    queue.pending.retain(|id| id != export_id);
                    queue.worker_alive = false;
                }
                inner.exports.remove(export_id);
                return Err(EngineError::WorkerSpawn(error.to_string()));
            }
        }
        Ok(())
    }

    pub fn status(&self, export_id: &str) -> Result<ExportStatus, EngineError> {
        let inner = self.lock()?;
        inner
            .exports
            .get(export_id)
            .map(|record| record.status(export_id))
            .ok_or_else(|| EngineError::ExportMissing(export_id.to_string()))
    }

    pub fn cancel(&self, export_id: &str) -> Result<ExportStatus, EngineError> {
        let mut mark_queued_terminal = false;
        {
            let mut inner = self.lock()?;
            let queued_session = {
                let Some(record) = inner.exports.get_mut(export_id) else {
                    return Err(EngineError::ExportMissing(export_id.to_string()));
                };
                match record.state {
                    ExportState::Queued => {
                        record.cancel_requested = true;
                        record.state = ExportState::Cancelled;
                        record.connection.take();
                        record.options.take();
                        record.current_part = None;
                        mark_queued_terminal = true;
                        Some(record.session_id.clone())
                    }
                    ExportState::Running => {
                        record.cancel_requested = true;
                        if let Some(interrupt) = record.interrupt.as_ref() {
                            interrupt.interrupt();
                        }
                        None
                    }
                    ExportState::Succeeded | ExportState::Failed | ExportState::Cancelled => None,
                }
            };
            if let Some(session_id) = queued_session {
                if let Some(queue) = inner.queues.get_mut(&session_id) {
                    queue.pending.retain(|id| id != export_id);
                }
            }
        }
        if mark_queued_terminal {
            self.remember_terminal(export_id);
        }
        self.status(export_id)
    }

    pub fn cancel_session(&self, session_id: &str) {
        let ids = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .exports
                    .iter()
                    .filter(|(_, record)| record.session_id == session_id)
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in ids {
            let _ = self.cancel(&id);
        }
    }

    fn set_interrupt(&self, export_id: &str, interrupt: Arc<duckdb::InterruptHandle>) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(record) = inner.exports.get_mut(export_id) {
                record.interrupt = Some(interrupt);
                if record.cancel_requested {
                    if let Some(handle) = record.interrupt.as_ref() {
                        handle.interrupt();
                    }
                }
            }
        }
    }

    fn is_cancel_requested(&self, export_id: &str) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| {
                inner
                    .exports
                    .get(export_id)
                    .map(|record| record.cancel_requested)
            })
            .unwrap_or(false)
    }

    fn mark_terminal(&self, export_id: &str, state: ExportState, error: Option<ErrorEnvelope>) {
        if let Ok(mut inner) = self.inner.lock() {
            let Some(record) = inner.exports.get_mut(export_id) else {
                return;
            };
            if matches!(
                record.state,
                ExportState::Succeeded | ExportState::Failed | ExportState::Cancelled
            ) {
                return;
            }
            let session_id = record.session_id.clone();
            record.state = state;
            record.error = error;
            record.current_part = None;
            record.interrupt = None;
            if let Some(queue) = inner.queues.get_mut(&session_id) {
                if queue.running.as_deref() == Some(export_id) {
                    queue.running = None;
                }
            }
        }
        self.remember_terminal(export_id);
    }

    fn remember_terminal(&self, export_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            if !inner.terminal_order.iter().any(|id| id == export_id) {
                inner.terminal_order.push_back(export_id.to_string());
            }
            while inner.terminal_order.len() > TERMINAL_EXPORT_LIMIT {
                if let Some(oldest) = inner.terminal_order.pop_front() {
                    inner.exports.remove(&oldest);
                }
            }
        }
    }
}

struct RegistryObserver<'a> {
    registry: &'a ExportRegistry,
    export_id: &'a str,
}

impl ExportObserver for RegistryObserver<'_> {
    fn check_cancelled(&self, current_part: u64) -> Result<(), EngineError> {
        if self.registry.is_cancel_requested(self.export_id) {
            return Err(EngineError::ExportCancelled);
        }
        if let Ok(mut inner) = self.registry.inner.lock() {
            if let Some(record) = inner.exports.get_mut(self.export_id) {
                record.current_part = Some(current_part);
            }
        }
        Ok(())
    }

    fn rows_written(&self, rows: u64, current_part: u64) {
        if let Ok(mut inner) = self.registry.inner.lock() {
            if let Some(record) = inner.exports.get_mut(self.export_id) {
                record.rows_written = record.rows_written.saturating_add(rows);
                record.current_part = Some(current_part);
            }
        }
    }

    fn part_completed(&self, part: &ExportPartSummary) {
        if let Ok(mut inner) = self.registry.inner.lock() {
            if let Some(record) = inner.exports.get_mut(self.export_id) {
                record.files_written = record.files_written.saturating_add(1);
                record.bytes_written = record.bytes_written.saturating_add(part.bytes);
                record.completed_parts.push_back(part.clone());
                while record.completed_parts.len() > MAX_REPORTED_EXPORT_PARTS {
                    record.completed_parts.pop_front();
                }
            }
        }
    }
}

fn worker_loop(registry: Arc<ExportRegistry>, session_id: &str) {
    loop {
        let claimed = {
            let mut inner = match registry.lock() {
                Ok(inner) => inner,
                Err(_) => return,
            };
            let Some(queue) = inner.queues.get_mut(session_id) else {
                return;
            };
            let Some(export_id) = queue.pending.pop_front() else {
                queue.worker_alive = false;
                return;
            };
            queue.running = Some(export_id.clone());
            match inner.exports.get_mut(&export_id) {
                Some(record) if record.state == ExportState::Queued && !record.cancel_requested => {
                    record.state = ExportState::Running;
                    record.started_at = Some(Instant::now());
                    Some((
                        export_id,
                        record.connection.take(),
                        record.options.take(),
                        record.sql.clone(),
                    ))
                }
                _ => None,
            }
        };
        let Some((export_id, connection, options, sql)) = claimed else {
            continue;
        };
        run_claimed_export(&registry, &export_id, connection, options, &sql);
    }
}

fn run_claimed_export(
    registry: &ExportRegistry,
    export_id: &str,
    connection: Option<Connection>,
    options: Option<ValidatedExportOptions>,
    sql: &str,
) {
    let (Some(connection), Some(options)) = (connection, options) else {
        registry.mark_terminal(
            export_id,
            ExportState::Failed,
            Some(ErrorEnvelope::new(
                "export.rejected",
                "engine lost export work before it started",
            )),
        );
        return;
    };
    registry.set_interrupt(export_id, connection.interrupt_handle());
    let observer = RegistryObserver {
        registry,
        export_id,
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        export::execute_validated_export(&connection, sql, &options, &observer)
    }));
    match result {
        Ok(Ok(_)) => registry.mark_terminal(export_id, ExportState::Succeeded, None),
        Ok(Err(error)) => {
            if registry.is_cancel_requested(export_id)
                || matches!(error, EngineError::ExportCancelled)
            {
                registry.mark_terminal(export_id, ExportState::Cancelled, None);
            } else {
                registry.mark_terminal(
                    export_id,
                    ExportState::Failed,
                    Some(ErrorEnvelope::new(error.code(), error.to_string())),
                );
            }
        }
        Err(payload) => {
            if registry.is_cancel_requested(export_id) {
                registry.mark_terminal(export_id, ExportState::Cancelled, None);
            } else {
                registry.mark_terminal(
                    export_id,
                    ExportState::Failed,
                    Some(ErrorEnvelope::new(
                        "export.interrupted",
                        panic_message(payload),
                    )),
                );
            }
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|value| (*value).to_string())
        })
        .unwrap_or_else(|| "engine export failure".to_string())
}

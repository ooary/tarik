//! Asynchronous, cancellable source-table imports with bounded status retention.

use std::{
    collections::{HashMap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use duckdb::Connection;
use tarik_engine_protocol::{
    EffectiveEngineResources, ErrorEnvelope, ImportRequest, ImportStage, ImportState, ImportStatus,
    SourceRecord,
};

use crate::{error::EngineError, sources};

const TERMINAL_IMPORT_LIMIT: usize = 32;

struct ImportRecord {
    session_id: String,
    request: Option<ImportRequest>,
    connection: Option<Connection>,
    state: ImportState,
    stage: ImportStage,
    queued_at: Instant,
    started_at: Option<Instant>,
    interrupt: Option<Arc<duckdb::InterruptHandle>>,
    cancel_requested: bool,
    effective_resources: EffectiveEngineResources,
    source: Option<SourceRecord>,
    error: Option<ErrorEnvelope>,
}

impl ImportRecord {
    fn status(&self, import_id: &str) -> ImportStatus {
        ImportStatus {
            import_id: import_id.to_string(),
            state: self.state,
            stage: self.stage,
            duration_ms: self
                .started_at
                .unwrap_or(self.queued_at)
                .elapsed()
                .as_millis() as u64,
            effective_resources: self.effective_resources.clone(),
            source: self.source.clone(),
            error: self.error.clone(),
        }
    }
}

#[derive(Default)]
struct Registry {
    imports: HashMap<String, ImportRecord>,
    terminal_order: VecDeque<String>,
}

pub struct ImportRegistry {
    inner: Mutex<Registry>,
}

impl ImportRegistry {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Registry::default()),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Registry>, EngineError> {
        self.inner.lock().map_err(|_| EngineError::RegistryPoisoned)
    }

    pub fn execute(
        self: &Arc<Self>,
        session_id: &str,
        import_id: &str,
        request: ImportRequest,
        connection: Connection,
        effective_resources: EffectiveEngineResources,
    ) -> Result<(), EngineError> {
        if request.project_id.trim().is_empty() || request.path.trim().is_empty() {
            return Err(EngineError::InvalidOptions(
                "import requires a project and source path",
            ));
        }
        let mut inner = self.lock()?;
        if inner.imports.contains_key(import_id) {
            return Err(EngineError::ImportExists(import_id.to_string()));
        }
        if inner.imports.values().any(|record| {
            record.session_id == session_id
                && matches!(record.state, ImportState::Queued | ImportState::Running)
        }) {
            return Err(EngineError::ImportBusy);
        }
        inner.imports.insert(
            import_id.to_string(),
            ImportRecord {
                session_id: session_id.to_string(),
                request: Some(request),
                connection: Some(connection),
                state: ImportState::Queued,
                stage: ImportStage::Queued,
                queued_at: Instant::now(),
                started_at: None,
                interrupt: None,
                cancel_requested: false,
                effective_resources,
                source: None,
                error: None,
            },
        );
        let registry = Arc::clone(self);
        let id = import_id.to_string();
        if let Err(error) = std::thread::Builder::new()
            .name(format!("tarik-import-{import_id}"))
            .spawn(move || run_import(registry, &id))
        {
            inner.imports.remove(import_id);
            return Err(EngineError::WorkerSpawn(error.to_string()));
        }
        Ok(())
    }

    pub fn status(&self, import_id: &str) -> Result<ImportStatus, EngineError> {
        self.lock()?
            .imports
            .get(import_id)
            .map(|record| record.status(import_id))
            .ok_or_else(|| EngineError::ImportMissing(import_id.to_string()))
    }

    pub fn cancel(&self, import_id: &str) -> Result<ImportStatus, EngineError> {
        let queued = {
            let mut inner = self.lock()?;
            let record = inner
                .imports
                .get_mut(import_id)
                .ok_or_else(|| EngineError::ImportMissing(import_id.to_string()))?;
            match record.state {
                ImportState::Queued => {
                    record.cancel_requested = true;
                    record.state = ImportState::Cancelled;
                    record.request.take();
                    record.connection.take();
                    true
                }
                ImportState::Running => {
                    record.cancel_requested = true;
                    if let Some(interrupt) = record.interrupt.as_ref() {
                        interrupt.interrupt();
                    }
                    false
                }
                ImportState::Succeeded
                | ImportState::Failed
                | ImportState::Cancelled
                | ImportState::RecoveryRequired => false,
            }
        };
        if queued {
            self.remember_terminal(import_id);
        }
        self.status(import_id)
    }

    pub fn has_active_session(&self, session_id: &str) -> Result<bool, EngineError> {
        Ok(self.lock()?.imports.values().any(|record| {
            record.session_id == session_id
                && matches!(record.state, ImportState::Queued | ImportState::Running)
        }))
    }

    pub fn cancel_session(&self, session_id: &str) {
        let ids = self
            .inner
            .lock()
            .map(|inner| {
                inner
                    .imports
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

    fn set_stage(&self, import_id: &str, stage: ImportStage) {
        if let Ok(mut inner) = self.inner.lock() {
            if let Some(record) = inner.imports.get_mut(import_id) {
                if matches!(record.state, ImportState::Queued | ImportState::Running) {
                    record.stage = stage;
                }
            }
        }
    }

    fn is_cancel_requested(&self, import_id: &str) -> bool {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| {
                inner
                    .imports
                    .get(import_id)
                    .map(|record| record.cancel_requested)
            })
            .unwrap_or(false)
    }

    fn mark_terminal(
        &self,
        import_id: &str,
        state: ImportState,
        source: Option<SourceRecord>,
        error: Option<ErrorEnvelope>,
    ) {
        if let Ok(mut inner) = self.inner.lock() {
            let Some(record) = inner.imports.get_mut(import_id) else {
                return;
            };
            if matches!(
                record.state,
                ImportState::Succeeded
                    | ImportState::Failed
                    | ImportState::Cancelled
                    | ImportState::RecoveryRequired
            ) {
                return;
            }
            record.state = state;
            record.source = source;
            record.error = error;
            record.interrupt = None;
            record.connection = None;
            record.request = None;
        }
        self.remember_terminal(import_id);
    }

    fn remember_terminal(&self, import_id: &str) {
        if let Ok(mut inner) = self.inner.lock() {
            if !inner.terminal_order.iter().any(|id| id == import_id) {
                inner.terminal_order.push_back(import_id.to_string());
            }
            while inner.terminal_order.len() > TERMINAL_IMPORT_LIMIT {
                if let Some(oldest) = inner.terminal_order.pop_front() {
                    inner.imports.remove(&oldest);
                }
            }
        }
    }
}

fn run_import(registry: Arc<ImportRegistry>, import_id: &str) {
    let claimed = {
        let mut inner = match registry.lock() {
            Ok(inner) => inner,
            Err(_) => return,
        };
        let Some(record) = inner.imports.get_mut(import_id) else {
            return;
        };
        if record.state != ImportState::Queued || record.cancel_requested {
            return;
        }
        record.state = ImportState::Running;
        record.stage = ImportStage::Validating;
        record.started_at = Some(Instant::now());
        let connection = record.connection.take();
        let request = record.request.take();
        if let Some(connection) = connection.as_ref() {
            record.interrupt = Some(connection.interrupt_handle());
        }
        (connection, request)
    };
    let (Some(connection), Some(request)) = claimed else {
        registry.mark_terminal(
            import_id,
            ImportState::Failed,
            None,
            Some(ErrorEnvelope::new(
                "import.rejected",
                "engine lost the import request",
            )),
        );
        return;
    };

    registry.set_stage(import_id, ImportStage::ReadingAndWriting);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        sources::import_table_checked(
            &connection,
            &request.project_id,
            Path::new(&request.path),
            request.expected_file_size_bytes,
            &request.options,
            || registry.set_stage(import_id, ImportStage::Finalizing),
        )
    }));
    match outcome {
        Ok(Ok(source)) => {
            registry.mark_terminal(import_id, ImportState::Succeeded, Some(source), None)
        }
        Ok(Err(error)) => {
            let recovery = matches!(error, EngineError::ImportRecoveryRequired(_));
            let cancelled = registry.is_cancel_requested(import_id) && !recovery;
            registry.mark_terminal(
                import_id,
                if recovery {
                    ImportState::RecoveryRequired
                } else if cancelled {
                    ImportState::Cancelled
                } else {
                    ImportState::Failed
                },
                None,
                (!cancelled).then(|| ErrorEnvelope::new(error.code(), error.to_string())),
            );
        }
        Err(_) => registry.mark_terminal(
            import_id,
            if registry.is_cancel_requested(import_id) {
                ImportState::Cancelled
            } else {
                ImportState::Failed
            },
            None,
            (!registry.is_cancel_requested(import_id)).then(|| {
                ErrorEnvelope::new("import.panicked", "import worker stopped unexpectedly")
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarik_engine_protocol::{EngineResourcePreset, EngineResourceSettings, ImportOptions};

    fn resources() -> EffectiveEngineResources {
        EffectiveEngineResources {
            preset: EngineResourcePreset::Fast,
            memory_limit_mib: 8192,
            memory_limit_display: "8.0 GiB".into(),
            threads: 4,
        }
    }

    #[test]
    fn rejects_more_than_one_active_import_per_session() {
        let registry = Arc::new(ImportRegistry::new());
        let connection = Connection::open_in_memory().unwrap();
        let request = ImportRequest {
            project_id: "p1".into(),
            path: "missing.parquet".into(),
            expected_file_size_bytes: 1,
            options: ImportOptions {
                table_name: "data".into(),
                csv: None,
                column_overrides: vec![],
            },
        };
        // Keep the first record deterministically queued while the registry lock
        // is held, then prove the admission check fails closed.
        let mut inner = registry.lock().unwrap();
        inner.imports.insert(
            "first".into(),
            ImportRecord {
                session_id: "s1".into(),
                request: None,
                connection: None,
                state: ImportState::Running,
                stage: ImportStage::ReadingAndWriting,
                queued_at: Instant::now(),
                started_at: Some(Instant::now()),
                interrupt: None,
                cancel_requested: false,
                effective_resources: resources(),
                source: None,
                error: None,
            },
        );
        drop(inner);
        let error = registry
            .execute("s1", "second", request, connection, resources())
            .unwrap_err();
        assert!(matches!(error, EngineError::ImportBusy));
    }

    #[test]
    fn effective_resource_shape_remains_valid() {
        EngineResourceSettings::preset(EngineResourcePreset::Fast)
            .validate()
            .unwrap();
        assert_eq!(resources().threads, 4);
    }
}

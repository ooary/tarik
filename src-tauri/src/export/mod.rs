//! Desktop export coordinator.
//!
//! The coordinator validates project ownership, submits immutable SQL/options,
//! polls bounded sidecar status, and persists one terminal export record.

pub mod commands;

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use serde::Serialize;
use tarik_engine_protocol::{ExportOptions, ExportPartSummary, ExportState, ExportStatus};

use crate::{
    metadata::{
        projects::ProjectsRepository,
        sources::{
            ExportHistoryRecord, ExportPartSummary as HistoryPart, ExportStatus as HistoryStatus,
            SourcesRepository,
        },
        MetadataDb,
    },
    storage::CleanupService,
};

const POLL_INTERVAL: Duration = Duration::from_millis(150);
const MAX_POLL_FAILURES: u32 = 3;
const MAX_ACTIVE_EXPORTS: usize = 8;
const MAX_TERMINAL_EXPORTS: usize = 256;

pub trait EngineExporter: Send + Sync + 'static {
    fn execute(&self, export_id: &str, sql: &str, options: &ExportOptions) -> Result<(), String>;
    fn execute_bounded(
        &self,
        export_id: &str,
        sql: &str,
        options: &ExportOptions,
        maximum_total_bytes: u64,
    ) -> Result<(), String> {
        let _ = maximum_total_bytes;
        self.execute(export_id, sql, options)
    }
    fn status(&self, export_id: &str) -> Result<Option<ExportStatus>, String>;
    fn cancel(&self, export_id: &str) -> Result<Option<ExportStatus>, String>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportErrorView {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportView {
    pub export_id: String,
    pub project_id: String,
    pub state: ExportState,
    pub duration_ms: u64,
    pub rows_written: u64,
    pub files_written: u64,
    pub bytes_written: u64,
    pub current_part: Option<u64>,
    pub completed_parts: Vec<ExportPartSummary>,
    pub error: Option<ExportErrorView>,
}

struct ExportRecord {
    project_id: String,
    sql: String,
    options: ExportOptions,
    state: ExportState,
    duration_ms: u64,
    rows_written: u64,
    files_written: u64,
    bytes_written: u64,
    current_part: Option<u64>,
    completed_parts: Vec<ExportPartSummary>,
    error: Option<ExportErrorView>,
    history_written: bool,
}

impl ExportRecord {
    fn view(&self, export_id: &str) -> ExportView {
        ExportView {
            export_id: export_id.to_string(),
            project_id: self.project_id.clone(),
            state: self.state,
            duration_ms: self.duration_ms,
            rows_written: self.rows_written,
            files_written: self.files_written,
            bytes_written: self.bytes_written,
            current_part: self.current_part,
            completed_parts: self.completed_parts.clone(),
            error: self.error.clone(),
        }
    }
}

pub struct ExportCoordinator {
    engine: Arc<dyn EngineExporter>,
    database: MetadataDb,
    exports: Mutex<HashMap<String, ExportRecord>>,
    terminal_order: Mutex<VecDeque<String>>,
    poll_interval: Duration,
    cleanup: Option<Arc<CleanupService>>,
}

impl ExportCoordinator {
    pub fn new(engine: Arc<dyn EngineExporter>, database: MetadataDb) -> Self {
        Self {
            engine,
            database,
            exports: Mutex::new(HashMap::new()),
            terminal_order: Mutex::new(VecDeque::new()),
            poll_interval: POLL_INTERVAL,
            cleanup: None,
        }
    }

    pub fn with_cleanup(mut self, cleanup: Arc<CleanupService>) -> Self {
        self.cleanup = Some(cleanup);
        self
    }

    #[cfg(test)]
    fn with_poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }

    pub fn execute(
        self: &Arc<Self>,
        project_id: &str,
        sql: &str,
        options: ExportOptions,
    ) -> Result<ExportView, String> {
        self.execute_with_budget(project_id, sql, options, None)
    }

    pub(crate) fn execute_bounded(
        self: &Arc<Self>,
        project_id: &str,
        sql: &str,
        options: ExportOptions,
        maximum_total_bytes: u64,
    ) -> Result<ExportView, String> {
        if maximum_total_bytes == 0 {
            return Err("export.invalid_quota: maximum total bytes must be positive".into());
        }
        self.execute_with_budget(project_id, sql, options, Some(maximum_total_bytes))
    }

    fn execute_with_budget(
        self: &Arc<Self>,
        project_id: &str,
        sql: &str,
        options: ExportOptions,
        maximum_total_bytes: Option<u64>,
    ) -> Result<ExportView, String> {
        if sql.trim().is_empty() {
            return Err("export.empty_sql".into());
        }
        if ProjectsRepository::new(self.database.clone())
            .find(project_id)
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Err(format!("export.project_missing: {project_id}"));
        }
        let validated = options
            .validate()
            .map_err(|error| format!("export.invalid_options.{}: {error}", error.field()))?;
        let options = validated.to_options();
        let export_id = uuid::Uuid::new_v4().to_string();
        let mut exports = self
            .exports
            .lock()
            .map_err(|_| "export registry poisoned".to_string())?;
        if exports
            .values()
            .filter(|record| matches!(record.state, ExportState::Queued | ExportState::Running))
            .count()
            >= MAX_ACTIVE_EXPORTS
        {
            return Err("export.busy: too many exports are active".into());
        }
        if let Some(cleanup) = self.cleanup.as_ref() {
            cleanup.register_export(&export_id, &options)?;
        }
        exports.insert(
            export_id.clone(),
            ExportRecord {
                project_id: project_id.to_string(),
                sql: sql.to_string(),
                options: options.clone(),
                state: ExportState::Queued,
                duration_ms: 0,
                rows_written: 0,
                files_written: 0,
                bytes_written: 0,
                current_part: None,
                completed_parts: Vec::new(),
                error: None,
                history_written: false,
            },
        );
        drop(exports);

        let submission = match maximum_total_bytes {
            Some(maximum) => self
                .engine
                .execute_bounded(&export_id, sql, &options, maximum),
            None => self.engine.execute(&export_id, sql, &options),
        };
        let submitted = match submission {
            Ok(()) => true,
            Err(message) => {
                self.mark_terminal(
                    &export_id,
                    ExportState::Failed,
                    Some(ExportErrorView {
                        code: "export.rejected".into(),
                        message,
                    }),
                );
                false
            }
        };
        if submitted {
            self.spawn_poller(export_id.clone());
        }
        self.status(&export_id)
            .ok_or_else(|| "export registry poisoned".to_string())
    }

    pub fn cancel_all(self: &Arc<Self>) -> u64 {
        let ids = self
            .exports
            .lock()
            .map(|exports| {
                exports
                    .iter()
                    .filter(|(_, record)| {
                        matches!(record.state, ExportState::Queued | ExportState::Running)
                    })
                    .map(|(id, _)| id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for id in &ids {
            let _ = self.cancel(id);
        }
        ids.len() as u64
    }

    pub fn has_active(&self) -> bool {
        self.exports
            .lock()
            .map(|exports| {
                exports.values().any(|record| {
                    matches!(record.state, ExportState::Queued | ExportState::Running)
                })
            })
            .unwrap_or(false)
    }

    pub fn status(&self, export_id: &str) -> Option<ExportView> {
        self.exports
            .lock()
            .ok()?
            .get(export_id)
            .map(|record| record.view(export_id))
    }

    pub fn release(&self, export_id: &str) -> Result<bool, String> {
        let removed = {
            let mut exports = self
                .exports
                .lock()
                .map_err(|_| "export registry poisoned".to_string())?;
            let Some(record) = exports.get(export_id) else {
                return Ok(false);
            };
            if matches!(record.state, ExportState::Queued | ExportState::Running) {
                return Err("export.active: cancel the export before releasing it".into());
            }
            exports.remove(export_id).is_some()
        };
        if removed {
            if let Ok(mut order) = self.terminal_order.lock() {
                order.retain(|id| id != export_id);
            }
        }
        Ok(removed)
    }

    pub fn completed_part_path(
        &self,
        export_id: &str,
        part_number: u64,
    ) -> Option<std::path::PathBuf> {
        let exports = self.exports.lock().ok()?;
        let record = exports.get(export_id)?;
        let reported = record
            .completed_parts
            .iter()
            .find(|part| part.part_number == part_number)?;
        let expected = record
            .options
            .clone()
            .validate()
            .ok()?
            .part_path(part_number)
            .ok()?;
        (std::path::Path::new(&reported.path) == expected && expected.is_file()).then_some(expected)
    }

    pub fn cancel(self: &Arc<Self>, export_id: &str) -> Result<ExportView, String> {
        if let Some(status) = self.engine.cancel(export_id)? {
            self.apply_status(export_id, &status);
            if matches!(
                status.state,
                ExportState::Succeeded | ExportState::Failed | ExportState::Cancelled
            ) {
                let error = status.error.map(|error| ExportErrorView {
                    code: error.code,
                    message: error.message,
                });
                self.mark_terminal(export_id, status.state, error);
            }
        }
        self.status(export_id)
            .ok_or_else(|| "export does not exist".to_string())
    }

    fn spawn_poller(self: &Arc<Self>, export_id: String) {
        let coordinator = Arc::clone(self);
        if thread::Builder::new()
            .name(format!("tarik-export-poll-{export_id}"))
            .spawn({
                let id = export_id.clone();
                move || coordinator.poll_until_terminal(&id)
            })
            .is_err()
        {
            self.mark_terminal(
                &export_id,
                ExportState::Failed,
                Some(ExportErrorView {
                    code: "export.poller".into(),
                    message: "could not start export status poller".into(),
                }),
            );
        }
    }

    fn poll_until_terminal(self: &Arc<Self>, export_id: &str) {
        let mut failures = 0u32;
        loop {
            thread::sleep(self.poll_interval);
            match self.engine.status(export_id) {
                Ok(Some(status)) => {
                    failures = 0;
                    self.apply_status(export_id, &status);
                    match status.state {
                        ExportState::Queued | ExportState::Running => {}
                        ExportState::Succeeded => {
                            self.mark_terminal(export_id, ExportState::Succeeded, None);
                            break;
                        }
                        ExportState::Failed | ExportState::Cancelled => {
                            let error = status.error.map(|error| ExportErrorView {
                                code: error.code,
                                message: error.message,
                            });
                            self.mark_terminal(export_id, status.state, error);
                            break;
                        }
                    }
                }
                Ok(None) => {
                    self.mark_terminal(
                        export_id,
                        ExportState::Failed,
                        Some(ExportErrorView {
                            code: "export.lost".into(),
                            message: "the engine no longer knows this export".into(),
                        }),
                    );
                    break;
                }
                Err(_) => {
                    failures += 1;
                    if failures >= MAX_POLL_FAILURES {
                        self.mark_terminal(
                            export_id,
                            ExportState::Failed,
                            Some(ExportErrorView {
                                code: "engine.unreachable".into(),
                                message: "the engine stopped answering export status polls".into(),
                            }),
                        );
                        break;
                    }
                }
            }
        }
    }

    fn apply_status(&self, export_id: &str, status: &ExportStatus) {
        if let Ok(mut exports) = self.exports.lock() {
            if let Some(record) = exports.get_mut(export_id) {
                if record.history_written {
                    return;
                }
                record.state = status.state;
                record.duration_ms = status.duration_ms;
                record.rows_written = status.rows_written;
                record.files_written = status.files_written;
                record.bytes_written = status.bytes_written;
                record.current_part = status.current_part;
                record.completed_parts = status.completed_parts.clone();
            }
        }
    }

    fn mark_terminal(&self, export_id: &str, state: ExportState, error: Option<ExportErrorView>) {
        let history = {
            let mut exports = match self.exports.lock() {
                Ok(exports) => exports,
                Err(_) => return,
            };
            let Some(record) = exports.get_mut(export_id) else {
                return;
            };
            if record.history_written {
                return;
            }
            let status = match state {
                ExportState::Succeeded => HistoryStatus::Succeeded,
                ExportState::Failed => HistoryStatus::Failed,
                ExportState::Cancelled => HistoryStatus::Cancelled,
                ExportState::Queued | ExportState::Running => return,
            };
            record.state = state;
            record.current_part = None;
            record.error = error;
            record.history_written = true;
            let now = chrono::Utc::now().to_rfc3339();
            ExportHistoryRecord {
                id: export_id.to_string(),
                project_id: record.project_id.clone(),
                status,
                format: format_name(&record.options).into(),
                output_directory: record.options.output_directory.clone(),
                base_name: record.options.base_name.clone(),
                rows_per_part: record.options.rows_per_part,
                sql_text: record.sql.clone(),
                options: serde_json::to_value(&record.options).unwrap_or_default(),
                duration_ms: Some(record.duration_ms.max(1)),
                rows_written: record.rows_written,
                files_written: record.files_written,
                bytes_written: record.bytes_written,
                completed_parts: record.completed_parts.iter().map(history_part).collect(),
                error_code: record.error.as_ref().map(|error| error.code.clone()),
                error_message: record.error.as_ref().map(|error| error.message.clone()),
                created_at: now.clone(),
                updated_at: now,
            }
        };
        if let Err(error) =
            SourcesRepository::new(self.database.clone()).add_terminal_export(&history)
        {
            eprintln!("tarik: could not persist export history: {error}");
        }
        let recovery_required = history.error_code.as_deref() == Some("export.recovery_required");
        if !recovery_required {
            if let Some(cleanup) = self.cleanup.as_ref() {
                cleanup.complete_export(export_id);
            }
        }
        self.remember_terminal(export_id);
    }

    fn remember_terminal(&self, export_id: &str) {
        let evicted = {
            let Ok(mut order) = self.terminal_order.lock() else {
                return;
            };
            order.retain(|id| id != export_id);
            order.push_back(export_id.to_string());
            let mut evicted = Vec::new();
            while order.len() > MAX_TERMINAL_EXPORTS {
                if let Some(id) = order.pop_front() {
                    evicted.push(id);
                }
            }
            evicted
        };
        if !evicted.is_empty() {
            if let Ok(mut exports) = self.exports.lock() {
                for id in evicted {
                    exports.remove(&id);
                }
            }
        }
    }
}

fn format_name(options: &ExportOptions) -> &'static str {
    match options.format {
        tarik_engine_protocol::ExportFormat::Csv => "csv",
        tarik_engine_protocol::ExportFormat::Parquet => "parquet",
    }
}

fn history_part(part: &ExportPartSummary) -> HistoryPart {
    HistoryPart {
        part_number: part.part_number,
        path: part.path.clone(),
        rows: part.rows,
        bytes: part.bytes,
    }
}

impl EngineExporter for crate::engine_manager::EngineManager {
    fn execute(&self, export_id: &str, sql: &str, options: &ExportOptions) -> Result<(), String> {
        self.execute_export(export_id, sql, options)
    }

    fn execute_bounded(
        &self,
        export_id: &str,
        sql: &str,
        options: &ExportOptions,
        maximum_total_bytes: u64,
    ) -> Result<(), String> {
        self.execute_export_bounded(export_id, sql, options, Some(maximum_total_bytes))
    }

    fn status(&self, export_id: &str) -> Result<Option<ExportStatus>, String> {
        self.export_status(export_id)
    }

    fn cancel(&self, export_id: &str) -> Result<Option<ExportStatus>, String> {
        self.cancel_export(export_id)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        path::{Path, PathBuf},
        sync::Mutex as StdMutex,
    };

    use tarik_engine_protocol::{
        CsvExportOptions, ErrorEnvelope, ExportFormat, ExportOverwritePolicy,
    };

    use super::*;

    struct FakeEngine {
        execute_error: Option<String>,
        statuses: StdMutex<VecDeque<ExportStatus>>,
        last: StdMutex<Option<ExportStatus>>,
        submitted: StdMutex<Vec<(String, String, ExportOptions)>>,
        cancel_called: StdMutex<bool>,
    }

    impl FakeEngine {
        fn new(statuses: Vec<ExportStatus>) -> Self {
            Self {
                execute_error: None,
                statuses: StdMutex::new(statuses.into()),
                last: StdMutex::new(None),
                submitted: StdMutex::new(Vec::new()),
                cancel_called: StdMutex::new(false),
            }
        }

        fn rejected(message: &str) -> Self {
            Self {
                execute_error: Some(message.into()),
                ..Self::new(Vec::new())
            }
        }
    }

    impl EngineExporter for FakeEngine {
        fn execute(
            &self,
            export_id: &str,
            sql: &str,
            options: &ExportOptions,
        ) -> Result<(), String> {
            self.submitted.lock().unwrap().push((
                export_id.to_string(),
                sql.to_string(),
                options.clone(),
            ));
            match &self.execute_error {
                Some(error) => Err(error.clone()),
                None => Ok(()),
            }
        }

        fn status(&self, _export_id: &str) -> Result<Option<ExportStatus>, String> {
            let status = if let Some(next) = self.statuses.lock().unwrap().pop_front() {
                *self.last.lock().unwrap() = Some(next.clone());
                next
            } else {
                let Some(last) = self.last.lock().unwrap().clone() else {
                    return Ok(None);
                };
                last
            };
            Ok(Some(status))
        }

        fn cancel(&self, _export_id: &str) -> Result<Option<ExportStatus>, String> {
            *self.cancel_called.lock().unwrap() = true;
            let status = self
                .last
                .lock()
                .unwrap()
                .clone()
                .or_else(|| self.statuses.lock().unwrap().front().cloned());
            Ok(status)
        }
    }

    fn output_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tarik-desktop-export-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&path).unwrap();
        path
    }

    fn options(directory: &Path) -> ExportOptions {
        ExportOptions {
            format: ExportFormat::Csv,
            output_directory: directory.to_string_lossy().into_owned(),
            base_name: "orders".into(),
            rows_per_part: 3,
            overwrite: ExportOverwritePolicy::FailIfExists,
            csv: Some(CsvExportOptions::default()),
            parquet: None,
        }
    }

    fn status(state: ExportState) -> ExportStatus {
        ExportStatus {
            export_id: "engine-id-is-advisory".into(),
            state,
            duration_ms: 25,
            rows_written: if state == ExportState::Queued { 0 } else { 5 },
            files_written: if state == ExportState::Succeeded {
                2
            } else {
                1
            },
            bytes_written: if state == ExportState::Queued { 0 } else { 240 },
            current_part: (state == ExportState::Running).then_some(2),
            completed_parts: if state == ExportState::Queued {
                Vec::new()
            } else {
                vec![ExportPartSummary {
                    part_number: 1,
                    path: "/exports/orders-part-00001.csv".into(),
                    rows: 3,
                    bytes: 120,
                }]
            },
            error: (state == ExportState::Failed)
                .then(|| ErrorEnvelope::new("export.io", "disk full")),
        }
    }

    fn coordinator(engine: Arc<FakeEngine>) -> (Arc<ExportCoordinator>, String, MetadataDb) {
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                Path::new("/tmp/export-test.duckdb"),
                crate::metadata::projects::ProjectOwnership::External,
            )
            .unwrap();
        let coordinator = Arc::new(
            ExportCoordinator::new(engine, database.clone())
                .with_poll_interval(Duration::from_millis(5)),
        );
        (coordinator, project.id, database)
    }

    fn wait_terminal(coordinator: &ExportCoordinator, export_id: &str) -> ExportView {
        for _ in 0..400 {
            if let Some(view) = coordinator.status(export_id) {
                if matches!(
                    view.state,
                    ExportState::Succeeded | ExportState::Failed | ExportState::Cancelled
                ) {
                    return view;
                }
            }
            thread::sleep(Duration::from_millis(5));
        }
        panic!("export did not terminate");
    }

    #[test]
    fn success_persists_sql_options_progress_and_parts_once() {
        let engine = Arc::new(FakeEngine::new(vec![
            status(ExportState::Queued),
            status(ExportState::Running),
            status(ExportState::Succeeded),
        ]));
        let (coordinator, project_id, database) = coordinator(engine);
        let directory = output_dir("success");
        let queued = coordinator
            .execute(&project_id, "SELECT * FROM orders", options(&directory))
            .unwrap();
        let view = wait_terminal(&coordinator, &queued.export_id);
        assert_eq!(view.state, ExportState::Succeeded);
        assert_eq!(view.rows_written, 5);
        assert_eq!(view.files_written, 2);

        let history = SourcesRepository::new(database)
            .get_export(&queued.export_id)
            .unwrap()
            .unwrap();
        assert_eq!(history.status, HistoryStatus::Succeeded);
        assert_eq!(history.sql_text, "SELECT * FROM orders");
        assert_eq!(history.rows_written, 5);
        assert_eq!(history.files_written, 2);
        assert_eq!(history.completed_parts[0].part_number, 1);
        assert_eq!(history.options["csv"]["includeHeader"], true);

        coordinator.mark_terminal(
            &queued.export_id,
            ExportState::Failed,
            Some(ExportErrorView {
                code: "late".into(),
                message: "must not overwrite".into(),
            }),
        );
        let history = SourcesRepository::new(coordinator.database.clone())
            .get_export(&queued.export_id)
            .unwrap()
            .unwrap();
        assert_eq!(history.status, HistoryStatus::Succeeded);
        assert_eq!(history.error_code, None);
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn failure_and_cancel_preserve_partial_part_summaries() {
        for terminal in [ExportState::Failed, ExportState::Cancelled] {
            let engine = Arc::new(FakeEngine::new(vec![
                status(ExportState::Running),
                status(terminal),
            ]));
            let (coordinator, project_id, database) = coordinator(engine.clone());
            let directory = output_dir("terminal");
            let queued = coordinator
                .execute(
                    &project_id,
                    "SELECT * FROM large_table",
                    options(&directory),
                )
                .unwrap();
            if terminal == ExportState::Cancelled {
                coordinator.cancel(&queued.export_id).unwrap();
                assert!(*engine.cancel_called.lock().unwrap());
            }
            let view = wait_terminal(&coordinator, &queued.export_id);
            assert_eq!(view.state, terminal);
            assert_eq!(view.completed_parts.len(), 1);
            let history = SourcesRepository::new(database)
                .get_export(&queued.export_id)
                .unwrap()
                .unwrap();
            assert_eq!(history.completed_parts.len(), 1);
            if terminal == ExportState::Failed {
                assert_eq!(history.error_code.as_deref(), Some("export.io"));
            } else {
                assert_eq!(history.error_code, None);
            }
            std::fs::remove_dir(directory).unwrap();
        }
    }

    #[test]
    fn validation_and_project_checks_prevent_submission() {
        let engine = Arc::new(FakeEngine::new(Vec::new()));
        let (coordinator, project_id, _database) = coordinator(engine.clone());
        let directory = output_dir("validation");
        let mut invalid = options(&directory);
        invalid.base_name = "../unsafe".into();
        let error = coordinator
            .execute(&project_id, "CREATE TABLE must_not_run(i INT)", invalid)
            .unwrap_err();
        assert!(error.starts_with("export.invalid_options.baseName:"));
        assert!(engine.submitted.lock().unwrap().is_empty());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 0);

        let error = coordinator
            .execute("missing", "SELECT 1", options(&directory))
            .unwrap_err();
        assert_eq!(error, "export.project_missing: missing");
        assert!(engine.submitted.lock().unwrap().is_empty());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn queued_cancel_applies_terminal_status_and_history_immediately() {
        let engine = Arc::new(FakeEngine::new(Vec::new()));
        *engine.last.lock().unwrap() = Some(status(ExportState::Cancelled));
        let (coordinator, project_id, database) = coordinator(engine.clone());
        let directory = output_dir("queued-cancel");
        let queued = coordinator
            .execute(&project_id, "SELECT 1", options(&directory))
            .unwrap();
        let cancelled = coordinator.cancel(&queued.export_id).unwrap();
        assert_eq!(cancelled.state, ExportState::Cancelled);
        assert!(*engine.cancel_called.lock().unwrap());
        let history = SourcesRepository::new(database)
            .get_export(&queued.export_id)
            .unwrap()
            .unwrap();
        assert_eq!(history.status, HistoryStatus::Cancelled);
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn reveal_path_is_derived_from_tracked_validated_options() {
        let engine = Arc::new(FakeEngine::new(vec![status(ExportState::Succeeded)]));
        let (coordinator, project_id, _database) = coordinator(engine);
        let directory = output_dir("reveal");
        let queued = coordinator
            .execute(&project_id, "SELECT 1", options(&directory))
            .unwrap();
        wait_terminal(&coordinator, &queued.export_id);
        let canonical = directory.join("orders-part-00001.csv");
        coordinator
            .exports
            .lock()
            .unwrap()
            .get_mut(&queued.export_id)
            .unwrap()
            .completed_parts[0]
            .path = canonical.to_string_lossy().into_owned();
        std::fs::write(&canonical, "i\n1\n").unwrap();

        assert_eq!(
            coordinator.completed_part_path(&queued.export_id, 1),
            Some(canonical.clone())
        );
        assert_eq!(coordinator.completed_part_path(&queued.export_id, 2), None);
        assert_eq!(coordinator.completed_part_path("forged", 1), None);
        std::fs::remove_file(canonical).unwrap();
        assert_eq!(coordinator.completed_part_path(&queued.export_id, 1), None);
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn bounded_submission_forwards_quota_and_terminal_release_preserves_history() {
        struct BudgetEngine {
            maximum: StdMutex<Option<u64>>,
        }
        impl EngineExporter for BudgetEngine {
            fn execute(
                &self,
                _export_id: &str,
                _sql: &str,
                _options: &ExportOptions,
            ) -> Result<(), String> {
                panic!("bounded lane must not use unbounded execute")
            }
            fn execute_bounded(
                &self,
                _export_id: &str,
                _sql: &str,
                _options: &ExportOptions,
                maximum_total_bytes: u64,
            ) -> Result<(), String> {
                *self.maximum.lock().unwrap() = Some(maximum_total_bytes);
                Ok(())
            }
            fn status(&self, export_id: &str) -> Result<Option<ExportStatus>, String> {
                let mut status = status(ExportState::Succeeded);
                status.export_id = export_id.to_string();
                Ok(Some(status))
            }
            fn cancel(&self, _export_id: &str) -> Result<Option<ExportStatus>, String> {
                Ok(None)
            }
        }
        let engine = Arc::new(BudgetEngine {
            maximum: StdMutex::new(None),
        });
        let database = MetadataDb::open_in_memory().unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert(
                "Test",
                Path::new("/tmp/export-budget.duckdb"),
                crate::metadata::projects::ProjectOwnership::External,
            )
            .unwrap();
        let coordinator = Arc::new(
            ExportCoordinator::new(engine.clone(), database.clone())
                .with_poll_interval(Duration::from_millis(5)),
        );
        let directory = output_dir("bounded");
        let queued = coordinator
            .execute_bounded(&project.id, "SELECT 1", options(&directory), 42_000)
            .unwrap();
        assert_eq!(*engine.maximum.lock().unwrap(), Some(42_000));
        wait_terminal(&coordinator, &queued.export_id);
        assert!(coordinator.release(&queued.export_id).unwrap());
        assert!(coordinator.status(&queued.export_id).is_none());
        assert!(SourcesRepository::new(database)
            .get_export(&queued.export_id)
            .unwrap()
            .is_some());
        assert!(!coordinator.release(&queued.export_id).unwrap());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn rejected_submission_records_one_terminal_failure() {
        let engine = Arc::new(FakeEngine::rejected("sidecar unavailable"));
        let (coordinator, project_id, database) = coordinator(engine);
        let directory = output_dir("rejected");
        let view = coordinator
            .execute(&project_id, "SELECT 1", options(&directory))
            .unwrap();
        assert_eq!(view.state, ExportState::Failed);
        assert_eq!(view.error.as_ref().unwrap().code, "export.rejected");
        let history = SourcesRepository::new(database)
            .get_export(&view.export_id)
            .unwrap()
            .unwrap();
        assert_eq!(history.status, HistoryStatus::Failed);
        assert_eq!(history.error_code.as_deref(), Some("export.rejected"));
        std::fs::remove_dir(directory).unwrap();
    }
}

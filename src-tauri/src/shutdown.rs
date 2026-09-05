use std::{
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use tauri::{Emitter, Manager};

use crate::{
    engine_manager::EngineManager,
    export::ExportCoordinator,
    metadata::MetadataDb,
    observability::{AppLogger, EventFields, LogLevel},
    profile::ProfileCoordinator,
    projects::ProjectManager,
    query::QueryCoordinator,
    results::ResultStore,
};

const JOB_WAIT: Duration = Duration::from_secs(2);
const JOB_POLL: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    WaitingForDraft,
    Running,
    Complete,
}

struct OwnedShutdownResources {
    queries: Arc<QueryCoordinator>,
    exports: Arc<ExportCoordinator>,
    profiles: Arc<ProfileCoordinator>,
    results: Arc<ResultStore>,
    projects: ProjectManager,
    engine: Arc<EngineManager>,
    metadata: MetadataDb,
    logger: Arc<AppLogger>,
}

pub struct ShutdownCoordinator {
    state: Mutex<State>,
    resources: Option<OwnedShutdownResources>,
}

impl ShutdownCoordinator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        queries: Arc<QueryCoordinator>,
        exports: Arc<ExportCoordinator>,
        profiles: Arc<ProfileCoordinator>,
        results: Arc<ResultStore>,
        projects: ProjectManager,
        engine: Arc<EngineManager>,
        metadata: MetadataDb,
        logger: Arc<AppLogger>,
    ) -> Self {
        Self {
            state: Mutex::new(State::Idle),
            resources: Some(OwnedShutdownResources {
                queries,
                exports,
                profiles,
                results,
                projects,
                engine,
                metadata,
                logger,
            }),
        }
    }

    #[cfg(test)]
    fn state_only() -> Self {
        Self {
            state: Mutex::new(State::Idle),
            resources: None,
        }
    }

    pub fn frontend_ready(&self) {
        if let Ok(mut state) = self.state.lock() {
            if *state == State::Idle {
                *state = State::WaitingForDraft;
            }
        }
    }

    pub fn should_intercept_close(&self) -> bool {
        self.state
            .lock()
            .map(|state| matches!(*state, State::WaitingForDraft | State::Running))
            .unwrap_or(false)
    }

    fn begin(&self, skip_draft: bool) -> Result<bool, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "shutdown state lock unavailable".to_string())?;
        match *state {
            State::Complete | State::Running => Ok(false),
            State::Idle if !skip_draft => Err("shutdown.frontend_not_ready".into()),
            State::Idle | State::WaitingForDraft => {
                *state = State::Running;
                Ok(true)
            }
        }
    }

    fn complete(&self) {
        if let Ok(mut state) = self.state.lock() {
            *state = State::Complete;
        }
    }

    fn owned_resources(&self) -> Result<&OwnedShutdownResources, String> {
        self.resources
            .as_ref()
            .ok_or_else(|| "shutdown resources are unavailable".to_string())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShutdownReport {
    phase: &'static str,
    queries_cancelled: u64,
    exports_cancelled: u64,
    profiles_cancelled: u64,
    results_released: u64,
    metadata_checkpointed: bool,
    warnings: Vec<String>,
}

struct ShutdownResources<'a> {
    queries: &'a Arc<QueryCoordinator>,
    exports: &'a Arc<ExportCoordinator>,
    profiles: &'a Arc<ProfileCoordinator>,
    results: &'a Arc<ResultStore>,
    projects: &'a ProjectManager,
    engine: &'a Arc<EngineManager>,
    metadata: &'a MetadataDb,
    logger: &'a Arc<AppLogger>,
}

fn run_shutdown(resources: ShutdownResources<'_>) -> ShutdownReport {
    let mut report = ShutdownReport {
        phase: "complete",
        queries_cancelled: resources.queries.cancel_all(),
        exports_cancelled: resources.exports.cancel_all(),
        profiles_cancelled: resources.profiles.cancel_all(),
        results_released: 0,
        metadata_checkpointed: false,
        warnings: Vec::new(),
    };

    let deadline = Instant::now() + JOB_WAIT;
    while Instant::now() < deadline
        && (resources.queries.has_active()
            || resources.exports.has_active()
            || resources.profiles.has_active())
    {
        thread::sleep(JOB_POLL);
    }
    if resources.queries.has_active()
        || resources.exports.has_active()
        || resources.profiles.has_active()
    {
        report
            .warnings
            .push("active jobs exceeded the two-second shutdown wait".into());
    }

    match resources.results.release_all() {
        Ok(count) => report.results_released = count,
        Err(error) => report
            .warnings
            .push(format!("result release failed: {error}")),
    }
    if let Err(error) = resources.projects.close() {
        report
            .warnings
            .push(format!("project session close failed: {error}"));
    }
    resources.engine.shutdown();
    match resources.metadata.checkpoint() {
        Ok(()) => report.metadata_checkpointed = true,
        Err(error) => report
            .warnings
            .push(format!("metadata checkpoint failed: {error}")),
    }
    resources.logger.record(
        if report.warnings.is_empty() {
            LogLevel::Info
        } else {
            LogLevel::Warning
        },
        "app",
        "graceful_shutdown",
        EventFields {
            status: Some(if report.warnings.is_empty() {
                "succeeded"
            } else {
                "completed_with_warnings"
            }),
            message: report.warnings.first().map(String::as_str),
            ..EventFields::default()
        },
    );
    if let Err(error) = resources.logger.flush() {
        report.warnings.push(format!("log flush failed: {error}"));
    }
    report
}

#[tauri::command]
pub fn register_shutdown_ready(coordinator: tauri::State<'_, Arc<ShutdownCoordinator>>) {
    coordinator.frontend_ready();
}

#[tauri::command]
pub async fn complete_shutdown<R: tauri::Runtime>(
    skip_draft: bool,
    app: tauri::AppHandle<R>,
    coordinator: tauri::State<'_, Arc<ShutdownCoordinator>>,
) -> Result<ShutdownReport, String> {
    if !coordinator.begin(skip_draft)? {
        return Ok(ShutdownReport {
            phase: "complete",
            queries_cancelled: 0,
            exports_cancelled: 0,
            profiles_cancelled: 0,
            results_released: 0,
            metadata_checkpointed: true,
            warnings: Vec::new(),
        });
    }
    let resources = coordinator.owned_resources()?;
    let queries = resources.queries.clone();
    let exports = resources.exports.clone();
    let profiles = resources.profiles.clone();
    let results = resources.results.clone();
    let projects = resources.projects.clone();
    let engine = resources.engine.clone();
    let metadata = resources.metadata.clone();
    let logger = resources.logger.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        run_shutdown(ShutdownResources {
            queries: &queries,
            exports: &exports,
            profiles: &profiles,
            results: &results,
            projects: &projects,
            engine: &engine,
            metadata: &metadata,
            logger: &logger,
        })
    })
    .await
    .map_err(|error| format!("shutdown task failed: {error}"))?;
    coordinator.complete();
    if let Some(window) = app.get_webview_window("main") {
        window.destroy().map_err(|error| error.to_string())?;
    } else {
        app.exit(0);
    }
    Ok(report)
}

pub fn request_frontend_shutdown<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    coordinator: &ShutdownCoordinator,
) -> bool {
    if !coordinator.should_intercept_close() {
        return false;
    }
    window.emit("shutdown-requested", ()).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_is_intercepted_only_after_frontend_registers() {
        let coordinator = ShutdownCoordinator::state_only();
        assert!(!coordinator.should_intercept_close());
        assert_eq!(
            coordinator.begin(false).unwrap_err(),
            "shutdown.frontend_not_ready"
        );

        coordinator.frontend_ready();
        assert!(coordinator.should_intercept_close());
        assert!(coordinator.begin(false).unwrap());
        assert!(coordinator.should_intercept_close());
        assert!(!coordinator.begin(false).unwrap());
        coordinator.complete();
        assert!(!coordinator.should_intercept_close());
        assert!(!coordinator.begin(true).unwrap());
    }

    #[test]
    fn forced_shutdown_can_start_without_frontend_registration() {
        let coordinator = ShutdownCoordinator::state_only();
        assert!(coordinator.begin(true).unwrap());
        coordinator.complete();
        assert!(!coordinator.begin(true).unwrap());
    }
}

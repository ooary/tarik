//! Tauri commands for the query execution lifecycle.

use std::sync::Arc;

use tauri::State;

use super::QueryCoordinator;
use crate::{engine_manager::EngineManager, observability::AppLogger, projects::ProjectManager};

#[tauri::command]
pub fn validate_query(
    project_id: String,
    sql: String,
    revision: u64,
    engine: State<'_, Arc<EngineManager>>,
    projects: State<'_, ProjectManager>,
) -> Result<tarik_engine_protocol::SqlValidation, String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "validation.no_active_project".to_string())?;
    if active.id != project_id {
        return Err(format!(
            "validation.project_mismatch: active project is {}, request was {}",
            active.id, project_id
        ));
    }
    if sql.trim().is_empty() {
        return Ok(tarik_engine_protocol::SqlValidation {
            revision,
            diagnostics: Vec::new(),
        });
    }
    engine.validate_query(&sql, revision)
}

#[tauri::command]
pub fn execute_query(
    project_id: String,
    tab_id: String,
    sql: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
    projects: State<'_, ProjectManager>,
    logger: State<'_, Arc<AppLogger>>,
) -> Result<super::ExecutionView, String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "query.no_active_project".to_string())?;
    if active.id != project_id {
        return Err(format!(
            "query.project_mismatch: active project is {}, request was {}",
            active.id, project_id
        ));
    }
    let span = logger
        .inner()
        .operation("query", "submit", Some(active.id.clone()));
    match coordinator.execute(&active.id, &tab_id, &sql) {
        Ok(view) => {
            span.succeed();
            Ok(view)
        }
        Err(error) => {
            span.fail("query.submit", &error);
            Err(error)
        }
    }
}

#[tauri::command]
pub fn get_query_status(
    execution_id: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
) -> Result<Option<super::ExecutionView>, String> {
    Ok(coordinator.status(&execution_id))
}

#[tauri::command]
pub fn get_tab_execution(
    project_id: String,
    tab_id: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
) -> Result<Option<super::ExecutionView>, String> {
    Ok(coordinator.latest_for_tab(&project_id, &tab_id))
}

#[tauri::command]
pub fn cancel_query(
    execution_id: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
) -> Result<super::ExecutionView, String> {
    coordinator.cancel(&execution_id)
}

#[tauri::command]
pub fn forget_tab_execution(
    tab_id: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
) -> Result<(), String> {
    coordinator.forget(&tab_id)
}

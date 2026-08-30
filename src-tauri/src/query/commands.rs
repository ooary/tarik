//! Tauri commands for the query execution lifecycle.

use std::sync::Arc;

use tauri::State;

use super::QueryCoordinator;
use crate::projects::ProjectManager;

#[tauri::command]
pub fn execute_query(
    project_id: String,
    tab_id: String,
    sql: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
    projects: State<'_, ProjectManager>,
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
    coordinator.execute(&active.id, &tab_id, &sql)
}

#[tauri::command]
pub fn get_query_status(
    execution_id: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
) -> Result<Option<super::ExecutionView>, String> {
    Ok(coordinator.status(&execution_id))
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

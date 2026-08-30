//! Tauri commands for the query execution lifecycle.

use std::sync::Arc;

use tauri::State;

use super::QueryCoordinator;

#[tauri::command]
pub fn execute_query(
    project_id: String,
    tab_id: String,
    sql: String,
    coordinator: State<'_, Arc<QueryCoordinator>>,
) -> Result<super::ExecutionView, String> {
    coordinator.execute(&project_id, &tab_id, &sql)
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

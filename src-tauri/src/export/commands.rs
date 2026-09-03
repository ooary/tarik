use std::sync::Arc;

use tarik_engine_protocol::ExportOptions;
use tauri::State;

use super::{ExportCoordinator, ExportView};
use crate::projects::ProjectManager;

#[tauri::command]
pub fn execute_export(
    project_id: String,
    sql: String,
    options: ExportOptions,
    coordinator: State<'_, Arc<ExportCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<ExportView, String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "export.no_active_project".to_string())?;
    if active.id != project_id {
        return Err(format!(
            "export.project_mismatch: active project is {}, request was {}",
            active.id, project_id
        ));
    }
    coordinator.execute(&project_id, &sql, options)
}

#[tauri::command]
pub fn get_export_status(
    export_id: String,
    coordinator: State<'_, Arc<ExportCoordinator>>,
) -> Result<Option<ExportView>, String> {
    Ok(coordinator.status(&export_id))
}

#[tauri::command]
pub fn cancel_export(
    export_id: String,
    coordinator: State<'_, Arc<ExportCoordinator>>,
) -> Result<ExportView, String> {
    coordinator.cancel(&export_id)
}

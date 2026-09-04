use std::sync::Arc;

use tarik_engine_protocol::ExportOptions;
use tauri::State;

use super::{ExportCoordinator, ExportView};
use crate::{observability::AppLogger, projects::ProjectManager};

#[tauri::command]
pub fn execute_export(
    project_id: String,
    sql: String,
    options: ExportOptions,
    coordinator: State<'_, Arc<ExportCoordinator>>,
    projects: State<'_, ProjectManager>,
    logger: State<'_, Arc<AppLogger>>,
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
    let span = logger
        .inner()
        .operation("export", "submit", Some(project_id.clone()));
    match coordinator.execute(&project_id, &sql, options) {
        Ok(view) => {
            span.succeed();
            Ok(view)
        }
        Err(error) => {
            span.fail("export.submit", &error);
            Err(error)
        }
    }
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

#[tauri::command]
pub fn reveal_export_part<R: tauri::Runtime>(
    export_id: String,
    part_number: u64,
    app: tauri::AppHandle<R>,
    coordinator: State<'_, Arc<ExportCoordinator>>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;

    let path = coordinator
        .completed_part_path(&export_id, part_number)
        .ok_or_else(|| "export part does not belong to a completed tracked export".to_string())?;
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|error| error.to_string())
}

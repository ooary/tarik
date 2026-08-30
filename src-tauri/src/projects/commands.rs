use serde::Deserialize;
use tauri::State;

use crate::engine::{EngineProfile, EngineProfileName, ProjectCatalog};

use super::{ActiveProject, ProjectManager};

#[tauri::command]
pub fn create_project(
    name: String,
    manager: State<'_, ProjectManager>,
) -> Result<ActiveProject, String> {
    manager.create(&name).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn open_project(
    name: String,
    duckdb_path: String,
    manager: State<'_, ProjectManager>,
) -> Result<ActiveProject, String> {
    manager
        .open(&name, std::path::Path::new(&duckdb_path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn close_project(manager: State<'_, ProjectManager>) -> Result<bool, String> {
    manager.close().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_active_project(
    manager: State<'_, ProjectManager>,
) -> Result<Option<ActiveProject>, String> {
    manager.active().map_err(|error| error.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineProfileInput {
    name: EngineProfileName,
    memory_limit_mb: u32,
    threads: u16,
    temp_directory: String,
}

#[tauri::command]
pub fn apply_engine_profile(
    profile: EngineProfileInput,
    manager: State<'_, ProjectManager>,
) -> Result<(), String> {
    manager
        .apply_profile(EngineProfile {
            name: profile.name,
            memory_limit_mb: profile.memory_limit_mb,
            threads: profile.threads,
            temp_directory: profile.temp_directory.into(),
        })
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn inspect_project_catalog(
    manager: State<'_, ProjectManager>,
) -> Result<ProjectCatalog, String> {
    manager.catalog().map_err(|error| error.to_string())
}

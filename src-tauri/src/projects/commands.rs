use serde::Deserialize;
use tauri::State;

use crate::engine::{EngineProfile, EngineProfileName, ProjectCatalog};

use super::{ActiveProject, ProjectManager};

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, super::ProjectError> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("project task failed: {error}"))?
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn create_project(
    name: String,
    manager: State<'_, ProjectManager>,
) -> Result<ActiveProject, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.create(&name)).await
}

#[tauri::command]
pub async fn open_project(
    name: String,
    duckdb_path: String,
    manager: State<'_, ProjectManager>,
) -> Result<ActiveProject, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.open(&name, std::path::Path::new(&duckdb_path))).await
}

#[tauri::command]
pub async fn close_project(manager: State<'_, ProjectManager>) -> Result<bool, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.close()).await
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
pub async fn apply_engine_profile(
    profile: EngineProfileInput,
    manager: State<'_, ProjectManager>,
) -> Result<(), String> {
    let manager = manager.inner().clone();
    blocking(move || {
        manager.apply_profile(EngineProfile {
            name: profile.name,
            memory_limit_mb: profile.memory_limit_mb,
            threads: profile.threads,
            temp_directory: profile.temp_directory.into(),
        })
    })
    .await
}

#[tauri::command]
pub async fn inspect_project_catalog(
    manager: State<'_, ProjectManager>,
) -> Result<ProjectCatalog, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.catalog()).await
}

use serde::Deserialize;
use tauri::State;

use crate::{
    engine::{EngineProfile, EngineProfileName, ProjectCatalog},
    metadata::{sources::SourcesRepository, MetadataDb},
    sources::{operations::SourceMutationResult, CsvOptions, ImportOptions, SourceInspection},
};

use super::{ActiveProject, ProjectManager, ProjectRemoval};

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
pub async fn reopen_recent_project(
    project_id: String,
    manager: State<'_, ProjectManager>,
) -> Result<ActiveProject, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.reopen(&project_id)).await
}

#[tauri::command]
pub async fn rename_project(
    project_id: String,
    new_name: String,
    manager: State<'_, ProjectManager>,
) -> Result<crate::metadata::projects::RecentProject, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.rename(&project_id, &new_name)).await
}

#[tauri::command]
pub async fn remove_project(
    project_id: String,
    manager: State<'_, ProjectManager>,
) -> Result<ProjectRemoval, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.remove(&project_id)).await
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
pub async fn inspect_source_file(
    path: String,
    csv: Option<CsvOptions>,
    manager: State<'_, ProjectManager>,
) -> Result<SourceInspection, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.inspect_source(path.into(), csv)).await
}

#[tauri::command]
pub async fn link_parquet_source(
    path: String,
    view_name: String,
    manager: State<'_, ProjectManager>,
    database: State<'_, MetadataDb>,
) -> Result<SourceMutationResult, String> {
    let manager = manager.inner().clone();
    let result = blocking(move || manager.link_parquet(path.into(), view_name)).await?;
    SourcesRepository::new(database.inner().clone())
        .upsert_source(&result.source)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
pub async fn import_source_table(
    path: String,
    options: ImportOptions,
    manager: State<'_, ProjectManager>,
    database: State<'_, MetadataDb>,
) -> Result<SourceMutationResult, String> {
    let manager = manager.inner().clone();
    let result = blocking(move || manager.import_table(path.into(), options)).await?;
    SourcesRepository::new(database.inner().clone())
        .upsert_source(&result.source)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
pub fn cancel_source_operation(manager: State<'_, ProjectManager>) -> Result<bool, String> {
    manager.interrupt().map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn repair_linked_source(
    source_id: String,
    replacement: String,
    manager: State<'_, ProjectManager>,
    database: State<'_, MetadataDb>,
) -> Result<SourceMutationResult, String> {
    let repository = SourcesRepository::new(database.inner().clone());
    let source = repository
        .get_source(&source_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("source was not found: {source_id}"))?;
    let manager = manager.inner().clone();
    let result = blocking(move || manager.repair_link(source, replacement.into())).await?;
    repository
        .upsert_source(&result.source)
        .map_err(|error| error.to_string())?;
    Ok(result)
}

#[tauri::command]
pub async fn remove_linked_source(
    source_id: String,
    manager: State<'_, ProjectManager>,
    database: State<'_, MetadataDb>,
) -> Result<bool, String> {
    let repository = SourcesRepository::new(database.inner().clone());
    let source = repository
        .get_source(&source_id)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| format!("source was not found: {source_id}"))?;
    let manager = manager.inner().clone();
    blocking(move || manager.drop_link(source)).await?;
    repository
        .remove_source(&source_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn inspect_project_catalog(
    manager: State<'_, ProjectManager>,
) -> Result<ProjectCatalog, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.catalog()).await
}

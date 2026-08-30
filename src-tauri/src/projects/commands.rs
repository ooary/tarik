use serde::Serialize;
use tarik_engine_protocol::{
    CatalogSnapshot, CsvOptions, ImportOptions, SourceInspection, SourceKind, SourceRecord,
    SourceState,
};
use tauri::State;

use crate::metadata::{
    sources::{self, SourcesRepository},
    MetadataDb,
};

use super::{ActiveProject, ProjectManager, ProjectRemoval};

fn to_metadata_source(source: &SourceRecord) -> sources::SourceRecord {
    sources::SourceRecord {
        id: source.id.clone(),
        project_id: source.project_id.clone(),
        display_name: source.display_name.clone(),
        kind: match source.kind {
            SourceKind::DuckdbTable => sources::SourceKind::DuckdbTable,
            SourceKind::LinkedParquet => sources::SourceKind::LinkedParquet,
            SourceKind::LinkedCsv => sources::SourceKind::LinkedCsv,
        },
        state: match source.state {
            SourceState::Ready => sources::SourceState::Ready,
            SourceState::Missing => sources::SourceState::Missing,
            SourceState::InvalidSchema => sources::SourceState::InvalidSchema,
        },
        source_path: source.source_path.clone(),
        duckdb_name: source.duckdb_name.clone(),
        options: serde_json::Value::Object(source.options.clone()),
        created_at: source.created_at.clone(),
        updated_at: source.updated_at.clone(),
    }
}

fn from_metadata_source(source: &sources::SourceRecord) -> SourceRecord {
    SourceRecord {
        id: source.id.clone(),
        project_id: source.project_id.clone(),
        display_name: source.display_name.clone(),
        kind: match source.kind {
            sources::SourceKind::DuckdbTable => SourceKind::DuckdbTable,
            sources::SourceKind::LinkedParquet => SourceKind::LinkedParquet,
            sources::SourceKind::LinkedCsv => SourceKind::LinkedCsv,
        },
        state: match source.state {
            sources::SourceState::Ready => SourceState::Ready,
            sources::SourceState::Missing => SourceState::Missing,
            sources::SourceState::InvalidSchema => SourceState::InvalidSchema,
        },
        source_path: source.source_path.clone(),
        duckdb_name: source.duckdb_name.clone(),
        options: source.options.as_object().cloned().unwrap_or_default(),
        created_at: source.created_at.clone(),
        updated_at: source.updated_at.clone(),
    }
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, super::ProjectError> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|error| format!("project task failed: {error}"))?
        .map_err(|error| error.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMutationResult {
    pub source: SourceRecord,
    pub inspection: Option<SourceInspection>,
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
    let source = blocking(move || manager.link_parquet(path.into(), view_name)).await?;
    SourcesRepository::new(database.inner().clone())
        .upsert_source(&to_metadata_source(&source))
        .map_err(|error| error.to_string())?;
    Ok(SourceMutationResult {
        source,
        inspection: None,
    })
}

#[tauri::command]
pub async fn import_source_table(
    path: String,
    options: ImportOptions,
    manager: State<'_, ProjectManager>,
    database: State<'_, MetadataDb>,
) -> Result<SourceMutationResult, String> {
    let manager = manager.inner().clone();
    let source = blocking(move || manager.import_table(path.into(), options)).await?;
    SourcesRepository::new(database.inner().clone())
        .upsert_source(&to_metadata_source(&source))
        .map_err(|error| error.to_string())?;
    Ok(SourceMutationResult {
        source,
        inspection: None,
    })
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
    let engine_source = from_metadata_source(&source);
    let manager = manager.inner().clone();
    let source = blocking(move || manager.repair_link(engine_source, replacement.into())).await?;
    repository
        .upsert_source(&to_metadata_source(&source))
        .map_err(|error| error.to_string())?;
    Ok(SourceMutationResult {
        source,
        inspection: None,
    })
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
    let engine_source = from_metadata_source(&source);
    let manager = manager.inner().clone();
    blocking(move || manager.drop_link(engine_source)).await?;
    repository
        .remove_source(&source_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn inspect_project_catalog(
    manager: State<'_, ProjectManager>,
) -> Result<CatalogSnapshot, String> {
    let manager = manager.inner().clone();
    blocking(move || manager.catalog()).await
}

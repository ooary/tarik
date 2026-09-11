use std::sync::Arc;

use serde::Serialize;
use tarik_engine_protocol::{
    CatalogSnapshot, CreateTableDefinition, CsvOptions, ImportOptions, SourceInspection,
    SourceKind, SourceRecord, SourceState,
};
use tauri::State;

use crate::{
    metadata::{
        sources::{self, SourcesRepository},
        MetadataDb,
    },
    observability::AppLogger,
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
    logger: State<'_, Arc<AppLogger>>,
) -> Result<ActiveProject, String> {
    let span = logger.inner().operation("project", "create", None);
    let manager = manager.inner().clone();
    match blocking(move || manager.create(&name)).await {
        Ok(project) => {
            span.succeed();
            Ok(project)
        }
        Err(error) => {
            span.fail("project.create", &error);
            Err(error)
        }
    }
}

#[tauri::command]
pub async fn open_project(
    name: String,
    duckdb_path: String,
    manager: State<'_, ProjectManager>,
    logger: State<'_, Arc<AppLogger>>,
) -> Result<ActiveProject, String> {
    let span = logger.inner().operation("project", "open", None);
    let manager = manager.inner().clone();
    match blocking(move || manager.open(&name, std::path::Path::new(&duckdb_path))).await {
        Ok(project) => {
            span.succeed();
            Ok(project)
        }
        Err(error) => {
            span.fail("project.open", &error);
            Err(error)
        }
    }
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
pub async fn close_project(
    manager: State<'_, ProjectManager>,
    agent_access: State<'_, Arc<crate::agent_access::AgentAccessManager>>,
    logger: State<'_, Arc<AppLogger>>,
) -> Result<bool, String> {
    let project_id = manager.active().ok().flatten().map(|project| project.id);
    if let Some(project_id) = &project_id {
        agent_access.invalidate_project(project_id);
    }
    let span = logger.inner().operation("project", "close", project_id);
    let manager = manager.inner().clone();
    match blocking(move || manager.close()).await {
        Ok(closed) => {
            span.succeed();
            Ok(closed)
        }
        Err(error) => {
            span.fail("project.close", &error);
            Err(error)
        }
    }
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
pub async fn create_table(
    project_id: String,
    definition: CreateTableDefinition,
    manager: State<'_, ProjectManager>,
) -> Result<bool, String> {
    let active = manager
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no DuckDB project is open".to_string())?;
    if active.id != project_id {
        return Err("table does not belong to the active project".to_string());
    }
    let manager = manager.inner().clone();
    blocking(move || manager.create_table(definition)).await?;
    Ok(true)
}

#[tauri::command]
pub async fn drop_catalog_object(
    project_id: String,
    database_name: String,
    schema: String,
    name: String,
    kind: String,
    manager: State<'_, ProjectManager>,
    metadata: State<'_, MetadataDb>,
) -> Result<bool, String> {
    let active = manager
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "no DuckDB project is open".to_string())?;
    if active.id != project_id {
        return Err("catalog object does not belong to the active project".to_string());
    }
    let manager = manager.inner().clone();
    let object_name = name.clone();
    blocking(move || manager.drop_catalog_object(&database_name, &schema, &name, &kind)).await?;
    SourcesRepository::new(metadata.inner().clone())
        .remove_by_object_name(&project_id, &object_name)
        .map_err(|error| error.to_string())?;
    Ok(true)
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

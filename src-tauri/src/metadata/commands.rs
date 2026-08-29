use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;

use super::{
    projects::{ProjectsRepository, RecentProject},
    queries::{ExecutionStatus, QueriesRepository, QueryHistoryEntry, SavedQuery},
    sessions::{QuerySessionSnapshot, SessionsRepository},
    settings::SettingsRepository,
    MetadataDb,
};

const WORKBENCH_PREFERENCES_KEY: &str = "workbench.preferences";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbenchPreferences {
    pub theme: String,
    pub sidebar_width: f64,
    pub bottom_panel_height: f64,
    pub sidebar_open: bool,
    pub bottom_panel_open: bool,
    pub active_output_panel: String,
}

#[tauri::command]
pub fn get_workbench_preferences(
    database: State<'_, MetadataDb>,
) -> Result<Option<WorkbenchPreferences>, String> {
    SettingsRepository::new(database.inner().clone())
        .get(WORKBENCH_PREFERENCES_KEY)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn set_workbench_preferences(
    preferences: WorkbenchPreferences,
    database: State<'_, MetadataDb>,
) -> Result<(), String> {
    SettingsRepository::new(database.inner().clone())
        .set(WORKBENCH_PREFERENCES_KEY, &preferences)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn upsert_recent_project(
    name: String,
    duckdb_path: String,
    database: State<'_, MetadataDb>,
) -> Result<RecentProject, String> {
    ProjectsRepository::new(database.inner().clone())
        .upsert(&name, Path::new(&duckdb_path))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_recent_projects(database: State<'_, MetadataDb>) -> Result<Vec<RecentProject>, String> {
    ProjectsRepository::new(database.inner().clone())
        .list()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn touch_recent_project(id: String, database: State<'_, MetadataDb>) -> Result<bool, String> {
    ProjectsRepository::new(database.inner().clone())
        .touch(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn upsert_saved_query(
    query: SavedQuery,
    database: State<'_, MetadataDb>,
) -> Result<(), String> {
    QueriesRepository::new(database.inner().clone())
        .upsert_saved(&query)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_saved_queries(
    project_id: String,
    search: Option<String>,
    database: State<'_, MetadataDb>,
) -> Result<Vec<SavedQuery>, String> {
    QueriesRepository::new(database.inner().clone())
        .list_saved(&project_id, search.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_saved_query(id: String, database: State<'_, MetadataDb>) -> Result<bool, String> {
    QueriesRepository::new(database.inner().clone())
        .delete_saved(&id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn add_query_history(
    entry: QueryHistoryEntry,
    database: State<'_, MetadataDb>,
) -> Result<(), String> {
    QueriesRepository::new(database.inner().clone())
        .add_history(&entry)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_query_history(
    project_id: String,
    status: Option<ExecutionStatus>,
    limit: u32,
    database: State<'_, MetadataDb>,
) -> Result<Vec<QueryHistoryEntry>, String> {
    QueriesRepository::new(database.inner().clone())
        .list_history(&project_id, status, limit)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn prune_query_history(
    project_id: String,
    keep: u32,
    database: State<'_, MetadataDb>,
) -> Result<usize, String> {
    QueriesRepository::new(database.inner().clone())
        .prune_history(&project_id, keep)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_query_session(
    snapshot: QuerySessionSnapshot,
    database: State<'_, MetadataDb>,
) -> Result<(), String> {
    SessionsRepository::new(database.inner().clone())
        .save_snapshot(&snapshot)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn load_query_session(
    session_id: String,
    database: State<'_, MetadataDb>,
) -> Result<Option<QuerySessionSnapshot>, String> {
    SessionsRepository::new(database.inner().clone())
        .load(&session_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn remove_recent_project(id: String, database: State<'_, MetadataDb>) -> Result<bool, String> {
    ProjectsRepository::new(database.inner().clone())
        .remove(&id)
        .map_err(|error| error.to_string())
}

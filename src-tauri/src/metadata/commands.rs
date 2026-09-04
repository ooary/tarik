use serde::{Deserialize, Serialize};
use tauri::State;

use super::{
    projects::{ProjectsRepository, RecentProject},
    queries::{
        HistoryPruneSummary, HistoryRetentionPolicy, QueriesRepository, QueryFolder,
        QueryHistoryFilter, QueryHistoryPage, SavedQuery, SavedQueryDraft,
    },
    sessions::{QuerySessionSnapshot, SessionsRepository},
    settings::SettingsRepository,
    sources::{SourceRecord, SourcesRepository},
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
    #[serde(default, skip_serializing)]
    pub active_output_panel: Option<String>,
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
pub fn list_recent_projects(database: State<'_, MetadataDb>) -> Result<Vec<RecentProject>, String> {
    ProjectsRepository::new(database.inner().clone())
        .list()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_sources(
    project_id: String,
    database: State<'_, MetadataDb>,
) -> Result<Vec<SourceRecord>, String> {
    SourcesRepository::new(database.inner().clone())
        .list_sources(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_saved_query(
    draft: SavedQueryDraft,
    database: State<'_, MetadataDb>,
) -> Result<SavedQuery, String> {
    QueriesRepository::new(database.inner().clone())
        .create_saved(&draft)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn update_saved_query(
    id: String,
    draft: SavedQueryDraft,
    database: State<'_, MetadataDb>,
) -> Result<SavedQuery, String> {
    QueriesRepository::new(database.inner().clone())
        .update_saved(&id, &draft)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn create_query_folder(
    project_id: String,
    name: String,
    database: State<'_, MetadataDb>,
) -> Result<QueryFolder, String> {
    QueriesRepository::new(database.inner().clone())
        .create_folder(&project_id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn rename_query_folder(
    project_id: String,
    id: String,
    name: String,
    database: State<'_, MetadataDb>,
) -> Result<QueryFolder, String> {
    QueriesRepository::new(database.inner().clone())
        .rename_folder(&project_id, &id, &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_query_folders(
    project_id: String,
    database: State<'_, MetadataDb>,
) -> Result<Vec<QueryFolder>, String> {
    QueriesRepository::new(database.inner().clone())
        .list_folders(&project_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_query_folder(
    project_id: String,
    id: String,
    database: State<'_, MetadataDb>,
) -> Result<bool, String> {
    QueriesRepository::new(database.inner().clone())
        .delete_folder(&project_id, &id)
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
pub fn delete_saved_query(
    project_id: String,
    id: String,
    database: State<'_, MetadataDb>,
) -> Result<bool, String> {
    QueriesRepository::new(database.inner().clone())
        .delete_saved(&project_id, &id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn list_query_history_page(
    project_id: String,
    filter: QueryHistoryFilter,
    database: State<'_, MetadataDb>,
) -> Result<QueryHistoryPage, String> {
    QueriesRepository::new(database.inner().clone())
        .list_history_page(&project_id, &filter)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn apply_query_history_retention(
    project_id: String,
    policy: HistoryRetentionPolicy,
    database: State<'_, MetadataDb>,
) -> Result<HistoryPruneSummary, String> {
    QueriesRepository::new(database.inner().clone())
        .apply_history_retention(&project_id, &policy, chrono::Utc::now())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn clear_query_history(
    project_id: String,
    database: State<'_, MetadataDb>,
) -> Result<HistoryPruneSummary, String> {
    QueriesRepository::new(database.inner().clone())
        .clear_history(&project_id)
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

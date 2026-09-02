mod engine_manager;
mod metadata;
mod paths;
mod plan;
mod projects;
mod query;
mod results;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Serialize;
use tauri::Manager;

/// Locate the DuckDB engine adapter binary.
///
/// Development: sibling of the current executable or the workspace debug build.
/// Packaged: a sibling `tarik-engine-duckdb` next to `tarik`.
fn locate_engine_binary() -> PathBuf {
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let sibling = parent.join("tarik-engine-duckdb");
            if sibling.exists() {
                return sibling;
            }
        }
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let debug = workspace.join("target/debug/tarik-engine-duckdb");
    if debug.exists() {
        return debug;
    }
    workspace.join("target/release/tarik-engine-duckdb")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeInfo {
    app_name: &'static str,
    app_version: &'static str,
    rust_target: &'static str,
}

fn runtime_info() -> RuntimeInfo {
    RuntimeInfo {
        app_name: "Tarik",
        app_version: env!("CARGO_PKG_VERSION"),
        rust_target: std::env::consts::OS,
    }
}

#[tauri::command]
fn get_runtime_info() -> RuntimeInfo {
    runtime_info()
}

#[tauri::command]
fn get_app_directories<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> Result<paths::AppDirectories, String> {
    paths::resolve_directories(&app).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directories = paths::resolve_directories(app.handle())
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let database = metadata::MetadataDb::open(directories.data_dir.join("tarik.sqlite"))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let result_root = directories.cache_dir.join("results");
            // Result artifacts are ephemeral by definition; a crashed or
            // killed session must not leave page directories behind.
            results::cleanup_stale_results(&result_root);
            let engine = Arc::new(engine_manager::EngineManager::new(
                locate_engine_binary(),
                result_root,
            ));
            let project_manager = projects::ProjectManager::new(
                database.clone(),
                directories.data_dir.join("projects"),
                engine.clone(),
            );
            let coordinator = Arc::new(query::QueryCoordinator::new(
                engine.clone(),
                database.clone(),
            ));
            let results_store = Arc::new(results::ResultStore::new(engine.clone()));
            app.manage(database);
            app.manage(engine);
            app.manage(project_manager);
            app.manage(coordinator);
            app.manage(results_store);
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(engine) = window
                    .app_handle()
                    .try_state::<Arc<engine_manager::EngineManager>>()
                {
                    engine.shutdown();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_info,
            get_app_directories,
            metadata::commands::get_workbench_preferences,
            metadata::commands::set_workbench_preferences,
            metadata::commands::upsert_recent_project,
            metadata::commands::list_recent_projects,
            metadata::commands::touch_recent_project,
            metadata::commands::remove_recent_project,
            metadata::commands::save_query_session,
            metadata::commands::load_query_session,
            metadata::commands::create_saved_query,
            metadata::commands::update_saved_query,
            metadata::commands::list_saved_queries,
            metadata::commands::delete_saved_query,
            metadata::commands::create_query_folder,
            metadata::commands::rename_query_folder,
            metadata::commands::list_query_folders,
            metadata::commands::delete_query_folder,
            metadata::commands::add_query_history,
            metadata::commands::list_query_history,
            metadata::commands::list_query_history_page,
            metadata::commands::prune_query_history,
            metadata::commands::upsert_source,
            metadata::commands::list_sources,
            metadata::commands::remove_source,
            metadata::commands::set_source_state,
            metadata::commands::get_source,
            metadata::commands::upsert_export_history,
            metadata::commands::get_export_history,
            projects::commands::create_project,
            projects::commands::open_project,
            projects::commands::reopen_recent_project,
            projects::commands::rename_project,
            projects::commands::remove_project,
            projects::commands::close_project,
            projects::commands::get_active_project,
            projects::commands::inspect_project_catalog,
            projects::commands::inspect_source_file,
            projects::commands::link_parquet_source,
            projects::commands::import_source_table,
            projects::commands::cancel_source_operation,
            projects::commands::repair_linked_source,
            projects::commands::drop_catalog_object,
            projects::commands::remove_linked_source,
            plan::commands::explain_query_plan,
            query::commands::execute_query,
            query::commands::get_query_status,
            query::commands::cancel_query,
            query::commands::forget_tab_execution,
            results::get_result_page,
            results::release_result,
            results::release_all_results
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tarik");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_application_runtime() {
        let info = runtime_info();

        assert_eq!(info.app_name, "Tarik");
        assert_eq!(info.app_version, env!("CARGO_PKG_VERSION"));
        assert!(!info.rust_target.is_empty());
    }
}

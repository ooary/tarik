mod engine_manager;
mod engine_resources;
mod export;
#[cfg(test)]
mod golden_tests;
mod metadata;
mod observability;
mod paths;
mod plan;
mod profile;
mod projects;
mod quality;
mod query;
mod results;
mod shutdown;
mod startup;
mod storage;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::Serialize;
use tauri::{Manager, State};

use observability::{AppLogger, EventFields, LogLevel};

/// Locate the DuckDB engine adapter binary.
///
/// Development: sibling of the current executable or the workspace debug build.
/// Packaged: a sibling `tarik-engine-duckdb[.exe]` next to `tarik[.exe]`.
fn locate_engine_binary() -> PathBuf {
    if let Ok(current) = std::env::current_exe() {
        if let Some(parent) = current.parent() {
            let sibling = parent.join(engine_executable_name());
            if sibling.exists() {
                return sibling;
            }
        }
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let debug = workspace
        .join("target/debug")
        .join(engine_executable_name());
    if debug.exists() {
        return debug;
    }
    workspace
        .join("target/release")
        .join(engine_executable_name())
}

fn engine_executable_name() -> &'static str {
    if cfg!(windows) {
        "tarik-engine-duckdb.exe"
    } else {
        "tarik-engine-duckdb"
    }
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
fn get_engine_status(
    engine: State<'_, Arc<engine_manager::EngineManager>>,
) -> engine_manager::EngineStatus {
    engine.status()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let directories = paths::resolve_directories(app.handle())
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let logger = Arc::new(AppLogger::open(directories.log_dir.clone()));
            observability::install_panic_hook(logger.clone(), app.handle().clone());
            logger.record(
                LogLevel::Info,
                "app",
                "startup",
                EventFields {
                    status: Some("started"),
                    ..EventFields::default()
                },
            );
            let database = metadata::MetadataDb::open(directories.data_dir.join("tarik.sqlite"))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let cleanup = Arc::new(storage::CleanupService::new(directories.cache_dir.clone()));
            let startup = startup::StartupCoordinator::begin(cleanup.clone(), logger.clone())
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let result_root = directories.cache_dir.join("results");
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
            let export_coordinator = Arc::new(
                export::ExportCoordinator::new(engine.clone(), database.clone())
                    .with_cleanup(cleanup.clone()),
            );
            let profile_coordinator = Arc::new(profile::ProfileCoordinator::new(engine.clone()));
            let quality_coordinator = Arc::new(quality::QualityCoordinator::new(
                engine.clone(),
                database.clone(),
            ));
            let engine_resources = Arc::new(engine_resources::EngineResourceManager::new(
                database.clone(),
                engine.clone(),
                coordinator.clone(),
                export_coordinator.clone(),
                profile_coordinator.clone(),
                quality_coordinator.clone(),
            ));
            let results_store = Arc::new(results::ResultStore::new(engine.clone()));
            let shutdown = Arc::new(shutdown::ShutdownCoordinator::new(
                coordinator.clone(),
                export_coordinator.clone(),
                profile_coordinator.clone(),
                quality_coordinator.clone(),
                results_store.clone(),
                project_manager.clone(),
                engine.clone(),
                database.clone(),
                logger.clone(),
            ));
            app.manage(logger);
            app.manage(cleanup);
            app.manage(startup);
            app.manage(database);
            app.manage(engine);
            app.manage(project_manager);
            app.manage(coordinator);
            app.manage(export_coordinator);
            app.manage(profile_coordinator);
            app.manage(quality_coordinator);
            app.manage(engine_resources);
            app.manage(results_store);
            app.manage(shutdown);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                if let Some(coordinator) = window
                    .app_handle()
                    .try_state::<Arc<shutdown::ShutdownCoordinator>>()
                {
                    if shutdown::request_frontend_shutdown(window, &coordinator) {
                        api.prevent_close();
                    }
                }
            }
            tauri::WindowEvent::Destroyed => {
                if let Some(engine) = window
                    .app_handle()
                    .try_state::<Arc<engine_manager::EngineManager>>()
                {
                    engine.shutdown();
                }
                if let Some(logger) = window.app_handle().try_state::<Arc<AppLogger>>() {
                    logger.record(
                        LogLevel::Info,
                        "app",
                        "shutdown",
                        EventFields {
                            status: Some("destroyed"),
                            ..EventFields::default()
                        },
                    );
                    let _ = logger.flush();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_info,
            get_engine_status,
            startup::get_startup_status,
            engine_resources::get_engine_resources,
            engine_resources::set_engine_resources,
            observability::get_log_info,
            observability::reveal_log_directory,
            observability::get_last_support_incident,
            observability::report_frontend_incident,
            storage::clear_cache,
            shutdown::register_shutdown_ready,
            shutdown::complete_shutdown,
            metadata::commands::get_workbench_preferences,
            metadata::commands::set_workbench_preferences,
            metadata::commands::list_recent_projects,
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
            metadata::commands::list_query_history_page,
            metadata::commands::apply_query_history_retention,
            metadata::commands::clear_query_history,
            metadata::commands::create_quality_check,
            metadata::commands::update_quality_check,
            metadata::commands::list_quality_checks,
            metadata::commands::delete_quality_check,
            metadata::commands::get_quality_check_history,
            metadata::commands::list_latest_quality_runs,
            metadata::commands::clear_quality_check_history,
            metadata::commands::list_sources,
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
            projects::commands::create_table,
            projects::commands::drop_catalog_object,
            projects::commands::remove_linked_source,
            plan::commands::explain_query_plan,
            profile::execute_profile,
            profile::get_profile_status,
            profile::cancel_profile,
            quality::preview_quality_check_sql,
            quality::run_quality_check,
            quality::run_quality_suite,
            quality::get_quality_run_detail,
            quality::rerun_quality_revision,
            quality::get_quality_run_status,
            quality::cancel_quality_run,
            quality::start_quality_failure_preview,
            quality::get_quality_failure_preview_status,
            quality::cancel_quality_failure_preview,
            quality::release_quality_failure_preview,
            query::commands::validate_query,
            query::commands::execute_query,
            query::commands::get_query_status,
            query::commands::get_tab_execution,
            query::commands::cancel_query,
            query::commands::forget_tab_execution,
            export::commands::execute_export,
            export::commands::get_export_status,
            export::commands::cancel_export,
            export::commands::reveal_export_part,
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

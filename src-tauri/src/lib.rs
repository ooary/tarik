mod metadata;
mod paths;

use serde::Serialize;
use tauri::Manager;

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
        .setup(|app| {
            let directories = paths::resolve_directories(app.handle())
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            let database = metadata::MetadataDb::open(directories.data_dir.join("tarik.sqlite"))
                .map_err(|error| Box::new(error) as Box<dyn std::error::Error>)?;
            app.manage(database);
            Ok(())
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
            metadata::commands::upsert_saved_query,
            metadata::commands::list_saved_queries,
            metadata::commands::delete_saved_query,
            metadata::commands::add_query_history,
            metadata::commands::list_query_history,
            metadata::commands::prune_query_history
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

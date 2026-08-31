use std::sync::Arc;

use tauri::State;

use super::{PlanMode, QueryPlan};
use crate::{engine_manager::EngineManager, projects::ProjectManager};

#[tauri::command]
pub async fn explain_query_plan(
    project_id: String,
    sql: String,
    mode: PlanMode,
    engine: State<'_, Arc<EngineManager>>,
    projects: State<'_, ProjectManager>,
) -> Result<QueryPlan, String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "plan.no_active_project".to_string())?;
    if active.id != project_id {
        return Err(format!(
            "plan.project_mismatch: active project is {}, request was {}",
            active.id, project_id
        ));
    }
    let engine = Arc::clone(engine.inner());
    tauri::async_runtime::spawn_blocking(move || super::capture_and_normalize(&engine, &sql, mode))
        .await
        .map_err(|error| format!("plan task failed: {error}"))?
}

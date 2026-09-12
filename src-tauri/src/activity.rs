use std::sync::Arc;

use serde::Serialize;
use tarik_engine_protocol::EffectiveEngineResources;

use crate::{
    agent_access::{AgentAccessManager, DesktopAgentConnectionSummary, DesktopAgentQuerySummary},
    engine_manager::EngineManager,
    query::{ActivityExecutionSummary, ExecutionView, QueryCoordinator},
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivitySnapshot {
    pub project_id: String,
    pub desktop_queries: Vec<ActivityExecutionSummary>,
    pub agent_queries: Vec<DesktopAgentQuerySummary>,
    pub agent_connections: Vec<DesktopAgentConnectionSummary>,
    pub resources: Option<EffectiveEngineResources>,
    pub progress_available: bool,
}

#[tauri::command]
pub fn get_activity_snapshot(
    project_id: String,
    queries: tauri::State<'_, Arc<QueryCoordinator>>,
    agents: tauri::State<'_, Arc<AgentAccessManager>>,
    engine: tauri::State<'_, Arc<EngineManager>>,
) -> Result<ActivitySnapshot, String> {
    let (agent_queries, agent_connections) = agents.desktop_activity(&project_id)?;
    Ok(ActivitySnapshot {
        project_id: project_id.clone(),
        desktop_queries: queries.activity(&project_id),
        agent_queries,
        agent_connections,
        resources: engine.effective_resources(),
        progress_available: false,
    })
}

#[tauri::command]
pub fn get_desktop_query_detail(
    execution_id: String,
    queries: tauri::State<'_, Arc<QueryCoordinator>>,
) -> Result<crate::query::DesktopQueryDetail, String> {
    queries.activity_detail(&execution_id)
}

#[tauri::command]
pub fn cancel_desktop_activity_query(
    execution_id: String,
    queries: tauri::State<'_, Arc<QueryCoordinator>>,
) -> Result<ExecutionView, String> {
    queries.cancel(&execution_id)
}

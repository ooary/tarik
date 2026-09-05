use std::sync::Arc;

use serde::Serialize;
use tarik_engine_protocol::{
    EffectiveEngineResources, EngineResourceSettings, MAX_ENGINE_MEMORY_MIB, MAX_ENGINE_THREADS,
    MIN_ENGINE_MEMORY_MIB, MIN_ENGINE_THREADS,
};
use tauri::State;

use crate::{
    engine_manager::EngineManager,
    export::ExportCoordinator,
    metadata::{settings::SettingsRepository, MetadataDb},
    projects::ProjectManager,
    query::QueryCoordinator,
};

const RESOURCE_SETTINGS_KEY: &str = "engine.resources";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceApplyState {
    Pending,
    Effective,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineResourceStatus {
    pub requested: EngineResourceSettings,
    pub effective: Option<EffectiveEngineResources>,
    pub state: ResourceApplyState,
    pub logical_cpu_count: Option<u16>,
    pub physical_memory_mib: Option<u64>,
    pub minimum_memory_mib: u64,
    pub maximum_memory_mib: u64,
    pub minimum_threads: u16,
    pub maximum_threads: u16,
}

pub struct EngineResourceManager {
    database: MetadataDb,
    engine: Arc<EngineManager>,
    queries: Arc<QueryCoordinator>,
    exports: Arc<ExportCoordinator>,
    requested: std::sync::Mutex<EngineResourceSettings>,
}

impl EngineResourceManager {
    pub fn new(
        database: MetadataDb,
        engine: Arc<EngineManager>,
        queries: Arc<QueryCoordinator>,
        exports: Arc<ExportCoordinator>,
    ) -> Self {
        let requested = load_requested(&database);
        let _ = engine.set_requested_resources(requested.clone());
        Self {
            database,
            engine,
            queries,
            exports,
            requested: std::sync::Mutex::new(requested),
        }
    }

    pub fn status(&self) -> Result<EngineResourceStatus, String> {
        let requested = self
            .requested
            .lock()
            .map_err(|_| "engine resource state lock".to_string())?
            .clone();
        let connected = self.engine.status().state == "connected";
        let effective = connected
            .then(|| self.engine.effective_resources())
            .flatten();
        Ok(status_with_environment(requested, effective))
    }

    pub fn apply(
        &self,
        requested: EngineResourceSettings,
        has_active_project: bool,
    ) -> Result<EngineResourceStatus, String> {
        requested
            .validate()
            .map_err(|error| format!("resources.invalid: {error}"))?;
        if self.queries.has_active() || self.exports.has_active() {
            return Err(
                "resources.busy: Finish or cancel the active query or export before applying settings."
                    .into(),
            );
        }

        let effective = if has_active_project && self.engine.status().state == "connected" {
            Some(self.engine.configure_resources(requested.clone())?)
        } else {
            self.engine.set_requested_resources(requested.clone())?;
            None
        };

        let persistence = SettingsRepository::new(self.database.clone())
            .set(RESOURCE_SETTINGS_KEY, &requested)
            .map_err(|error| format!("resources.persistence: {error}"));
        *self
            .requested
            .lock()
            .map_err(|_| "engine resource state lock".to_string())? = requested.clone();
        persistence?;
        Ok(status_with_environment(requested, effective))
    }
}

fn load_requested(database: &MetadataDb) -> EngineResourceSettings {
    SettingsRepository::new(database.clone())
        .get::<EngineResourceSettings>(RESOURCE_SETTINGS_KEY)
        .ok()
        .flatten()
        .filter(|settings| settings.validate().is_ok())
        .unwrap_or_default()
}

fn status_with_environment(
    requested: EngineResourceSettings,
    effective: Option<EffectiveEngineResources>,
) -> EngineResourceStatus {
    EngineResourceStatus {
        requested,
        state: if effective.is_some() {
            ResourceApplyState::Effective
        } else {
            ResourceApplyState::Pending
        },
        effective,
        logical_cpu_count: std::thread::available_parallelism()
            .ok()
            .and_then(|count| u16::try_from(count.get()).ok()),
        physical_memory_mib: detected_physical_memory_mib(),
        minimum_memory_mib: MIN_ENGINE_MEMORY_MIB,
        maximum_memory_mib: MAX_ENGINE_MEMORY_MIB,
        minimum_threads: MIN_ENGINE_THREADS,
        maximum_threads: MAX_ENGINE_THREADS,
    }
}

fn detected_physical_memory_mib() -> Option<u64> {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    let bytes = system.total_memory();
    (bytes > 0).then_some(bytes / 1024 / 1024)
}

#[tauri::command]
pub fn get_engine_resources(
    manager: State<'_, Arc<EngineResourceManager>>,
) -> Result<EngineResourceStatus, String> {
    manager.status()
}

#[tauri::command]
pub fn set_engine_resources(
    requested: EngineResourceSettings,
    manager: State<'_, Arc<EngineResourceManager>>,
    projects: State<'_, ProjectManager>,
) -> Result<EngineResourceStatus, String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .is_some();
    manager.apply(requested, active)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarik_engine_protocol::EngineResourcePreset;

    #[test]
    fn invalid_or_missing_persisted_settings_fall_back_to_balanced() {
        let database = MetadataDb::open_in_memory().unwrap();
        assert_eq!(load_requested(&database), EngineResourceSettings::default());
        SettingsRepository::new(database.clone())
            .set(
                RESOURCE_SETTINGS_KEY,
                &EngineResourceSettings {
                    preset: EngineResourcePreset::Custom,
                    memory_limit_mib: 1,
                    threads: 0,
                },
            )
            .unwrap();
        assert_eq!(load_requested(&database), EngineResourceSettings::default());
    }

    #[test]
    fn status_never_fabricates_effective_values() {
        let requested = EngineResourceSettings::default();
        let status = status_with_environment(requested.clone(), None);
        assert_eq!(status.requested, requested);
        assert_eq!(status.state, ResourceApplyState::Pending);
        assert_eq!(status.effective, None);
    }
}

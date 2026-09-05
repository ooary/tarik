use std::sync::{Arc, Mutex};

use tarik_engine_protocol::{ProfileRequest, ProfileState, ProfileStatus};
use tauri::State;

use crate::{engine_manager::EngineManager, projects::ProjectManager};

pub struct ProfileCoordinator {
    engine: Arc<EngineManager>,
    active: Mutex<Vec<String>>,
}

impl ProfileCoordinator {
    pub fn new(engine: Arc<EngineManager>) -> Self {
        Self {
            engine,
            active: Mutex::new(Vec::new()),
        }
    }

    pub fn execute(&self, request: ProfileRequest) -> Result<ProfileStatus, String> {
        request
            .validate()
            .map_err(|error| format!("profile.invalid: {error}"))?;
        let profile_id = uuid::Uuid::new_v4().to_string();
        self.engine.execute_profile(&profile_id, &request)?;
        self.active
            .lock()
            .map_err(|_| "profile registry lock".to_string())?
            .push(profile_id.clone());
        Ok(ProfileStatus {
            profile_id,
            state: ProfileState::Queued,
            duration_ms: 0,
            snapshot: None,
            error: None,
        })
    }

    pub fn status(&self, profile_id: &str) -> Result<Option<ProfileStatus>, String> {
        let status = self.engine.profile_status(profile_id)?;
        if status.as_ref().is_some_and(|status| {
            matches!(
                status.state,
                ProfileState::Succeeded | ProfileState::Failed | ProfileState::Cancelled
            )
        }) {
            self.remove_active(profile_id);
        }
        Ok(status)
    }

    pub fn cancel(&self, profile_id: &str) -> Result<Option<ProfileStatus>, String> {
        let status = self.engine.cancel_profile(profile_id)?;
        if status.as_ref().is_some_and(|status| {
            matches!(
                status.state,
                ProfileState::Succeeded | ProfileState::Failed | ProfileState::Cancelled
            )
        }) {
            self.remove_active(profile_id);
        }
        Ok(status)
    }

    pub fn cancel_all(&self) -> u64 {
        let ids = self
            .active
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        for id in &ids {
            let _ = self.engine.cancel_profile(id);
        }
        ids.len() as u64
    }

    pub fn has_active(&self) -> bool {
        let ids = self
            .active
            .lock()
            .map(|ids| ids.clone())
            .unwrap_or_default();
        let mut any_active = false;
        for id in &ids {
            match self.engine.profile_status(id) {
                Ok(Some(status))
                    if matches!(status.state, ProfileState::Queued | ProfileState::Running) =>
                {
                    any_active = true;
                }
                Ok(_) => self.remove_active(id),
                Err(_) => any_active = true,
            }
        }
        any_active
    }

    fn remove_active(&self, profile_id: &str) {
        if let Ok(mut ids) = self.active.lock() {
            ids.retain(|id| id != profile_id);
        }
    }
}

#[tauri::command]
pub fn execute_profile(
    request: ProfileRequest,
    profiles: State<'_, Arc<ProfileCoordinator>>,
    projects: State<'_, ProjectManager>,
) -> Result<ProfileStatus, String> {
    let active = projects
        .active()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "profile.no_active_project".to_string())?;
    if active.id != request.project_id {
        return Err(format!(
            "profile.project_mismatch: active project is {}, request was {}",
            active.id, request.project_id
        ));
    }
    profiles.execute(request)
}

#[tauri::command]
pub fn get_profile_status(
    profile_id: String,
    profiles: State<'_, Arc<ProfileCoordinator>>,
) -> Result<Option<ProfileStatus>, String> {
    profiles.status(&profile_id)
}

#[tauri::command]
pub fn cancel_profile(
    profile_id: String,
    profiles: State<'_, Arc<ProfileCoordinator>>,
) -> Result<Option<ProfileStatus>, String> {
    profiles.cancel(&profile_id)
}

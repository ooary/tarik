//! Optional installation of Tarik's reviewed Agent Skill.
//!
//! Guidance has no pairing, grant, approval, query, destination, or export
//! authority. The installer supports only reviewed current-user skill locations
//! and never accepts a source or destination path from the frontend.

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::metadata::{settings::SettingsRepository, MetadataDb};

const SKILL_CONTENT: &str = include_str!("../../docs/agent-skills/tarik-mcp/SKILL.md");
const PLAN_LIFETIME: Duration = Duration::from_secs(5 * 60);
const MAX_PLANS: usize = 8;
const RECEIPT_KEY: &str = "agent.guidance.skill_receipt";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillHost {
    Pi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillOperation {
    Install,
    Remove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillState {
    Available,
    Installed,
    Conflict,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillStatus {
    pub host: SkillHost,
    pub host_name: String,
    pub state: SkillState,
    pub detail: String,
    pub can_install: bool,
    pub can_remove: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillPlanView {
    pub plan_id: String,
    pub host: SkillHost,
    pub host_name: String,
    pub operation: SkillOperation,
    pub summary: String,
    pub target: String,
    pub expires_in_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSkillApplyResult {
    pub host: SkillHost,
    pub operation: SkillOperation,
    pub state: SkillState,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillReceipt {
    host: SkillHost,
    content_hash: String,
    installed_at: String,
}

#[derive(Debug)]
struct SkillPlan {
    view: AgentSkillPlanView,
    target: PathBuf,
    expected_before: SkillState,
    created_at: Instant,
}

pub struct AgentGuidanceManager {
    settings: SettingsRepository,
    plans: Mutex<HashMap<String, SkillPlan>>,
    home_override: Option<PathBuf>,
}

impl AgentGuidanceManager {
    pub fn new(database: MetadataDb) -> Self {
        Self {
            settings: SettingsRepository::new(database),
            plans: Mutex::new(HashMap::new()),
            home_override: None,
        }
    }

    #[cfg(test)]
    fn with_home(mut self, home: PathBuf) -> Self {
        self.home_override = Some(home);
        self
    }

    pub fn status(&self) -> AgentSkillStatus {
        match self.pi_target().and_then(|target| {
            let state = inspect_target(&target, self.receipt()?.as_ref())?;
            Ok((target, state))
        }) {
            Ok((_target, state)) => AgentSkillStatus {
                host: SkillHost::Pi,
                host_name: "Pi".into(),
                state,
                detail: match state {
                    SkillState::Available => {
                        "The reviewed Tarik workflow skill can be installed for Pi."
                    }
                    SkillState::Installed => "Tarik's exact reviewed workflow skill is installed.",
                    SkillState::Conflict => {
                        "A different tarik-mcp skill exists. Tarik will not replace it."
                    }
                    SkillState::Unsupported => "Pi's reviewed user skill location is unavailable.",
                }
                .into(),
                can_install: state == SkillState::Available,
                can_remove: state == SkillState::Installed,
            },
            Err(_) => AgentSkillStatus {
                host: SkillHost::Pi,
                host_name: "Pi".into(),
                state: SkillState::Unsupported,
                detail: "Pi's reviewed current-user agent directory was not found or is unsafe. Use MCP prompts or install the packaged skill manually.".into(),
                can_install: false,
                can_remove: false,
            },
        }
    }

    pub fn plan(
        &self,
        host: SkillHost,
        operation: SkillOperation,
    ) -> Result<AgentSkillPlanView, String> {
        let target = match host {
            SkillHost::Pi => self.pi_target()?,
        };
        let state = inspect_target(&target, self.receipt()?.as_ref())?;
        match (operation, state) {
            (SkillOperation::Install, SkillState::Available)
            | (SkillOperation::Remove, SkillState::Installed) => {}
            (SkillOperation::Install, SkillState::Conflict)
            | (SkillOperation::Remove, SkillState::Conflict) => {
                return Err(
                    "agent.guidance_conflict: A foreign tarik-mcp skill will not be changed."
                        .into(),
                )
            }
            (SkillOperation::Install, SkillState::Installed) => {
                return Err(
                    "agent.guidance_already_installed: The reviewed skill is installed.".into(),
                )
            }
            _ => {
                return Err(
                    "agent.guidance_state_changed: Refresh and review the skill state again."
                        .into(),
                )
            }
        }

        let plan_id = uuid::Uuid::new_v4().to_string();
        let view = AgentSkillPlanView {
            plan_id: plan_id.clone(),
            host,
            host_name: "Pi".into(),
            operation,
            summary: match operation {
                SkillOperation::Install => {
                    "Install Tarik's reviewed guidance-only MCP workflow skill for Pi."
                }
                SkillOperation::Remove => {
                    "Remove only Tarik's exact reviewed MCP workflow skill from Pi."
                }
            }
            .into(),
            target: display_target(&target),
            expires_in_seconds: PLAN_LIFETIME.as_secs(),
        };
        let mut plans = self
            .plans
            .lock()
            .map_err(|_| "agent.guidance_state_unavailable".to_string())?;
        plans.retain(|_, plan| plan.created_at.elapsed() < PLAN_LIFETIME);
        if plans.len() >= MAX_PLANS {
            return Err("agent.guidance_plan_limit: Finish an existing guidance plan.".into());
        }
        plans.insert(
            plan_id,
            SkillPlan {
                view: view.clone(),
                target,
                expected_before: state,
                created_at: Instant::now(),
            },
        );
        Ok(view)
    }

    pub fn apply(&self, plan_id: &str) -> Result<AgentSkillApplyResult, String> {
        let plan = self
            .plans
            .lock()
            .map_err(|_| "agent.guidance_state_unavailable".to_string())?
            .remove(plan_id)
            .ok_or_else(|| {
                "agent.guidance_plan_missing: Review the skill change again before applying it."
                    .to_string()
            })?;
        if plan.created_at.elapsed() >= PLAN_LIFETIME {
            return Err("agent.guidance_plan_expired: Review the skill change again.".into());
        }
        let current_target = match plan.view.host {
            SkillHost::Pi => self.pi_target()?,
        };
        if current_target != plan.target {
            return Err("agent.guidance_plan_stale: The reviewed skill target changed.".into());
        }
        let current = inspect_target(&plan.target, self.receipt()?.as_ref())?;
        if current != plan.expected_before {
            return Err("agent.guidance_plan_stale: The skill changed after review.".into());
        }

        match plan.view.operation {
            SkillOperation::Install => {
                install_exact_skill(&plan.target)?;
                let receipt = SkillReceipt {
                    host: plan.view.host,
                    content_hash: skill_hash(),
                    installed_at: chrono::Utc::now().to_rfc3339(),
                };
                if let Err(error) = self.settings.set(RECEIPT_KEY, &receipt) {
                    let _ = remove_exact_skill(&plan.target);
                    return Err(format!(
                        "agent.guidance_receipt_failed: Skill installation was reverted: {error}"
                    ));
                }
                Ok(AgentSkillApplyResult {
                    host: plan.view.host,
                    operation: plan.view.operation,
                    state: SkillState::Installed,
                    message:
                        "Tarik's guidance-only workflow skill was installed. Restart Pi to load it."
                            .into(),
                })
            }
            SkillOperation::Remove => {
                remove_exact_skill(&plan.target)?;
                if let Err(error) = self.settings.delete(RECEIPT_KEY) {
                    return Err(format!(
                        "agent.guidance_recovery_required: The skill was removed but its receipt could not be cleared: {error}"
                    ));
                }
                Ok(AgentSkillApplyResult {
                    host: plan.view.host,
                    operation: plan.view.operation,
                    state: SkillState::Available,
                    message: "Tarik's exact workflow skill was removed. Restart Pi to unload it."
                        .into(),
                })
            }
        }
    }

    fn receipt(&self) -> Result<Option<SkillReceipt>, String> {
        self.settings
            .get(RECEIPT_KEY)
            .map_err(|error| error.to_string())
    }

    fn pi_target(&self) -> Result<PathBuf, String> {
        let home = self
            .home_override
            .clone()
            .or_else(current_home)
            .ok_or_else(|| "agent.guidance_unsupported: User home is unavailable.".to_string())?;
        verify_owned_directory(&home)?;
        let pi_agent = home.join(".pi").join("agent");
        verify_owned_directory(&pi_agent).map_err(|_| {
            "agent.guidance_unsupported: A verified Pi agent directory was not found.".to_string()
        })?;
        Ok(pi_agent.join("skills").join("tarik-mcp").join("SKILL.md"))
    }
}

fn current_home() -> Option<PathBuf> {
    std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
}

fn inspect_target(target: &Path, receipt: Option<&SkillReceipt>) -> Result<SkillState, String> {
    verify_target_ancestors(target)?;
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Ok(SkillState::Conflict);
            }
            verify_owned_path(target, &metadata)?;
            let bytes =
                fs::read(target).map_err(|error| format!("agent.guidance_read_failed: {error}"))?;
            let exact = bytes == SKILL_CONTENT.as_bytes();
            let owned = receipt.is_some_and(|receipt| {
                receipt.host == SkillHost::Pi && receipt.content_hash == skill_hash()
            });
            Ok(if exact && owned {
                SkillState::Installed
            } else {
                SkillState::Conflict
            })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SkillState::Available),
        Err(error) => Err(format!("agent.guidance_read_failed: {error}")),
    }
}

fn install_exact_skill(target: &Path) -> Result<(), String> {
    verify_target_ancestors(target)?;
    if fs::symlink_metadata(target).is_ok() {
        return Err("agent.guidance_collision: The reviewed skill target already exists.".into());
    }
    let directory = target
        .parent()
        .ok_or_else(|| "agent.guidance_unsafe_target".to_string())?;
    let skills = directory
        .parent()
        .ok_or_else(|| "agent.guidance_unsafe_target".to_string())?;
    if !skills.exists() {
        fs::create_dir(skills).map_err(|error| format!("agent.guidance_create_failed: {error}"))?;
    }
    verify_owned_directory(skills)?;
    fs::create_dir(directory).map_err(|error| format!("agent.guidance_create_failed: {error}"))?;
    verify_owned_directory(directory)?;
    let stage = directory.join(format!(".SKILL.md.{}.tmp", uuid::Uuid::new_v4()));
    let mut published = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage)
            .map_err(|error| format!("agent.guidance_write_failed: {error}"))?;
        file.write_all(SKILL_CONTENT.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("agent.guidance_write_failed: {error}"))?;
        fs::hard_link(&stage, target)
            .map_err(|error| format!("agent.guidance_publish_failed: {error}"))?;
        published = true;
        let observed =
            fs::read(target).map_err(|error| format!("agent.guidance_verify_failed: {error}"))?;
        if observed != SKILL_CONTENT.as_bytes() {
            return Err("agent.guidance_verify_failed: Installed skill did not match.".into());
        }
        Ok(())
    })();
    let _ = fs::remove_file(&stage);
    if result.is_err() {
        if published {
            let _ = fs::remove_file(target);
        }
        let _ = fs::remove_dir(directory);
    }
    result
}

fn remove_exact_skill(target: &Path) -> Result<(), String> {
    let bytes = fs::read(target).map_err(|error| format!("agent.guidance_read_failed: {error}"))?;
    if bytes != SKILL_CONTENT.as_bytes() {
        return Err("agent.guidance_conflict: The skill changed and was not removed.".into());
    }
    fs::remove_file(target).map_err(|error| format!("agent.guidance_remove_failed: {error}"))?;
    if let Some(directory) = target.parent() {
        let _ = fs::remove_dir(directory);
    }
    Ok(())
}

fn verify_target_ancestors(target: &Path) -> Result<(), String> {
    let skill_dir = target
        .parent()
        .ok_or_else(|| "agent.guidance_unsafe_target".to_string())?;
    let skills_dir = skill_dir
        .parent()
        .ok_or_else(|| "agent.guidance_unsafe_target".to_string())?;
    let agent_dir = skills_dir
        .parent()
        .ok_or_else(|| "agent.guidance_unsafe_target".to_string())?;
    verify_owned_directory(agent_dir)?;
    if skills_dir.exists() {
        verify_owned_directory(skills_dir)?;
    }
    if skill_dir.exists() {
        verify_owned_directory(skill_dir)?;
    }
    Ok(())
}

fn verify_owned_directory(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("agent.guidance_unsafe_target: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("agent.guidance_unsafe_target: Expected a regular user directory.".into());
    }
    verify_owned_path(path, &metadata)
}

fn verify_owned_path(_path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(
                "agent.guidance_unsafe_owner: Path is not owned by the current user.".into(),
            );
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("agent.guidance_unsafe_target: Reparse points are not allowed.".into());
        }
        crate::agent_setup::verify_windows_owner(_path)?;
    }
    Ok(())
}

fn display_target(target: &Path) -> String {
    target.to_string_lossy().into_owned()
}

fn skill_hash() -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(SKILL_CONTENT.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[tauri::command]
pub fn get_agent_skill_status(manager: tauri::State<'_, AgentGuidanceManager>) -> AgentSkillStatus {
    manager.status()
}

#[tauri::command]
pub fn plan_agent_skill(
    host: SkillHost,
    operation: SkillOperation,
    manager: tauri::State<'_, AgentGuidanceManager>,
) -> Result<AgentSkillPlanView, String> {
    manager.plan(host, operation)
}

#[tauri::command]
pub fn apply_agent_skill(
    plan_id: String,
    manager: tauri::State<'_, AgentGuidanceManager>,
) -> Result<AgentSkillApplyResult, String> {
    manager.apply(&plan_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager(name: &str) -> (AgentGuidanceManager, PathBuf) {
        let home = std::env::temp_dir().join(format!(
            "tarik-agent-guidance-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(home.join(".pi/agent")).unwrap();
        (
            AgentGuidanceManager::new(MetadataDb::open_in_memory().unwrap())
                .with_home(home.clone()),
            home,
        )
    }

    #[test]
    fn installs_and_removes_only_the_exact_reviewed_skill() {
        let (manager, home) = manager("lifecycle");
        assert_eq!(manager.status().state, SkillState::Available);
        let plan = manager
            .plan(SkillHost::Pi, SkillOperation::Install)
            .unwrap();
        assert!(plan.target.ends_with("tarik-mcp/SKILL.md"));
        let installed = manager.apply(&plan.plan_id).unwrap();
        assert_eq!(installed.state, SkillState::Installed);
        assert_eq!(manager.status().state, SkillState::Installed);
        let target = home.join(".pi/agent/skills/tarik-mcp/SKILL.md");
        assert_eq!(fs::read_to_string(&target).unwrap(), SKILL_CONTENT);

        let plan = manager.plan(SkillHost::Pi, SkillOperation::Remove).unwrap();
        let removed = manager.apply(&plan.plan_id).unwrap();
        assert_eq!(removed.state, SkillState::Available);
        assert!(!target.exists());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn refuses_foreign_content_and_stale_plans() {
        let (manager, home) = manager("conflict");
        let target = home.join(".pi/agent/skills/tarik-mcp/SKILL.md");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "foreign instructions").unwrap();
        assert_eq!(manager.status().state, SkillState::Conflict);
        assert!(manager
            .plan(SkillHost::Pi, SkillOperation::Install)
            .is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "foreign instructions");

        fs::remove_file(&target).unwrap();
        fs::remove_dir(target.parent().unwrap()).unwrap();
        let plan = manager
            .plan(SkillHost::Pi, SkillOperation::Install)
            .unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&target, "raced content").unwrap();
        assert!(manager.apply(&plan.plan_id).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "raced content");
        fs::remove_dir_all(home).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn refuses_symlinked_skill_locations() {
        use std::os::unix::fs::symlink;
        let (manager, home) = manager("symlink");
        let foreign = home.join("foreign");
        fs::create_dir_all(&foreign).unwrap();
        symlink(&foreign, home.join(".pi/agent/skills")).unwrap();
        assert_eq!(manager.status().state, SkillState::Unsupported);
        fs::remove_dir_all(home).unwrap();
    }
}

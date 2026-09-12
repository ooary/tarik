//! Private export-destination grants for authenticated local MCP clients.
//!
//! Absolute paths are accepted only from direct desktop folder selection and
//! remain inside this module and SQLite. MCP receives only redacted views.

use std::{
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tarik_agent_protocol::{ExportDestinationList, ExportDestinationView, ExportFormat};

use crate::{
    metadata::{
        agent::AgentClientState,
        agent::AgentRepository,
        agent_destinations::{
            AgentDestinationRepository, DestinationPolicy, ExportDestinationRecord,
        },
        projects::{ProjectOwnership, ProjectsRepository},
        MetadataDb,
    },
    projects::ProjectManager,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DestinationPolicyInput {
    pub display_label: String,
    pub allow_csv: bool,
    pub allow_parquet: bool,
    pub maximum_rows_per_part: u64,
    pub maximum_total_bytes: u64,
}

impl From<DestinationPolicyInput> for DestinationPolicy {
    fn from(value: DestinationPolicyInput) -> Self {
        Self {
            display_label: value.display_label,
            allow_csv: value.allow_csv,
            allow_parquet: value.allow_parquet,
            maximum_rows_per_part: value.maximum_rows_per_part,
            maximum_total_bytes: value.maximum_total_bytes,
        }
    }
}

#[derive(Clone)]
pub struct AgentDestinationManager {
    database: MetadataDb,
    repository: AgentDestinationRepository,
    projects: ProjectManager,
    protected_roots: Vec<PathBuf>,
}

impl AgentDestinationManager {
    pub fn new(
        database: MetadataDb,
        projects: ProjectManager,
        protected_roots: Vec<PathBuf>,
    ) -> Self {
        Self {
            repository: AgentDestinationRepository::new(database.clone()),
            database,
            projects,
            protected_roots,
        }
    }

    pub fn create(
        &self,
        client_id: &str,
        project_id: &str,
        selected_directory: &str,
        policy: DestinationPolicyInput,
    ) -> Result<ExportDestinationView, String> {
        self.require_direct_owner(client_id, project_id)?;
        let identity = self.inspect_selected_directory(Path::new(selected_directory))?;
        let record = self
            .repository
            .create(
                client_id,
                project_id,
                &identity.path.to_string_lossy(),
                &identity.identity,
                &policy.into(),
            )
            .map_err(metadata_error)?;
        Ok(redacted(&record, true))
    }

    pub fn list_for_desktop(
        &self,
        client_id: &str,
        project_id: &str,
    ) -> Result<ExportDestinationList, String> {
        self.require_direct_owner(client_id, project_id)?;
        self.list_owner(client_id, project_id)
    }

    pub fn update_policy(
        &self,
        client_id: &str,
        project_id: &str,
        destination_id: &str,
        policy: DestinationPolicyInput,
    ) -> Result<ExportDestinationView, String> {
        self.require_direct_owner(client_id, project_id)?;
        let record = self
            .repository
            .update_policy(destination_id, client_id, project_id, &policy.into())
            .map_err(metadata_error)?
            .ok_or_else(destination_missing)?;
        let ready = self.revalidate_record(&record).is_ok();
        Ok(redacted(&record, ready))
    }

    pub fn set_enabled(
        &self,
        client_id: &str,
        project_id: &str,
        destination_id: &str,
        enabled: bool,
    ) -> Result<ExportDestinationView, String> {
        self.require_direct_owner(client_id, project_id)?;
        if enabled {
            let record = self
                .repository
                .find(destination_id)
                .map_err(metadata_error)?
                .filter(|record| record.client_id == client_id && record.project_id == project_id)
                .ok_or_else(destination_missing)?;
            self.revalidate_record(&record)?;
        }
        let record = self
            .repository
            .set_enabled(destination_id, client_id, project_id, enabled)
            .map_err(metadata_error)?
            .ok_or_else(destination_missing)?;
        let ready = self.revalidate_record(&record).is_ok();
        Ok(redacted(&record, ready))
    }

    pub fn repair(
        &self,
        client_id: &str,
        project_id: &str,
        destination_id: &str,
        selected_directory: &str,
    ) -> Result<ExportDestinationView, String> {
        self.require_direct_owner(client_id, project_id)?;
        let current = self
            .repository
            .find(destination_id)
            .map_err(metadata_error)?
            .filter(|record| record.client_id == client_id && record.project_id == project_id)
            .ok_or_else(destination_missing)?;
        let identity = self.inspect_selected_directory(Path::new(selected_directory))?;
        if current.canonical_path == identity.path.to_string_lossy()
            && current.directory_identity == identity.identity
        {
            return Err("agent.destination_unchanged: Select a different valid local folder only when the original destination moved.".into());
        }
        let record = self
            .repository
            .replace_directory(
                destination_id,
                client_id,
                project_id,
                &identity.path.to_string_lossy(),
                &identity.identity,
            )
            .map_err(metadata_error)?
            .ok_or_else(destination_missing)?;
        Ok(redacted(&record, true))
    }

    pub fn revoke(
        &self,
        client_id: &str,
        project_id: &str,
        destination_id: &str,
    ) -> Result<bool, String> {
        self.require_direct_owner(client_id, project_id)?;
        self.repository
            .revoke(destination_id, client_id, project_id)
            .map_err(metadata_error)
    }

    pub(crate) fn resolve_for_export(
        &self,
        client_id: &str,
        project_id: &str,
        destination_id: &str,
    ) -> Result<ExportDestinationRecord, String> {
        let record = self
            .repository
            .find(destination_id)
            .map_err(metadata_error)?
            .filter(|record| record.client_id == client_id && record.project_id == project_id)
            .ok_or_else(destination_missing)?;
        if !record.enabled {
            return Err(
                "agent.destination_disabled: Enable this destination in Tarik before exporting."
                    .into(),
            );
        }
        self.revalidate_record(&record)?;
        Ok(record)
    }

    /// MCP listing: caller supplies identity only after AgentAccessManager has
    /// authenticated the connection and revalidated Analyze for this project.
    pub fn list_for_agent(
        &self,
        client_id: &str,
        project_id: &str,
    ) -> Result<ExportDestinationList, String> {
        self.list_owner(client_id, project_id)
    }

    fn list_owner(
        &self,
        client_id: &str,
        project_id: &str,
    ) -> Result<ExportDestinationList, String> {
        let records = self
            .repository
            .list_for_owner(client_id, project_id)
            .map_err(metadata_error)?;
        let destinations = records
            .iter()
            .map(|record| {
                let ready = self.revalidate_record(record).is_ok();
                redacted(record, ready)
            })
            .collect();
        Ok(ExportDestinationList {
            project_id: project_id.to_string(),
            destinations,
        })
    }

    fn require_direct_owner(&self, client_id: &str, project_id: &str) -> Result<(), String> {
        let active = self
            .projects
            .active()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "agent.project_closed: Open the project in Tarik first.".to_string())?;
        if active.id != project_id {
            return Err(
                "agent.project_closed: Only the active project can receive an export destination."
                    .into(),
            );
        }
        let client = AgentRepository::new(self.database.clone())
            .find_client(client_id)
            .map_err(metadata_error)?
            .filter(|client| client.state == AgentClientState::Paired)
            .ok_or_else(|| "agent.client_missing: Select a paired client.".to_string())?;
        let analyze = AgentRepository::new(self.database.clone())
            .list_grants(&client.id)
            .map_err(metadata_error)?
            .into_iter()
            .any(|grant| grant.project_id == project_id && grant.analyze);
        if !analyze {
            return Err(
                "agent.permission_denied: Grant Analyze before creating an export destination."
                    .into(),
            );
        }
        Ok(())
    }

    fn inspect_selected_directory(&self, candidate: &Path) -> Result<DirectoryIdentity, String> {
        if candidate.as_os_str().is_empty()
            || candidate
                .components()
                .any(|part| matches!(part, Component::ParentDir))
        {
            return Err(destination_unsafe());
        }
        reject_substituted_ancestors(candidate)?;
        let metadata = fs::symlink_metadata(candidate).map_err(|_| {
            "agent.destination_missing: Select an existing local folder.".to_string()
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(destination_unsafe());
        }
        let canonical = fs::canonicalize(candidate).map_err(|_| destination_unsafe())?;
        let canonical_metadata =
            fs::symlink_metadata(&canonical).map_err(|_| destination_unsafe())?;
        if canonical_metadata.file_type().is_symlink() || !canonical_metadata.is_dir() {
            return Err(destination_unsafe());
        }
        if canonical.parent().is_none() {
            return Err("agent.destination_unsafe: Filesystem roots cannot be delegated.".into());
        }
        verify_user_owned(&canonical, &canonical_metadata)?;
        reject_remote_filesystem(&canonical)?;
        self.reject_protected_overlap(&canonical)?;
        Ok(DirectoryIdentity {
            identity: directory_identity(&canonical, &canonical_metadata)?,
            path: canonical,
        })
    }

    fn revalidate_record(&self, record: &ExportDestinationRecord) -> Result<(), String> {
        let identity = self.inspect_selected_directory(Path::new(&record.canonical_path))?;
        if identity.identity != record.directory_identity
            || identity.path.to_string_lossy() != record.canonical_path
        {
            return Err(
                "agent.destination_moved: Repair this destination in Tarik before exporting."
                    .into(),
            );
        }
        Ok(())
    }

    fn reject_protected_overlap(&self, candidate: &Path) -> Result<(), String> {
        let mut protected = self.protected_roots.clone();
        for project in ProjectsRepository::new(self.database.clone())
            .list()
            .map_err(metadata_error)?
        {
            if project.ownership == ProjectOwnership::Managed {
                let path = PathBuf::from(project.duckdb_path);
                if let Some(parent) = path.parent() {
                    protected.push(parent.to_path_buf());
                }
            }
        }
        for path in protected {
            let canonical = fs::canonicalize(&path).unwrap_or(path);
            if candidate.starts_with(&canonical) || canonical.starts_with(candidate) {
                return Err("agent.destination_unsafe: Choose a folder outside Tarik data, cache, logs, and project storage.".into());
            }
        }
        Ok(())
    }
}

struct DirectoryIdentity {
    path: PathBuf,
    identity: String,
}

fn redacted(record: &ExportDestinationRecord, ready: bool) -> ExportDestinationView {
    let mut formats = Vec::new();
    if record.allow_csv {
        formats.push(ExportFormat::Csv);
    }
    if record.allow_parquet {
        formats.push(ExportFormat::Parquet);
    }
    ExportDestinationView {
        destination_id: record.id.clone(),
        label: record.display_label.clone(),
        formats,
        maximum_rows_per_part: record.maximum_rows_per_part,
        maximum_total_bytes: record.maximum_total_bytes,
        create_new_only: true,
        enabled: record.enabled,
        ready,
        revision: record.revision,
    }
}

fn metadata_error(error: impl std::fmt::Display) -> String {
    format!("agent.destination_store: {error}")
}

fn destination_missing() -> String {
    "agent.destination_missing: The destination does not exist for this client and project.".into()
}

fn destination_unsafe() -> String {
    "agent.destination_unsafe: Select a regular local user-owned folder.".into()
}

fn reject_substituted_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(|_| destination_unsafe())?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(
                "agent.destination_unsafe: Symlinked or substituted folders cannot be delegated."
                    .into(),
            );
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(
                    "agent.destination_unsafe: Reparse-point folders cannot be delegated.".into(),
                );
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn verify_user_owned(_path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    use std::os::unix::fs::MetadataExt;
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(
            "agent.destination_unsafe: The folder is not owned by the current user.".into(),
        );
    }
    Ok(())
}

#[cfg(windows)]
fn verify_user_owned(path: &Path, metadata: &fs::Metadata) -> Result<(), String> {
    use std::os::windows::fs::MetadataExt;
    if metadata.file_attributes() & 0x400 != 0 {
        return Err("agent.destination_unsafe: Reparse-point folders are not allowed.".into());
    }
    crate::agent_setup::verify_windows_owner(path).map_err(|_| {
        "agent.destination_unsafe: The folder is not owned by the current user.".into()
    })
}

#[cfg(unix)]
fn directory_identity(_path: &Path, metadata: &fs::Metadata) -> Result<String, String> {
    use std::os::unix::fs::MetadataExt;
    Ok(format!("unix:{}:{}", metadata.dev(), metadata.ino()))
}

#[cfg(windows)]
fn directory_identity(path: &Path, _metadata: &fs::Metadata) -> Result<String, String> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{
            CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
            OPEN_EXISTING,
        },
    };

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(destination_unsafe());
    }

    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    let result = unsafe { GetFileInformationByHandle(handle, &mut information) };
    unsafe {
        CloseHandle(handle);
    }
    if result == 0 {
        return Err(destination_unsafe());
    }

    let file_index =
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow);
    Ok(format!(
        "windows:{}:{}",
        information.dwVolumeSerialNumber, file_index
    ))
}

#[cfg(target_os = "linux")]
fn reject_remote_filesystem(path: &Path) -> Result<(), String> {
    let mountinfo = fs::read_to_string("/proc/self/mountinfo").map_err(|_| {
        "agent.destination_unsafe: The local filesystem class could not be verified.".to_string()
    })?;
    let path_text = path.to_string_lossy();
    let mut best: Option<(usize, &str)> = None;
    for line in mountinfo.lines() {
        let Some((before, after)) = line.split_once(" - ") else {
            continue;
        };
        let fields = before.split_whitespace().collect::<Vec<_>>();
        let Some(mount) = fields.get(4) else {
            continue;
        };
        let mount = mount.replace("\\040", " ").replace("\\134", "\\");
        let contains_path = mount == "/"
            || (path_text.starts_with(&mount)
                && path_text
                    .as_bytes()
                    .get(mount.len())
                    .is_none_or(|byte| *byte == b'/'));
        if contains_path && best.is_none_or(|(length, _)| mount.len() > length) {
            let fs_type = after.split_whitespace().next().unwrap_or_default();
            best = Some((mount.len(), fs_type));
        }
    }
    let fs_type = best.map(|(_, fs_type)| fs_type).ok_or_else(|| {
        "agent.destination_unsafe: The local filesystem class could not be verified.".to_string()
    })?;
    if matches!(
        fs_type,
        "nfs"
            | "nfs4"
            | "cifs"
            | "smb3"
            | "9p"
            | "afs"
            | "ceph"
            | "glusterfs"
            | "sshfs"
            | "fuse.sshfs"
    ) {
        return Err(
            "agent.destination_unsafe: Network and remote filesystems cannot be delegated.".into(),
        );
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn reject_remote_filesystem(_path: &Path) -> Result<(), String> {
    Err("agent.destination_unsafe: Delegated exports require a reviewed local filesystem policy on this platform.".into())
}

#[cfg(windows)]
fn reject_remote_filesystem(path: &Path) -> Result<(), String> {
    use std::path::Prefix;
    use windows_sys::Win32::{
        Storage::FileSystem::GetDriveTypeW, System::WindowsProgramming::DRIVE_REMOTE,
    };

    let drive = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive,
            Prefix::UNC(..) | Prefix::VerbatimUNC(..) => {
                return Err(
                    "agent.destination_unsafe: Network folders cannot be delegated.".into(),
                );
            }
            _ => return Err(destination_unsafe()),
        },
        _ => return Err(destination_unsafe()),
    };
    let wide = [u16::from(drive), ':' as u16, '\\' as u16, 0];
    if unsafe { GetDriveTypeW(wide.as_ptr()) } == DRIVE_REMOTE {
        return Err("agent.destination_unsafe: Network drives cannot be delegated.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn list_agent_export_destinations(
    client_id: String,
    project_id: String,
    manager: tauri::State<'_, Arc<AgentDestinationManager>>,
) -> Result<ExportDestinationList, String> {
    manager.list_for_desktop(&client_id, &project_id)
}

#[tauri::command]
pub fn create_agent_export_destination(
    client_id: String,
    project_id: String,
    selected_directory: String,
    policy: DestinationPolicyInput,
    manager: tauri::State<'_, Arc<AgentDestinationManager>>,
) -> Result<ExportDestinationView, String> {
    manager.create(&client_id, &project_id, &selected_directory, policy)
}

#[tauri::command]
pub fn update_agent_export_destination(
    client_id: String,
    project_id: String,
    destination_id: String,
    policy: DestinationPolicyInput,
    manager: tauri::State<'_, Arc<AgentDestinationManager>>,
    exports: tauri::State<'_, Arc<crate::agent_exports::AgentExportManager>>,
) -> Result<ExportDestinationView, String> {
    let updated = manager.update_policy(&client_id, &project_id, &destination_id, policy)?;
    exports.invalidate_destination(&destination_id);
    Ok(updated)
}

#[tauri::command]
pub fn set_agent_export_destination_enabled(
    client_id: String,
    project_id: String,
    destination_id: String,
    enabled: bool,
    manager: tauri::State<'_, Arc<AgentDestinationManager>>,
    exports: tauri::State<'_, Arc<crate::agent_exports::AgentExportManager>>,
) -> Result<ExportDestinationView, String> {
    let updated = manager.set_enabled(&client_id, &project_id, &destination_id, enabled)?;
    exports.invalidate_destination(&destination_id);
    Ok(updated)
}

#[tauri::command]
pub fn repair_agent_export_destination(
    client_id: String,
    project_id: String,
    destination_id: String,
    selected_directory: String,
    manager: tauri::State<'_, Arc<AgentDestinationManager>>,
    exports: tauri::State<'_, Arc<crate::agent_exports::AgentExportManager>>,
) -> Result<ExportDestinationView, String> {
    let updated = manager.repair(
        &client_id,
        &project_id,
        &destination_id,
        &selected_directory,
    )?;
    exports.invalidate_destination(&destination_id);
    Ok(updated)
}

#[tauri::command]
pub fn revoke_agent_export_destination(
    client_id: String,
    project_id: String,
    destination_id: String,
    manager: tauri::State<'_, Arc<AgentDestinationManager>>,
    exports: tauri::State<'_, Arc<crate::agent_exports::AgentExportManager>>,
) -> Result<bool, String> {
    let revoked = manager.revoke(&client_id, &project_id, &destination_id)?;
    if revoked {
        exports.invalidate_destination(&destination_id);
    }
    Ok(revoked)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tarik_agent_protocol::ProjectGrant;

    use crate::{
        engine_manager::EngineManager,
        metadata::{
            agent::AgentRepository,
            agent_destinations::{MAX_DESTINATION_BYTES, MAX_DESTINATION_ROWS_PER_PART},
        },
        projects::ActiveProject,
    };

    use super::*;

    fn fixture(name: &str) -> (AgentDestinationManager, PathBuf, String) {
        fixture_with_ownership(name, ProjectOwnership::External)
    }

    fn fixture_with_ownership(
        name: &str,
        ownership: ProjectOwnership,
    ) -> (AgentDestinationManager, PathBuf, String) {
        let root = std::env::temp_dir().join(format!("tarik-dest-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("projects")).unwrap();
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("cache")).unwrap();
        fs::create_dir_all(root.join("logs")).unwrap();
        let database = MetadataDb::open_in_memory().unwrap();
        let project_store = root.join("project-store");
        fs::create_dir(&project_store).unwrap();
        let project = ProjectsRepository::new(database.clone())
            .upsert("Test", &project_store.join("project.duckdb"), ownership)
            .unwrap();
        let engine = Arc::new(EngineManager::new(
            root.join("missing-engine"),
            root.join("results"),
        ));
        let projects = ProjectManager::new(database.clone(), root.join("projects"), engine);
        projects.set_active_for_test(ActiveProject {
            id: project.id.clone(),
            name: project.name,
            duckdb_path: project.duckdb_path.into(),
        });
        let agents = AgentRepository::new(database.clone());
        agents.pair("client", "Pi", &[1; 32], &[2; 32]).unwrap();
        agents
            .set_grant(
                "client",
                &ProjectGrant {
                    project_id: project.id.clone(),
                    inspect: true,
                    analyze: true,
                    modify_workspace: false,
                    modify_data: false,
                },
            )
            .unwrap();
        let manager = AgentDestinationManager::new(
            database,
            projects,
            vec![
                root.join("data"),
                root.join("cache"),
                root.join("logs"),
                root.join("projects"),
            ],
        );
        (manager, root, project.id)
    }

    fn policy() -> DestinationPolicyInput {
        DestinationPolicyInput {
            display_label: "Daily exports".into(),
            allow_csv: true,
            allow_parquet: true,
            maximum_rows_per_part: MAX_DESTINATION_ROWS_PER_PART,
            maximum_total_bytes: MAX_DESTINATION_BYTES,
        }
    }

    #[test]
    fn creates_redacted_owner_bound_destination_and_revalidates_identity() {
        let (manager, root, project_id) = fixture("create");
        let output = root.join("output");
        fs::create_dir(&output).unwrap();
        let view = manager
            .create("client", &project_id, output.to_str().unwrap(), policy())
            .unwrap();
        assert!(view.ready);
        assert!(view.create_new_only);
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains(output.to_str().unwrap()));
        assert!(manager
            .list_for_agent("other", &project_id)
            .unwrap()
            .destinations
            .is_empty());
        fs::remove_dir(&output).unwrap();
        let listed = manager.list_for_agent("client", &project_id).unwrap();
        assert!(!listed.destinations[0].ready);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn external_project_parent_is_a_valid_custom_destination() {
        let (manager, root, project_id) = fixture("external-parent");
        let shared_directory = root.join("project-store");
        let view = manager
            .create(
                "client",
                &project_id,
                shared_directory.to_str().unwrap(),
                policy(),
            )
            .unwrap();
        assert!(view.ready);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_project_directory_remains_protected() {
        let (manager, root, project_id) =
            fixture_with_ownership("managed-project", ProjectOwnership::Managed);
        let project_directory = root.join("project-store");
        assert!(manager
            .create(
                "client",
                &project_id,
                project_directory.to_str().unwrap(),
                policy(),
            )
            .is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_root_symlink_and_protected_or_project_overlap() {
        let (manager, root, project_id) = fixture("unsafe");
        assert!(manager
            .create("client", &project_id, "/", policy())
            .is_err());
        assert!(manager
            .create(
                "client",
                &project_id,
                root.join("data").to_str().unwrap(),
                policy()
            )
            .is_err());
        assert!(manager
            .create("client", &project_id, root.to_str().unwrap(), policy())
            .is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let target = root.join("target");
            fs::create_dir(&target).unwrap();
            let link = root.join("link");
            symlink(&target, &link).unwrap();
            assert!(manager
                .create("client", &project_id, link.to_str().unwrap(), policy())
                .is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }
}

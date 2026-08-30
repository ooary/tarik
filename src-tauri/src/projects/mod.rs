pub mod commands;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    engine::{DuckDbWorker, EngineProfile, EngineProfileName, ProjectCatalog},
    metadata::{
        projects::{ProjectOwnership, ProjectsRepository, RecentProject},
        MetadataDb,
    },
    sources::{operations::SourceMutationResult, CsvOptions, ImportOptions, SourceInspection},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveProject {
    pub id: String,
    pub name: String,
    pub duckdb_path: PathBuf,
}

struct OpenProject {
    info: ActiveProject,
    worker: DuckDbWorker,
}

#[derive(Clone)]
pub struct ProjectManager {
    metadata: MetadataDb,
    projects_root: PathBuf,
    temp_root: PathBuf,
    active: Arc<Mutex<Option<OpenProject>>>,
}

impl ProjectManager {
    pub fn new(metadata: MetadataDb, projects_root: PathBuf, temp_root: PathBuf) -> Self {
        Self {
            metadata,
            projects_root,
            temp_root,
            active: Arc::new(Mutex::new(None)),
        }
    }

    pub fn create(&self, name: &str) -> Result<ActiveProject, ProjectError> {
        let name = validate_name(name)?;
        let project_directory = self.projects_root.join(Uuid::new_v4().to_string());
        fs::create_dir_all(&project_directory).map_err(|source| ProjectError::CreateDirectory {
            path: project_directory.clone(),
            source,
        })?;
        let database_path = project_directory.join(format!("{}.duckdb", project_file_stem(name)));
        match self.open_path(name, &database_path, ProjectOwnership::Managed) {
            Ok(project) => Ok(project),
            Err(error) => {
                let _ = fs::remove_dir_all(project_directory);
                Err(error)
            }
        }
    }

    pub fn open(&self, name: &str, path: &Path) -> Result<ActiveProject, ProjectError> {
        if !path.is_file() {
            return Err(ProjectError::NotAFile(path.to_path_buf()));
        }
        self.open_path(validate_name(name)?, path, ProjectOwnership::External)
    }

    pub fn reopen(&self, project_id: &str) -> Result<ActiveProject, ProjectError> {
        let recent = ProjectsRepository::new(self.metadata.clone())
            .find(project_id)?
            .ok_or_else(|| ProjectError::UnknownProject(project_id.to_owned()))?;
        self.open_path(
            &recent.name,
            Path::new(&recent.duckdb_path),
            recent.ownership,
        )
    }

    fn open_path(
        &self,
        name: &str,
        path: &Path,
        ownership: ProjectOwnership,
    ) -> Result<ActiveProject, ProjectError> {
        let mut active = self.lock()?;
        if active.is_some() {
            return Err(ProjectError::AlreadyOpen);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| ProjectError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::create_dir_all(&self.temp_root).map_err(|source| ProjectError::CreateDirectory {
            path: self.temp_root.clone(),
            source,
        })?;

        let worker = DuckDbWorker::start(path.to_path_buf())?;
        worker.apply_profile(EngineProfile::preset(
            EngineProfileName::Balanced,
            &self.temp_root,
        ))?;
        let recent =
            ProjectsRepository::new(self.metadata.clone()).upsert(name, path, ownership)?;
        self.refresh_source_health(&recent.id)?;
        let info = ActiveProject {
            id: recent.id,
            name: recent.name,
            duckdb_path: recent.duckdb_path.into(),
        };
        *active = Some(OpenProject {
            info: info.clone(),
            worker,
        });
        Ok(info)
    }

    pub fn close(&self) -> Result<bool, ProjectError> {
        let project = self.lock()?.take();
        match project {
            Some(project) => {
                project.worker.shutdown()?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    pub fn rename(&self, project_id: &str, new_name: &str) -> Result<RecentProject, ProjectError> {
        let new_name = validate_name(new_name)?;
        let repository = ProjectsRepository::new(self.metadata.clone());
        let project = repository
            .find(project_id)?
            .ok_or_else(|| ProjectError::UnknownProject(project_id.to_owned()))?;
        self.close_if_active(project_id)?;

        match project.ownership {
            ProjectOwnership::External => {
                if !repository.rename_display(project_id, new_name)? {
                    return Err(ProjectError::UnknownProject(project_id.to_owned()));
                }
            }
            ProjectOwnership::Managed => {
                let old_path = PathBuf::from(&project.duckdb_path);
                let parent = self.managed_parent(&old_path)?;
                let new_path = parent.join(format!("{}.duckdb", project_file_stem(new_name)));
                if new_path != old_path && new_path.exists() {
                    return Err(ProjectError::DestinationExists(new_path));
                }
                if new_path != old_path {
                    fs::rename(&old_path, &new_path).map_err(|source| ProjectError::Rename {
                        from: old_path.clone(),
                        to: new_path.clone(),
                        source,
                    })?;
                }
                if let Err(error) = repository.update_name_and_path(project_id, new_name, &new_path)
                {
                    if new_path != old_path {
                        fs::rename(&new_path, &old_path).map_err(|rollback| {
                            ProjectError::RenameRollback {
                                original: error.to_string(),
                                from: new_path,
                                to: old_path,
                                source: rollback,
                            }
                        })?;
                    }
                    return Err(error.into());
                }
            }
        }
        repository
            .find(project_id)?
            .ok_or_else(|| ProjectError::UnknownProject(project_id.to_owned()))
    }

    pub fn remove(&self, project_id: &str) -> Result<ProjectRemoval, ProjectError> {
        let repository = ProjectsRepository::new(self.metadata.clone());
        let project = repository
            .find(project_id)?
            .ok_or_else(|| ProjectError::UnknownProject(project_id.to_owned()))?;
        self.close_if_active(project_id)?;

        match project.ownership {
            ProjectOwnership::External => {
                repository.remove(project_id)?;
                Ok(ProjectRemoval::Forgotten)
            }
            ProjectOwnership::Managed => {
                let path = PathBuf::from(&project.duckdb_path);
                let directory = self.managed_parent(&path)?.to_path_buf();
                let staging_root = self.projects_root.join(".deleting");
                fs::create_dir_all(&staging_root).map_err(|source| {
                    ProjectError::CreateDirectory {
                        path: staging_root.clone(),
                        source,
                    }
                })?;
                let staged = staging_root.join(project_id);
                if staged.exists() {
                    return Err(ProjectError::DestinationExists(staged));
                }
                fs::rename(&directory, &staged).map_err(|source| ProjectError::Rename {
                    from: directory.clone(),
                    to: staged.clone(),
                    source,
                })?;
                if let Err(error) = repository.remove(project_id) {
                    fs::rename(&staged, &directory).map_err(|rollback| {
                        ProjectError::RenameRollback {
                            original: error.to_string(),
                            from: staged,
                            to: directory,
                            source: rollback,
                        }
                    })?;
                    return Err(error.into());
                }
                fs::remove_dir_all(&staged).map_err(|source| ProjectError::DeleteStaged {
                    path: staged,
                    source,
                })?;
                Ok(ProjectRemoval::Deleted)
            }
        }
    }

    fn close_if_active(&self, project_id: &str) -> Result<(), ProjectError> {
        let should_close = self
            .lock()?
            .as_ref()
            .is_some_and(|open| open.info.id == project_id);
        if should_close {
            self.close()?;
        }
        Ok(())
    }

    fn managed_parent<'a>(&self, database_path: &'a Path) -> Result<&'a Path, ProjectError> {
        let parent = database_path
            .parent()
            .ok_or_else(|| ProjectError::UnsafeManagedPath(database_path.to_path_buf()))?;
        let expected_parent = parent.parent();
        if expected_parent != Some(self.projects_root.as_path()) {
            return Err(ProjectError::UnsafeManagedPath(database_path.to_path_buf()));
        }
        Ok(parent)
    }

    pub fn active(&self) -> Result<Option<ActiveProject>, ProjectError> {
        Ok(self.lock()?.as_ref().map(|project| project.info.clone()))
    }

    pub fn apply_profile(&self, profile: EngineProfile) -> Result<(), ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        project.worker.apply_profile(profile)?;
        Ok(())
    }

    pub fn catalog(&self) -> Result<ProjectCatalog, ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        Ok(project.worker.inspect_catalog()?)
    }

    fn refresh_source_health(&self, project_id: &str) -> Result<(), ProjectError> {
        let repository = crate::metadata::sources::SourcesRepository::new(self.metadata.clone());
        for source in repository.list_sources(project_id)? {
            let state = crate::sources::operations::check_link_health(&source);
            if state != source.state {
                repository.set_source_state(&source.id, state)?;
            }
        }
        Ok(())
    }

    pub fn inspect_source(
        &self,
        path: PathBuf,
        csv: Option<CsvOptions>,
    ) -> Result<SourceInspection, ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        Ok(project.worker.inspect_source(path, csv)?)
    }

    pub fn link_parquet(
        &self,
        path: PathBuf,
        view_name: String,
    ) -> Result<SourceMutationResult, ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        Ok(project
            .worker
            .link_parquet(project.info.id.clone(), path, view_name)?)
    }

    pub fn import_table(
        &self,
        path: PathBuf,
        options: ImportOptions,
    ) -> Result<SourceMutationResult, ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        Ok(project
            .worker
            .import_table(project.info.id.clone(), path, options)?)
    }

    pub fn repair_link(
        &self,
        source: crate::metadata::sources::SourceRecord,
        replacement: PathBuf,
    ) -> Result<SourceMutationResult, ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        if source.project_id != project.info.id {
            return Err(ProjectError::SourceProjectMismatch);
        }
        Ok(project.worker.repair_link(source, replacement)?)
    }

    pub fn interrupt(&self) -> Result<bool, ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        Ok(project.worker.interrupt()?)
    }

    pub fn drop_link(
        &self,
        source: crate::metadata::sources::SourceRecord,
    ) -> Result<(), ProjectError> {
        let active = self.lock()?;
        let project = active.as_ref().ok_or(ProjectError::NoActiveProject)?;
        if source.project_id != project.info.id {
            return Err(ProjectError::SourceProjectMismatch);
        }
        project.worker.drop_link(source)?;
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Option<OpenProject>>, ProjectError> {
        self.active.lock().map_err(|_| ProjectError::Lock)
    }
}

fn project_file_stem(name: &str) -> String {
    let mut stem = String::with_capacity(name.len());
    let mut separator = false;
    for character in name.chars() {
        if character.is_ascii_alphanumeric() {
            stem.push(character.to_ascii_lowercase());
            separator = false;
        } else if (character.is_alphanumeric() || character == '_' || character == '-')
            && character != '\0'
        {
            stem.push(character);
            separator = false;
        } else if !separator && !stem.is_empty() {
            stem.push('-');
            separator = true;
        }
    }
    let stem = stem.trim_matches(['-', '.', ' ']).to_owned();
    let upper = stem.to_ascii_uppercase();
    let windows_reserved = matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    if stem.is_empty() || windows_reserved {
        format!("tarik-{stem}").trim_end_matches('-').to_owned()
    } else {
        stem
    }
}

fn validate_name(name: &str) -> Result<&str, ProjectError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ProjectError::InvalidName);
    }
    Ok(name)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectRemoval {
    Deleted,
    Forgotten,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("a DuckDB project is already open")]
    AlreadyOpen,
    #[error("no DuckDB project is open")]
    NoActiveProject,
    #[error("project name cannot be empty")]
    InvalidName,
    #[error("source does not belong to the active project")]
    SourceProjectMismatch,
    #[error("project path is not a file: {0}")]
    NotAFile(PathBuf),
    #[error("recent project was not found: {0}")]
    UnknownProject(String),
    #[error("managed project path is outside the Tarik projects directory: {0}")]
    UnsafeManagedPath(PathBuf),
    #[error("project destination already exists: {0}")]
    DestinationExists(PathBuf),
    #[error("could not rename project from {from} to {to}: {source}")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    #[error("could not roll back project rename from {from} to {to} after {original}: {source}")]
    RenameRollback {
        original: String,
        from: PathBuf,
        to: PathBuf,
        source: std::io::Error,
    },
    #[error(
        "project metadata was removed, but staged files could not be deleted at {path}: {source}"
    )]
    DeleteStaged {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not create project directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("active project lock is unavailable")]
    Lock,
    #[error(transparent)]
    Engine(#[from] crate::engine::EngineError),
    #[error(transparent)]
    Metadata(#[from] crate::metadata::MetadataError),
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use duckdb::Connection;

    use super::*;

    fn fixture() -> (ProjectManager, PathBuf) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tarik-projects-{stamp}"));
        let manager = ProjectManager::new(
            MetadataDb::open_in_memory().unwrap(),
            root.join("projects"),
            root.join("cache"),
        );
        (manager, root)
    }

    #[test]
    fn creates_closes_and_reopens_persistent_project() {
        let (manager, root) = fixture();
        let created = manager.create("Retail Analysis").unwrap();
        assert!(created.duckdb_path.is_file());
        assert_eq!(
            created.duckdb_path.file_name().unwrap(),
            "retail-analysis.duckdb"
        );
        manager.close().unwrap();

        Connection::open(&created.duckdb_path)
            .unwrap()
            .execute_batch("CREATE TABLE persisted(id INTEGER);")
            .unwrap();
        manager.reopen(&created.id).unwrap();
        assert!(manager
            .catalog()
            .unwrap()
            .objects
            .iter()
            .any(|object| object.name == "persisted"));
        manager.close().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_rename_closes_worker_moves_file_and_reopens() {
        let (manager, root) = fixture();
        let project = manager.create("Retail").unwrap();

        let renamed = manager.rename(&project.id, "Finance 2026").unwrap();
        assert_eq!(renamed.ownership, ProjectOwnership::Managed);
        assert_ne!(renamed.duckdb_path, project.duckdb_path);
        assert!(Path::new(&renamed.duckdb_path).is_file());
        assert_eq!(
            Path::new(&renamed.duckdb_path).file_name().unwrap(),
            "finance-2026.duckdb"
        );
        assert!(!project.duckdb_path.exists());
        manager.reopen(&project.id).unwrap();
        manager.close().unwrap();
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn external_rename_and_forget_preserve_user_file() {
        let (manager, root) = fixture();
        fs::create_dir_all(&root).unwrap();
        let external_path = root.join("user-owned.duckdb");
        Connection::open(&external_path).unwrap();
        let external = manager.open("Warehouse", &external_path).unwrap();

        let renamed = manager.rename(&external.id, "Finance Warehouse").unwrap();
        assert_eq!(renamed.ownership, ProjectOwnership::External);
        assert_eq!(renamed.duckdb_path, external_path.to_string_lossy());
        assert!(external_path.is_file());
        assert_eq!(
            manager.remove(&external.id).unwrap(),
            ProjectRemoval::Forgotten
        );
        assert!(external_path.is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_delete_closes_worker_and_cascades_metadata() {
        let (manager, root) = fixture();
        let managed = manager.create("Disposable").unwrap();
        let directory = managed.duckdb_path.parent().unwrap().to_path_buf();

        assert_eq!(
            manager.remove(&managed.id).unwrap(),
            ProjectRemoval::Deleted
        );
        assert!(!directory.exists());
        assert!(ProjectsRepository::new(manager.metadata.clone())
            .find(&managed.id)
            .unwrap()
            .is_none());
        assert!(manager.active().unwrap().is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_rename_rejects_filename_collision_without_changing_metadata() {
        let (manager, root) = fixture();
        let managed = manager.create("Retail").unwrap();
        manager.close().unwrap();
        let collision = managed.duckdb_path.parent().unwrap().join("finance.duckdb");
        Connection::open(&collision).unwrap();

        assert!(matches!(
            manager.rename(&managed.id, "Finance"),
            Err(ProjectError::DestinationExists(_))
        ));
        let current = ProjectsRepository::new(manager.metadata.clone())
            .find(&managed.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.name, "Retail");
        assert!(managed.duckdb_path.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_filename_is_windows_safe() {
        assert_eq!(project_file_stem("Retail Analysis"), "retail-analysis");
        assert_eq!(project_file_stem("Sales: 2026/Q1"), "sales-2026-q1");
        assert_eq!(project_file_stem("CON"), "tarik-con");
        assert_eq!(project_file_stem("  ...  "), "tarik");
        assert_eq!(project_file_stem("Pelanggan_日本語"), "pelanggan_日本語");
    }

    #[test]
    fn rejects_second_active_project_and_invalid_paths() {
        let (manager, root) = fixture();
        manager.create("First").unwrap();
        assert!(matches!(
            manager.create("Second"),
            Err(ProjectError::AlreadyOpen)
        ));
        manager.close().unwrap();
        assert!(matches!(
            manager.open("Missing", &root.join("missing.duckdb")),
            Err(ProjectError::NotAFile(_))
        ));
        assert!(matches!(
            manager.create("  "),
            Err(ProjectError::InvalidName)
        ));
        let _ = fs::remove_dir_all(root);
    }
}

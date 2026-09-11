pub mod commands;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use serde::{Deserialize, Serialize};
use tarik_engine_protocol::{
    CatalogSnapshot, CreateTableDefinition, CsvOptions, ImportOptions, SourceInspection,
    SourceRecord,
};
use uuid::Uuid;

use crate::{
    engine_manager::EngineManager,
    metadata::{
        projects::{ProjectOwnership, ProjectsRepository, RecentProject},
        sources::SourcesRepository,
        MetadataDb,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveProject {
    pub id: String,
    pub name: String,
    pub duckdb_path: PathBuf,
}

#[derive(Clone)]
pub struct ProjectManager {
    metadata: MetadataDb,
    projects_root: PathBuf,
    engine: Arc<EngineManager>,
    active: Arc<Mutex<Option<ActiveProject>>>,
}

impl ProjectManager {
    pub fn new(metadata: MetadataDb, projects_root: PathBuf, engine: Arc<EngineManager>) -> Self {
        Self {
            metadata,
            projects_root,
            engine,
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

        self.engine
            .open_session(path)
            .map_err(ProjectError::Engine)?;
        let recent =
            ProjectsRepository::new(self.metadata.clone()).upsert(name, path, ownership)?;
        self.refresh_source_health(&recent.id)?;
        let info = ActiveProject {
            id: recent.id,
            name: recent.name,
            duckdb_path: recent.duckdb_path.into(),
        };
        *active = Some(info.clone());
        Ok(info)
    }

    pub fn close(&self) -> Result<bool, ProjectError> {
        let project = self.lock()?.take();
        match project {
            Some(_) => {
                self.engine.close_session().map_err(ProjectError::Engine)?;
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
            .is_some_and(|open| open.id == project_id);
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
        Ok(self.lock()?.clone())
    }

    #[cfg(test)]
    pub(crate) fn set_active_for_test(&self, project: ActiveProject) {
        *self.active.lock().expect("project test lock") = Some(project);
    }

    pub fn catalog(&self) -> Result<CatalogSnapshot, ProjectError> {
        self.require_active()?;
        self.engine.catalog().map_err(ProjectError::Engine)
    }

    pub fn create_table(&self, definition: CreateTableDefinition) -> Result<(), ProjectError> {
        self.require_active()?;
        self.engine
            .create_table(&definition)
            .map_err(ProjectError::Engine)
    }

    pub fn inspect_source(
        &self,
        path: PathBuf,
        csv: Option<CsvOptions>,
    ) -> Result<SourceInspection, ProjectError> {
        self.require_active()?;
        self.engine
            .inspect_source(
                path.to_str()
                    .ok_or_else(|| ProjectError::InvalidPath(path.clone()))?,
                csv,
            )
            .map_err(ProjectError::Engine)
    }

    pub fn link_parquet(
        &self,
        path: PathBuf,
        view_name: String,
    ) -> Result<SourceRecord, ProjectError> {
        let active = self.require_active()?;
        self.engine
            .link_parquet(
                &active.id,
                path.to_str()
                    .ok_or_else(|| ProjectError::InvalidPath(path.clone()))?,
                &view_name,
            )
            .map_err(ProjectError::Engine)
    }

    pub fn import_table(
        &self,
        path: PathBuf,
        options: ImportOptions,
    ) -> Result<SourceRecord, ProjectError> {
        let active = self.require_active()?;
        self.engine
            .import_table(
                &active.id,
                path.to_str()
                    .ok_or_else(|| ProjectError::InvalidPath(path.clone()))?,
                options,
            )
            .map_err(ProjectError::Engine)
    }

    pub fn repair_link(
        &self,
        source: SourceRecord,
        replacement: PathBuf,
    ) -> Result<SourceRecord, ProjectError> {
        let active = self.require_active()?;
        if source.project_id != active.id {
            return Err(ProjectError::SourceProjectMismatch);
        }
        self.engine
            .repair_link(
                &source,
                replacement
                    .to_str()
                    .ok_or_else(|| ProjectError::InvalidPath(replacement.clone()))?,
            )
            .map_err(ProjectError::Engine)
    }

    pub fn drop_link(&self, source: SourceRecord) -> Result<(), ProjectError> {
        let active = self.require_active()?;
        if source.project_id != active.id {
            return Err(ProjectError::SourceProjectMismatch);
        }
        self.engine.drop_link(&source).map_err(ProjectError::Engine)
    }

    pub fn drop_catalog_object(
        &self,
        database: &str,
        schema: &str,
        name: &str,
        kind: &str,
    ) -> Result<(), ProjectError> {
        self.require_active()?;
        // Validate against a fresh engine snapshot so stale frontend state
        // cannot change the object kind or target a non-existent relation.
        let snapshot = self.engine.catalog().map_err(ProjectError::Engine)?;
        let object = snapshot
            .objects
            .iter()
            .find(|object| {
                object.database == database && object.schema == schema && object.name == name
            })
            .ok_or_else(|| ProjectError::UnknownCatalogObject {
                database: database.to_string(),
                schema: schema.to_string(),
                name: name.to_string(),
            })?;
        if object.kind != kind {
            return Err(ProjectError::CatalogObjectKindMismatch {
                expected: object.kind.clone(),
                actual: kind.to_string(),
            });
        }
        self.engine
            .drop_catalog_object(database, schema, name, kind)
            .map_err(ProjectError::Engine)
    }

    pub fn interrupt(&self) -> Result<bool, ProjectError> {
        self.require_active()?;
        Ok(false)
    }

    fn require_active(&self) -> Result<ActiveProject, ProjectError> {
        self.lock()?.clone().ok_or(ProjectError::NoActiveProject)
    }

    fn refresh_source_health(&self, project_id: &str) -> Result<(), ProjectError> {
        let repository = SourcesRepository::new(self.metadata.clone());
        for source in repository.list_sources(project_id)? {
            let state = check_source_health(&source);
            if state != source.state {
                repository.set_source_state(&source.id, state)?;
            }
        }
        Ok(())
    }

    fn lock(&self) -> Result<MutexGuard<'_, Option<ActiveProject>>, ProjectError> {
        self.active.lock().map_err(|_| ProjectError::Lock)
    }
}

fn check_source_health(
    source: &crate::metadata::sources::SourceRecord,
) -> crate::metadata::sources::SourceState {
    use crate::metadata::sources::{SourceKind, SourceState};
    match source.kind {
        SourceKind::LinkedParquet | SourceKind::LinkedCsv => source
            .source_path
            .as_ref()
            .map(Path::new)
            .filter(|path| path.is_file())
            .map(|_| SourceState::Ready)
            .unwrap_or(SourceState::Missing),
        SourceKind::DuckdbTable => SourceState::Ready,
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
    #[error("catalog object was not found: {database}.{schema}.{name}")]
    UnknownCatalogObject {
        database: String,
        schema: String,
        name: String,
    },
    #[error("catalog object kind changed; expected {expected}, request was {actual}")]
    CatalogObjectKindMismatch { expected: String, actual: String },
    #[error("project path is not valid UTF-8: {0}")]
    InvalidPath(PathBuf),
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
    #[error("engine operation failed: {0}")]
    Engine(String),
    #[error(transparent)]
    Metadata(#[from] crate::metadata::MetadataError),
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::engine_manager::EngineManager;

    fn fixture() -> (ProjectManager, PathBuf, Arc<EngineManager>) {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tarik-projects-{stamp}"));
        let engine = Arc::new(EngineManager::new(
            PathBuf::from("unused-engine-binary"),
            std::env::temp_dir().join("tarik-test-results"),
        ));
        let manager = ProjectManager::new(
            MetadataDb::open_in_memory().unwrap(),
            root.join("projects"),
            engine.clone(),
        );
        (manager, root, engine)
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
    fn external_rename_and_forget_preserve_user_file() {
        let (manager, root, _) = fixture();
        fs::create_dir_all(&root).unwrap();
        let external_path = root.join("user-owned.duckdb");
        fs::write(&external_path, b"not a real database").unwrap();
        let project = ProjectsRepository::new(manager.metadata.clone())
            .upsert("Warehouse", &external_path, ProjectOwnership::External)
            .unwrap();

        let renamed = manager.rename(&project.id, "Finance Warehouse").unwrap();
        assert_eq!(renamed.ownership, ProjectOwnership::External);
        assert_eq!(renamed.duckdb_path, external_path.to_string_lossy());
        assert!(external_path.is_file());
        assert_eq!(
            manager.remove(&project.id).unwrap(),
            ProjectRemoval::Forgotten
        );
        assert!(external_path.is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn managed_delete_cascades_metadata() {
        let (manager, root, _) = fixture();
        let project = ProjectsRepository::new(manager.metadata.clone())
            .upsert(
                "Disposable",
                Path::new("/tmp/tarik-no-such.duckdb"),
                ProjectOwnership::Managed,
            )
            .unwrap();
        assert!(manager.remove(&project.id).is_err()); // path is not under managed root
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(windows)]
    #[test]
    fn managed_rename_reports_windows_lock_without_changing_metadata() {
        use std::os::windows::fs::OpenOptionsExt;

        let (manager, root, _) = fixture();
        let directory = root.join("projects").join(Uuid::new_v4().to_string());
        fs::create_dir_all(&directory).unwrap();
        let old_path = directory.join("retail.duckdb");
        fs::write(&old_path, b"locked").unwrap();
        let project = ProjectsRepository::new(manager.metadata.clone())
            .upsert("Retail", &old_path, ProjectOwnership::Managed)
            .unwrap();
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&old_path)
            .unwrap();

        assert!(matches!(
            manager.rename(&project.id, "Finance"),
            Err(ProjectError::Rename { .. })
        ));
        let current = ProjectsRepository::new(manager.metadata.clone())
            .find(&project.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.name, "Retail");
        assert_eq!(current.duckdb_path, old_path.to_string_lossy());
        assert!(old_path.is_file());
        assert!(!directory.join("finance.duckdb").exists());

        drop(locked);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn managed_rename_rejects_filename_collision() {
        let (manager, root, _) = fixture();
        let directory = root.join("projects").join(Uuid::new_v4().to_string());
        fs::create_dir_all(&directory).unwrap();
        let old_path = directory.join("retail.duckdb");
        fs::write(&old_path, b"x").unwrap();
        let project = ProjectsRepository::new(manager.metadata.clone())
            .upsert("Retail", &old_path, ProjectOwnership::Managed)
            .unwrap();
        let collision = directory.join("finance.duckdb");
        fs::write(&collision, b"x").unwrap();

        assert!(matches!(
            manager.rename(&project.id, "Finance"),
            Err(ProjectError::DestinationExists(_))
        ));
        let current = ProjectsRepository::new(manager.metadata.clone())
            .find(&project.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.name, "Retail");
        assert!(old_path.exists());
        let _ = fs::remove_dir_all(root);
    }
}

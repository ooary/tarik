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
    metadata::{projects::ProjectsRepository, MetadataDb},
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
        let database_path = project_directory.join("project.duckdb");
        match self.open_path(name, &database_path) {
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
        self.open_path(validate_name(name)?, path)
    }

    fn open_path(&self, name: &str, path: &Path) -> Result<ActiveProject, ProjectError> {
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
        let recent = ProjectsRepository::new(self.metadata.clone()).upsert(name, path)?;
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

    fn lock(&self) -> Result<MutexGuard<'_, Option<OpenProject>>, ProjectError> {
        self.active.lock().map_err(|_| ProjectError::Lock)
    }
}

fn validate_name(name: &str) -> Result<&str, ProjectError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(ProjectError::InvalidName);
    }
    Ok(name)
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("a DuckDB project is already open")]
    AlreadyOpen,
    #[error("no DuckDB project is open")]
    NoActiveProject,
    #[error("project name cannot be empty")]
    InvalidName,
    #[error("project path is not a file: {0}")]
    NotAFile(PathBuf),
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
        let created = manager.create("Retail").unwrap();
        assert!(created.duckdb_path.is_file());
        manager.close().unwrap();

        Connection::open(&created.duckdb_path)
            .unwrap()
            .execute_batch("CREATE TABLE persisted(id INTEGER);")
            .unwrap();
        manager.open("Retail", &created.duckdb_path).unwrap();
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

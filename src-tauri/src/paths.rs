use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDirectories {
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub log_dir: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("could not resolve application directories: {0}")]
    Resolve(#[from] tauri::Error),
    #[error("could not create application directory {path}: {source}")]
    Create {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub fn resolve_directories<R: Runtime>(app: &AppHandle<R>) -> Result<AppDirectories, PathError> {
    let resolver = app.path();
    let directories = AppDirectories {
        data_dir: resolver.app_data_dir()?,
        cache_dir: resolver.app_cache_dir()?,
        log_dir: resolver.app_log_dir()?,
    };

    for path in [
        &directories.data_dir,
        &directories.cache_dir,
        &directories.log_dir,
    ] {
        std::fs::create_dir_all(path).map_err(|source| PathError::Create {
            path: path.clone(),
            source,
        })?;
    }

    Ok(directories)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directory_value_is_serializable() {
        let directories = AppDirectories {
            data_dir: PathBuf::from("/tmp/tarik/data"),
            cache_dir: PathBuf::from("/tmp/tarik/cache"),
            log_dir: PathBuf::from("/tmp/tarik/logs"),
        };

        let json = serde_json::to_string(&directories).expect("directory paths should serialize");
        assert!(json.contains("dataDir"));
        assert!(json.contains("cacheDir"));
        assert!(json.contains("logDir"));
    }
}

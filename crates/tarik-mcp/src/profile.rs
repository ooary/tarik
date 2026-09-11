use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

pub const PROFILE_FILE: &str = "profile.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientProfile {
    pub profile_id: String,
    pub client_label: String,
    pub pairing_key: String,
}

impl Drop for ClientProfile {
    fn drop(&mut self) {
        self.pairing_key.zeroize();
    }
}

pub fn profile_directory(profile_name: &str) -> Result<PathBuf, String> {
    let profile_name = validate_profile_name(profile_name)?;
    if let Some(root) = std::env::var_os("TARIK_MCP_CONFIG_DIR") {
        return Ok(PathBuf::from(root).join(profile_name));
    }
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "APPDATA is unavailable".to_string())?
        .join("com.tarik.desktop");
    #[cfg(target_os = "macos")]
    let root = home_dir()?
        .join("Library")
        .join("Application Support")
        .join("com.tarik.desktop");
    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".config"))
        .join("tarik");
    Ok(root.join("mcp").join(profile_name))
}

pub fn desktop_agent_directory() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("TARIK_AGENT_DIR") {
        return Ok(PathBuf::from(root));
    }
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "APPDATA is unavailable".to_string())?
        .join("com.tarik.desktop");
    #[cfg(target_os = "macos")]
    let root = home_dir()?
        .join("Library")
        .join("Application Support")
        .join("com.tarik.desktop");
    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or(home_dir()?.join(".local/share"))
        .join("com.tarik.desktop");
    Ok(root.join("agent"))
}

pub fn load(path: &Path) -> Result<Option<ClientProfile>, String> {
    let file = path.join(PROFILE_FILE);
    let metadata = match fs::symlink_metadata(&file) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not inspect MCP profile: {error}")),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("MCP profile is not a regular private file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err("MCP profile permissions are not user-private".into());
        }
    }
    let bytes = Zeroizing::new(
        fs::read(&file).map_err(|error| format!("could not read MCP profile: {error}"))?,
    );
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "MCP profile is malformed; remove it and pair again".into())
}

pub fn save(path: &Path, profile: &ClientProfile) -> Result<(), String> {
    prepare_private_directory(path)?;
    let target = path.join(PROFILE_FILE);
    let stage = path.join(format!(".{PROFILE_FILE}.tmp"));
    if fs::symlink_metadata(&target)
        .ok()
        .is_some_and(|metadata| metadata.file_type().is_symlink())
    {
        return Err("refusing to replace a symbolic-link MCP profile".into());
    }
    let bytes = Zeroizing::new(
        serde_json::to_vec(profile)
            .map_err(|error| format!("could not encode MCP profile: {error}"))?,
    );
    let mut options = fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&stage)
        .map_err(|error| format!("could not create MCP profile: {error}"))?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("could not save MCP profile: {error}"))?;
    fs::rename(stage, target).map_err(|error| format!("could not publish MCP profile: {error}"))
}

fn prepare_private_directory(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path)
        .map_err(|error| format!("could not create MCP profile directory: {error}"))?;
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("could not inspect MCP profile directory: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("MCP profile directory is unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("could not secure MCP profile directory: {error}"))?;
        let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err("MCP profile directory permissions are unsafe".into());
        }
    }
    Ok(())
}

fn validate_profile_name(value: &str) -> Result<&str, String> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(
            "profile name must use 1-64 ASCII letters, numbers, hyphens, or underscores".into(),
        );
    }
    Ok(value)
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "home directory is unavailable".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_cannot_escape_the_config_root() {
        assert!(validate_profile_name("claude-desktop").is_ok());
        for invalid in ["", "../escape", "a/b", "has space"] {
            assert!(validate_profile_name(invalid).is_err());
        }
    }

    #[test]
    fn profile_round_trip_uses_private_permissions() {
        let root = std::env::temp_dir().join(format!("tarik-mcp-profile-{}", uuid::Uuid::new_v4()));
        let profile = ClientProfile {
            profile_id: "profile-1".into(),
            client_label: "Pi".into(),
            pairing_key: "01".repeat(32),
        };
        save(&root, &profile).unwrap();
        let loaded = load(&root).unwrap().unwrap();
        assert_eq!(loaded.profile_id, "profile-1");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o077, 0);
            assert_eq!(
                fs::metadata(root.join(PROFILE_FILE))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o077,
                0
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}

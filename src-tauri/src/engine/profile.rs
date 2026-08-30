use std::path::{Path, PathBuf};

use duckdb::Connection;
use serde::{Deserialize, Serialize};

use super::EngineError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineProfileName {
    LowMemory,
    Balanced,
    Fast,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineProfile {
    pub name: EngineProfileName,
    pub memory_limit_mb: u32,
    pub threads: u16,
    pub temp_directory: PathBuf,
}

impl EngineProfile {
    pub fn preset(name: EngineProfileName, temp_directory: impl Into<PathBuf>) -> Self {
        let (memory_limit_mb, threads) = match name {
            EngineProfileName::LowMemory => (512, 1),
            EngineProfileName::Balanced => (2_048, 2),
            EngineProfileName::Fast => (8_192, 4),
        };
        Self {
            name,
            memory_limit_mb,
            threads,
            temp_directory: temp_directory.into(),
        }
    }

    pub fn validate(&self) -> Result<(), EngineError> {
        if !(128..=262_144).contains(&self.memory_limit_mb) {
            return Err(EngineError::InvalidProfile(
                "memory limit must be between 128 MB and 256 GB",
            ));
        }
        if !(1..=256).contains(&self.threads) {
            return Err(EngineError::InvalidProfile(
                "thread count must be between 1 and 256",
            ));
        }
        if self.temp_directory.as_os_str().is_empty() {
            return Err(EngineError::InvalidProfile(
                "temporary directory is required",
            ));
        }
        Ok(())
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), EngineError> {
        self.validate()?;
        let memory = format!("{}MB", self.memory_limit_mb);
        connection.execute("SET memory_limit = ?", [memory])?;
        connection.execute("SET threads = ?", [self.threads])?;
        set_temp_directory(connection, &self.temp_directory)?;
        Ok(())
    }
}

fn set_temp_directory(connection: &Connection, path: &Path) -> Result<(), EngineError> {
    let path = path.to_str().ok_or(EngineError::InvalidProfile(
        "temporary directory must be valid UTF-8",
    ))?;
    connection.execute("SET temp_directory = ?", [path])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_map_to_increasing_resources() {
        let low = EngineProfile::preset(EngineProfileName::LowMemory, "/tmp/tarik");
        let balanced = EngineProfile::preset(EngineProfileName::Balanced, "/tmp/tarik");
        let fast = EngineProfile::preset(EngineProfileName::Fast, "/tmp/tarik");

        assert!(low.memory_limit_mb < balanced.memory_limit_mb);
        assert!(balanced.memory_limit_mb < fast.memory_limit_mb);
        assert!(low.threads < fast.threads);
    }

    #[test]
    fn rejects_out_of_range_values() {
        let profile = EngineProfile {
            name: EngineProfileName::Balanced,
            memory_limit_mb: 20,
            threads: 0,
            temp_directory: "/tmp".into(),
        };
        assert!(matches!(
            profile.validate(),
            Err(EngineError::InvalidProfile(_))
        ));
    }

    #[test]
    fn applies_settings_without_sql_interpolation() {
        let connection = Connection::open_in_memory().unwrap();
        let profile = EngineProfile::preset(EngineProfileName::LowMemory, "/tmp/tarik's-cache");

        profile.apply(&connection).unwrap();

        let threads: i64 = connection
            .query_row("SELECT current_setting('threads')", [], |row| row.get(0))
            .unwrap();
        assert_eq!(threads, 1);
    }
}

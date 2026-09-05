use duckdb::Connection;
use tarik_engine_protocol::{
    EffectiveEngineResources, EngineResourceSettings, EngineResourceValidationError,
};

use crate::error::EngineError;

pub fn apply(
    connection: &Connection,
    settings: &EngineResourceSettings,
) -> Result<EffectiveEngineResources, EngineError> {
    settings
        .validate()
        .map_err(|error| EngineError::InvalidResources(error.to_string()))?;

    let memory = format!("{}MiB", settings.memory_limit_mib);
    connection.execute("SET memory_limit = ?", [&memory])?;
    connection.execute("SET threads = ?", [settings.threads])?;
    readback(connection, settings)
}

pub fn readback(
    connection: &Connection,
    requested: &EngineResourceSettings,
) -> Result<EffectiveEngineResources, EngineError> {
    let memory_display: String =
        connection.query_row("SELECT current_setting('memory_limit')", [], |row| {
            row.get(0)
        })?;
    let threads: u16 =
        connection.query_row("SELECT current_setting('threads')", [], |row| row.get(0))?;
    let memory_limit_mib = parse_memory_mib(&memory_display).ok_or_else(|| {
        EngineError::InvalidResources("DuckDB memory readback was invalid".into())
    })?;
    if memory_limit_mib != requested.memory_limit_mib || threads != requested.threads {
        return Err(EngineError::InvalidResources(format!(
            "DuckDB readback mismatch: requested {} MiB/{} threads, found {} MiB/{} threads",
            requested.memory_limit_mib, requested.threads, memory_limit_mib, threads
        )));
    }
    Ok(EffectiveEngineResources {
        preset: requested.preset,
        memory_limit_mib,
        memory_limit_display: memory_display,
        threads,
    })
}

fn parse_memory_mib(value: &str) -> Option<u64> {
    let normalized = value.trim().to_ascii_uppercase().replace(' ', "");
    for suffix in ["TIB", "GIB", "MIB", "KIB", "TB", "GB", "MB", "KB", "B"] {
        if let Some(number) = normalized.strip_suffix(suffix) {
            let amount = number.parse::<f64>().ok()?;
            let mib = match suffix {
                "TIB" | "TB" => amount * 1024.0 * 1024.0,
                "GIB" | "GB" => amount * 1024.0,
                "MIB" | "MB" => amount,
                "KIB" | "KB" => amount / 1024.0,
                "B" => amount / 1024.0 / 1024.0,
                _ => return None,
            };
            if !mib.is_finite() || mib < 0.0 || mib > u64::MAX as f64 {
                return None;
            }
            return Some(mib.round() as u64);
        }
    }
    None
}

impl From<EngineResourceValidationError> for EngineError {
    fn from(error: EngineResourceValidationError) -> Self {
        Self::InvalidResources(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarik_engine_protocol::{EngineResourcePreset, EngineResourceSettings};

    #[test]
    fn applies_parameterized_settings_and_reads_effective_values() {
        let connection = Connection::open_in_memory().unwrap();
        let requested = EngineResourceSettings::preset(EngineResourcePreset::LowMemory);

        let effective = apply(&connection, &requested).unwrap();

        assert_eq!(effective.memory_limit_mib, 512);
        assert_eq!(effective.threads, 1);
        assert!(!effective.memory_limit_display.is_empty());
    }

    #[test]
    fn parses_duckdb_memory_units_without_unsafe_rounding() {
        assert_eq!(parse_memory_mib("512.0 MiB"), Some(512));
        assert_eq!(parse_memory_mib("512 MiB"), Some(512));
        assert_eq!(parse_memory_mib("2 GiB"), Some(2048));
        assert_eq!(parse_memory_mib("536870912 B"), Some(512));
    }
}

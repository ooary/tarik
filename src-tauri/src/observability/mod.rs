//! Bounded structured application logging.
//!
//! The persisted schema is intentionally closed: callers can record operation,
//! project/incident IDs, duration, status, and stable error codes, but cannot
//! attach SQL, parameters, previews, result rows, or arbitrary data maps.

use std::{
    fs::{self, File, OpenOptions},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Instant,
};

use serde::Serialize;

const ACTIVE_LOG: &str = "tarik.log";
const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024;
const DEFAULT_ARCHIVES: usize = 6;
const MAX_TEXT_BYTES: usize = 512;

#[derive(Debug, Clone, Copy)]
struct LogPolicy {
    max_bytes: u64,
    archives: usize,
}

impl Default for LogPolicy {
    fn default() -> Self {
        Self {
            max_bytes: DEFAULT_MAX_BYTES,
            archives: DEFAULT_ARCHIVES,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PersistedEvent<'a> {
    timestamp: String,
    level: LogLevel,
    target: &'a str,
    event: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    incident_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
}

#[derive(Debug, Clone, Default)]
pub struct EventFields<'a> {
    pub operation_id: Option<&'a str>,
    pub project_id: Option<&'a str>,
    pub incident_id: Option<&'a str>,
    pub duration_ms: Option<u64>,
    pub status: Option<&'a str>,
    pub error_code: Option<&'a str>,
    /// Message must be operational, not user content. SQL-like text is
    /// replaced before persistence as a second line of defense.
    pub message: Option<&'a str>,
}

struct LogState {
    writer: Option<BufWriter<File>>,
    bytes_written: u64,
    degraded: bool,
}

pub struct AppLogger {
    directory: PathBuf,
    policy: LogPolicy,
    state: Mutex<LogState>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogInfo {
    pub directory: PathBuf,
    pub active_file: PathBuf,
    pub max_file_bytes: u64,
    pub retained_files: usize,
    pub available: bool,
}

impl AppLogger {
    pub fn open(directory: PathBuf) -> Self {
        Self::open_with_policy(directory, LogPolicy::default())
    }

    fn open_with_policy(directory: PathBuf, policy: LogPolicy) -> Self {
        let opened = open_active(&directory);
        let state = match opened {
            Ok((writer, bytes_written)) => LogState {
                writer: Some(writer),
                bytes_written,
                degraded: false,
            },
            Err(error) => {
                eprintln!("tarik: file logging unavailable: {error}");
                LogState {
                    writer: None,
                    bytes_written: 0,
                    degraded: true,
                }
            }
        };
        let logger = Self {
            directory,
            policy,
            state: Mutex::new(state),
        };
        logger.remove_excess_archives();
        logger
    }

    pub fn info(&self) -> LogInfo {
        let available = self
            .state
            .lock()
            .map(|state| !state.degraded && state.writer.is_some())
            .unwrap_or(false);
        LogInfo {
            directory: self.directory.clone(),
            active_file: self.directory.join(ACTIVE_LOG),
            max_file_bytes: self.policy.max_bytes,
            retained_files: self.policy.archives + 1,
            available,
        }
    }

    pub fn record(&self, level: LogLevel, target: &str, event: &str, fields: EventFields<'_>) {
        let target = sanitize_token(target);
        let event = sanitize_token(event);
        let operation_id = fields.operation_id.map(sanitize_token);
        let project_id = fields.project_id.map(sanitize_token);
        let incident_id = fields.incident_id.map(sanitize_token);
        let status = fields.status.map(sanitize_token);
        let error_code = fields.error_code.map(sanitize_token);
        let message = fields.message.map(redact_message);
        let persisted = PersistedEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level,
            target: &target,
            event: &event,
            operation_id: operation_id.as_deref(),
            project_id: project_id.as_deref(),
            incident_id: incident_id.as_deref(),
            duration_ms: fields.duration_ms,
            status: status.as_deref(),
            error_code: error_code.as_deref(),
            message: message.as_deref(),
        };
        let Ok(mut line) = serde_json::to_vec(&persisted) else {
            return;
        };
        line.push(b'\n');
        self.write_line(&line);
    }

    pub fn operation(
        self: &Arc<Self>,
        target: &'static str,
        event: &'static str,
        project_id: Option<String>,
    ) -> OperationSpan {
        let operation_id = uuid::Uuid::new_v4().to_string();
        self.record(
            LogLevel::Info,
            target,
            event,
            EventFields {
                operation_id: Some(&operation_id),
                project_id: project_id.as_deref(),
                status: Some("started"),
                ..EventFields::default()
            },
        );
        OperationSpan {
            logger: Arc::clone(self),
            target,
            event,
            project_id,
            operation_id,
            started_at: Instant::now(),
            finished: false,
        }
    }

    pub fn flush(&self) -> std::io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| std::io::Error::other("logger lock unavailable"))?;
        match state.writer.as_mut() {
            Some(writer) => writer.flush(),
            None => Ok(()),
        }
    }

    fn write_line(&self, line: &[u8]) {
        let Ok(mut state) = self.state.lock() else {
            eprintln!("tarik: logger lock unavailable");
            return;
        };
        if state.bytes_written > 0
            && state.bytes_written.saturating_add(line.len() as u64) > self.policy.max_bytes
        {
            if let Err(error) = self.rotate(&mut state) {
                state.writer = None;
                state.degraded = true;
                eprintln!("tarik: log rotation failed: {error}");
            }
        }
        let Some(writer) = state.writer.as_mut() else {
            eprintln!("tarik log: {}", String::from_utf8_lossy(line).trim());
            return;
        };
        if let Err(error) = writer.write_all(line).and_then(|_| writer.flush()) {
            state.writer = None;
            state.degraded = true;
            eprintln!("tarik: log write failed: {error}");
            return;
        }
        state.bytes_written = state.bytes_written.saturating_add(line.len() as u64);
    }

    fn rotate(&self, state: &mut LogState) -> std::io::Result<()> {
        if let Some(mut writer) = state.writer.take() {
            writer.flush()?;
        }
        if self.policy.archives == 0 {
            let _ = fs::remove_file(self.directory.join(ACTIVE_LOG));
        } else {
            let oldest = archive_path(&self.directory, self.policy.archives);
            remove_regular_file(&oldest)?;
            for index in (1..self.policy.archives).rev() {
                let from = archive_path(&self.directory, index);
                let to = archive_path(&self.directory, index + 1);
                rename_regular_file(&from, &to)?;
            }
            let active = self.directory.join(ACTIVE_LOG);
            if active.is_file() {
                fs::rename(active, archive_path(&self.directory, 1))?;
            }
        }
        let (writer, bytes) = open_active(&self.directory)?;
        state.writer = Some(writer);
        state.bytes_written = bytes;
        state.degraded = false;
        Ok(())
    }

    fn remove_excess_archives(&self) {
        let Ok(entries) = fs::read_dir(&self.directory) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(index) = name
                .strip_prefix("tarik.log.")
                .and_then(|value| value.parse::<usize>().ok())
            else {
                continue;
            };
            if index > self.policy.archives && entry.path().is_file() {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

pub struct OperationSpan {
    logger: Arc<AppLogger>,
    target: &'static str,
    event: &'static str,
    project_id: Option<String>,
    operation_id: String,
    started_at: Instant,
    finished: bool,
}

impl OperationSpan {
    pub fn succeed(mut self) {
        self.finish(LogLevel::Info, "succeeded", None, None);
    }

    pub fn fail(mut self, error_code: &str, safe_message: &str) {
        self.finish(
            LogLevel::Error,
            "failed",
            Some(error_code),
            Some(safe_message),
        );
    }

    fn finish(
        &mut self,
        level: LogLevel,
        status: &str,
        error_code: Option<&str>,
        message: Option<&str>,
    ) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.logger.record(
            level,
            self.target,
            self.event,
            EventFields {
                operation_id: Some(&self.operation_id),
                project_id: self.project_id.as_deref(),
                duration_ms: Some(self.started_at.elapsed().as_millis() as u64),
                status: Some(status),
                error_code,
                message,
                ..EventFields::default()
            },
        );
    }
}

impl Drop for OperationSpan {
    fn drop(&mut self) {
        if !self.finished {
            self.finish(
                LogLevel::Warning,
                "abandoned",
                Some("operation.abandoned"),
                Some("operation ended without a terminal outcome"),
            );
        }
    }
}

fn open_active(directory: &Path) -> std::io::Result<(BufWriter<File>, u64)> {
    fs::create_dir_all(directory)?;
    let path = directory.join(ACTIVE_LOG);
    if fs::symlink_metadata(&path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(std::io::Error::other("active log path is a symbolic link"));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let bytes = file.metadata()?.len();
    Ok((BufWriter::new(file), bytes))
}

fn archive_path(directory: &Path, index: usize) -> PathBuf {
    directory.join(format!("{ACTIVE_LOG}.{index}"))
}

fn remove_regular_file(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(std::io::Error::other(
            "refusing to rotate a symbolic-link archive",
        )),
        Ok(metadata) if metadata.is_file() => fs::remove_file(path),
        Ok(_) => Err(std::io::Error::other("log archive is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn rename_regular_file(from: &Path, to: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(from) {
        Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
            fs::rename(from, to)
        }
        Ok(_) => Err(std::io::Error::other("log archive is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sanitize_token(value: &str) -> String {
    value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        })
        .take(128)
        .collect()
}

fn redact_message(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let lower = normalized.to_ascii_lowercase();
    let sql_words = [
        "select ", "insert ", "update ", "delete ", "create ", "alter ", "drop ", "copy ",
        "pragma ",
    ];
    if sql_words.iter().any(|word| lower.contains(word)) {
        return "[redacted user content]".into();
    }
    truncate_utf8(&normalized, MAX_TEXT_BYTES)
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

#[tauri::command]
pub fn get_log_info(logger: tauri::State<'_, Arc<AppLogger>>) -> LogInfo {
    logger.info()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("tarik-log-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn writes_closed_json_schema_and_redacts_sql_like_messages() {
        let directory = temp_dir("redaction");
        let logger = AppLogger::open(directory.clone());
        logger.record(
            LogLevel::Error,
            "query",
            "execute",
            EventFields {
                operation_id: Some("op-1"),
                project_id: Some("project-1"),
                duration_ms: Some(42),
                status: Some("failed"),
                error_code: Some("sql.parse"),
                message: Some("Parser failed for SELECT secret FROM customer_rows"),
                ..EventFields::default()
            },
        );
        logger.flush().unwrap();

        let text = fs::read_to_string(directory.join(ACTIVE_LOG)).unwrap();
        let event: serde_json::Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(event["target"], "query");
        assert_eq!(event["operationId"], "op-1");
        assert_eq!(event["projectId"], "project-1");
        assert_eq!(event["durationMs"], 42);
        assert_eq!(event["message"], "[redacted user content]");
        assert!(!text.contains("secret"));
        assert!(event.get("sql").is_none());
        assert!(event.get("rows").is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rotates_by_size_and_removes_excess_archives() {
        let directory = temp_dir("rotation");
        fs::write(directory.join("tarik.log.9"), b"old").unwrap();
        let logger = AppLogger::open_with_policy(
            directory.clone(),
            LogPolicy {
                max_bytes: 220,
                archives: 2,
            },
        );
        for index in 0..20 {
            logger.record(
                LogLevel::Info,
                "test",
                "rotation",
                EventFields {
                    operation_id: Some(&format!("operation-{index}")),
                    ..EventFields::default()
                },
            );
        }
        logger.flush().unwrap();

        assert!(directory.join(ACTIVE_LOG).is_file());
        assert!(directory.join("tarik.log.1").is_file());
        assert!(directory.join("tarik.log.2").is_file());
        assert!(!directory.join("tarik.log.3").exists());
        assert!(!directory.join("tarik.log.9").exists());
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 3);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn operation_span_records_duration_and_terminal_status() {
        let directory = temp_dir("span");
        let logger = Arc::new(AppLogger::open(directory.clone()));
        logger
            .operation("project", "open", Some("p1".into()))
            .succeed();
        logger.flush().unwrap();
        let lines = fs::read_to_string(directory.join(ACTIVE_LOG)).unwrap();
        let events = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["status"], "started");
        assert_eq!(events[1]["status"], "succeeded");
        assert!(events[1]["durationMs"].as_u64().is_some());
        assert_eq!(events[0]["operationId"], events[1]["operationId"]);
        fs::remove_dir_all(directory).unwrap();
    }
}

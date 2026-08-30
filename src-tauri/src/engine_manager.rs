use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde_json::Value;
use tarik_engine_client::EngineProcess;
use tarik_engine_protocol::{
    CatalogSnapshot, CsvOptions, ImportOptions, ProjectLocator, SourceInspection, SourceRecord,
};

pub struct EngineManager {
    engine_bin: PathBuf,
    /// Directory where published result page artifacts live.
    result_root: PathBuf,
    process: Mutex<Option<EngineProcess>>,
    session_id: Mutex<Option<String>>,
}

impl EngineManager {
    pub fn new(engine_bin: PathBuf, result_root: PathBuf) -> Self {
        Self {
            engine_bin,
            result_root,
            process: Mutex::new(None),
            session_id: Mutex::new(None),
        }
    }

    fn with_process<T>(
        &self,
        operation: impl FnOnce(&mut EngineProcess) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .process
            .lock()
            .map_err(|_| "engine process lock".to_string())?;
        if guard.is_none() {
            let mut process = EngineProcess::start(&self.engine_bin)
                .map_err(|error| format!("could not start engine: {error}"))?;
            let info = process
                .handshake()
                .map_err(|error| format!("engine handshake failed: {error}"))?;
            if info.protocol_version != tarik_engine_protocol::PROTOCOL_VERSION {
                return Err(format!(
                    "engine protocol mismatch: expected {}, found {}",
                    tarik_engine_protocol::PROTOCOL_VERSION,
                    info.protocol_version
                ));
            }
            *guard = Some(process);
        }
        operation(guard.as_mut().expect("engine process present"))
    }

    pub fn open_session(&self, project_path: &Path) -> Result<(), String> {
        let mut session = self
            .session_id
            .lock()
            .map_err(|_| "session lock".to_string())?;
        // Close any previous session before opening a new one so a stale
        // session cannot block project reopen.
        if let Some(previous) = session.as_ref() {
            let _ = self.with_process(|process| {
                process
                    .request(
                        "session.close",
                        serde_json::json!({ "sessionId": previous }),
                    )
                    .map_err(|error| error.to_string())
            });
        }
        let session_id = uuid::Uuid::new_v4().to_string();
        let locator = ProjectLocator {
            engine_id: "duckdb".into(),
            payload: serde_json::json!({ "path": project_path.to_string_lossy() })
                .as_object()
                .unwrap()
                .clone(),
        };
        self.with_process(|process| {
            process
                .request(
                    "session.open",
                    serde_json::json!({
                        "sessionId": session_id,
                        "locator": locator,
                    }),
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })?;
        *session = Some(session_id);
        Ok(())
    }

    pub fn close_session(&self) -> Result<(), String> {
        let mut session = self
            .session_id
            .lock()
            .map_err(|_| "session lock".to_string())?;
        if let Some(session_id) = session.as_ref() {
            self.with_process(|process| {
                process
                    .request(
                        "session.close",
                        serde_json::json!({ "sessionId": session_id }),
                    )
                    .map_err(|error| error.to_string())
            })?;
            *session = None;
        }
        Ok(())
    }

    fn require_session_id(&self) -> Result<String, String> {
        self.session_id
            .lock()
            .map_err(|_| "session lock".to_string())?
            .clone()
            .ok_or_else(|| "no engine session is open".to_string())
    }

    fn session_request(&self, method: &str, mut params: Value) -> Result<Value, String> {
        let session_id = self.require_session_id()?;
        if let Value::Object(map) = &mut params {
            map.insert("sessionId".into(), Value::String(session_id));
        }
        self.with_process(|process| {
            process
                .request(method, params)
                .map_err(|error| error.to_string())
        })
    }

    pub fn catalog(&self) -> Result<CatalogSnapshot, String> {
        let value = self.session_request("catalog.inspect", serde_json::json!({}))?;
        serde_json::from_value(value).map_err(|error| format!("catalog decode failed: {error}"))
    }

    pub fn inspect_source(
        &self,
        path: &str,
        csv: Option<CsvOptions>,
    ) -> Result<SourceInspection, String> {
        let value = self.with_process(|process| {
            process
                .request(
                    "source.inspect",
                    serde_json::json!({ "path": path, "csv": csv }),
                )
                .map_err(|error| error.to_string())
        })?;
        serde_json::from_value(value).map_err(|error| format!("source decode failed: {error}"))
    }

    pub fn link_parquet(
        &self,
        project_id: &str,
        path: &str,
        view_name: &str,
    ) -> Result<SourceRecord, String> {
        let value = self.session_request(
            "duckdb.source.link_parquet",
            serde_json::json!({
                "projectId": project_id,
                "path": path,
                "viewName": view_name,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| format!("source decode failed: {error}"))
    }

    pub fn import_table(
        &self,
        project_id: &str,
        path: &str,
        options: ImportOptions,
    ) -> Result<SourceRecord, String> {
        let value = self.session_request(
            "duckdb.source.import_table",
            serde_json::json!({
                "projectId": project_id,
                "path": path,
                "options": options,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| format!("source decode failed: {error}"))
    }

    pub fn repair_link(
        &self,
        source: &SourceRecord,
        replacement: &str,
    ) -> Result<SourceRecord, String> {
        let value = self.session_request(
            "duckdb.source.repair_link",
            serde_json::json!({
                "source": source,
                "replacement": replacement,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| format!("source decode failed: {error}"))
    }

    pub fn drop_link(&self, source: &SourceRecord) -> Result<(), String> {
        self.session_request(
            "duckdb.source.drop_link",
            serde_json::json!({ "source": source }),
        )
        .map(|_| ())
    }

    fn structured_code(error: &str) -> Option<&str> {
        error
            .strip_prefix("engine returned a structured error: ")
            .and_then(|rest| rest.split(": ").next())
    }

    fn raw_request(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.with_process(|process| {
            process
                .request(method, params)
                .map_err(|error| error.to_string())
        })
    }

    /// Submit an asynchronous query job for the active engine session. Returns
    /// once the job is queued.
    pub fn execute_query(&self, execution_id: &str, sql: &str) -> Result<(), String> {
        self.session_request(
            "query.execute",
            serde_json::json!({
                "executionId": execution_id,
                "sql": sql,
                "cacheDir": self.result_root.to_string_lossy(),
            }),
        )
        .map(|_| ())
    }

    /// Poll a job status. `Ok(None)` means the engine no longer tracks the
    /// execution (pruned or unknown).
    pub fn query_status(
        &self,
        execution_id: &str,
    ) -> Result<Option<tarik_engine_protocol::ExecutionStatus>, String> {
        match self.raw_request(
            "query.status",
            serde_json::json!({ "executionId": execution_id }),
        ) {
            Ok(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|error| format!("execution status decode failed: {error}")),
            Err(error) => {
                if Self::structured_code(&error) == Some("execution.missing") {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }

    /// Request cancellation. `Ok(None)` means the engine no longer tracks
    /// the execution.
    pub fn cancel_query(
        &self,
        execution_id: &str,
    ) -> Result<Option<tarik_engine_protocol::ExecutionStatus>, String> {
        match self.raw_request(
            "query.cancel",
            serde_json::json!({ "executionId": execution_id }),
        ) {
            Ok(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|error| format!("execution status decode failed: {error}")),
            Err(error) => {
                if Self::structured_code(&error) == Some("execution.missing") {
                    Ok(None)
                } else {
                    Err(error)
                }
            }
        }
    }

    /// Read one bounded page window of a published result.
    pub fn result_page(
        &self,
        result_id: &str,
        offset: u64,
        max_rows: u32,
    ) -> Result<serde_json::Value, String> {
        self.raw_request(
            "result.get_page",
            serde_json::json!({
                "resultId": result_id,
                "offset": offset,
                "maxRows": max_rows,
            }),
        )
    }

    /// Delete a published result and its page artifacts.
    pub fn release_result(&self, result_id: &str) -> Result<(), String> {
        self.raw_request(
            "result.release",
            serde_json::json!({ "resultId": result_id }),
        )
        .map(|_| ())
    }

    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(process) = guard.take() {
                process.shutdown();
            }
        }
    }
}

impl crate::query::EngineExecutor for EngineManager {
    fn execute(&self, execution_id: &str, sql: &str) -> Result<(), String> {
        self.execute_query(execution_id, sql)
    }

    fn status(
        &self,
        execution_id: &str,
    ) -> Result<Option<tarik_engine_protocol::ExecutionStatus>, String> {
        self.query_status(execution_id)
    }

    fn cancel(
        &self,
        execution_id: &str,
    ) -> Result<Option<tarik_engine_protocol::ExecutionStatus>, String> {
        self.cancel_query(execution_id)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use tarik_engine_protocol::ExecutionState;

    fn workspace_engine() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/debug/tarik-engine-duckdb")
    }

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tarik-manager-{name}-{stamp}{suffix}"))
    }

    #[test]
    fn execute_query_injects_the_active_session_id() {
        let engine_bin = workspace_engine();
        assert!(
            engine_bin.exists(),
            "engine binary missing; build with ./scripts/build-engine.sh"
        );
        let database = temp_path("session", ".duckdb");
        let result_root = temp_path("results", "");
        let manager = EngineManager::new(engine_bin, result_root.clone());
        manager.open_session(&database).unwrap();

        // This public method must inject the active sessionId; omitting it is
        // rejected by the sidecar as request.missing_field.
        manager
            .execute_query("session-injection", "SELECT 42 AS answer;")
            .unwrap();

        let terminal = (0..200)
            .find_map(|_| {
                let status = manager.query_status("session-injection").unwrap()?;
                if matches!(
                    status.state,
                    ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
                ) {
                    Some(status)
                } else {
                    std::thread::sleep(Duration::from_millis(10));
                    None
                }
            })
            .expect("query did not reach a terminal state");
        assert_eq!(terminal.state, ExecutionState::Succeeded);
        assert_eq!(terminal.rows_produced, Some(1));
        assert!(terminal.result.is_some());

        manager.release_result("session-injection").unwrap();
        manager.close_session().unwrap();
        manager.shutdown();
        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_dir_all(result_root);
    }
}

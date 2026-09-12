use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde::Serialize;
use serde_json::Value;
use tarik_engine_client::EngineProcess;
use tarik_engine_protocol::{
    AgentMutationResult, AgentSqlClassification, CatalogSnapshot, CreateTableDefinition,
    CsvOptions, EffectiveEngineResources, EngineResourceSettings, ExportOptions, ExportStatus,
    ProfileRequest, ProfileStatus, ProjectLocator, SourceInspection, SourceRecord, SqlValidation,
};

pub struct EngineManager {
    engine_bin: PathBuf,
    /// Directory where published result page artifacts live.
    result_root: PathBuf,
    process: Mutex<Option<EngineProcess>>,
    session: Mutex<Option<EngineSession>>,
    requested_resources: Mutex<EngineResourceSettings>,
    effective_resources: Mutex<Option<EffectiveEngineResources>>,
}

struct EngineSession {
    id: String,
    project_path: PathBuf,
    process_id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub state: &'static str,
    pub process_id: Option<u32>,
}

impl EngineManager {
    pub fn new(engine_bin: PathBuf, result_root: PathBuf) -> Self {
        Self {
            engine_bin,
            result_root,
            process: Mutex::new(None),
            session: Mutex::new(None),
            requested_resources: Mutex::new(EngineResourceSettings::default()),
            effective_resources: Mutex::new(None),
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
        if guard.as_mut().is_some_and(|process| !process.is_usable()) {
            *guard = None;
        }
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
        let process = guard.as_mut().expect("engine process present");
        let result = operation(process);
        if result.is_err() && !process.is_usable() {
            *guard = None;
        }
        result
    }

    pub fn set_requested_resources(&self, resources: EngineResourceSettings) -> Result<(), String> {
        resources.validate().map_err(|error| error.to_string())?;
        *self
            .requested_resources
            .lock()
            .map_err(|_| "engine resource lock".to_string())? = resources;
        Ok(())
    }

    pub fn open_session(&self, project_path: &Path) -> Result<EffectiveEngineResources, String> {
        let requested = self
            .requested_resources
            .lock()
            .map_err(|_| "engine resource lock".to_string())?
            .clone();
        self.with_process(|process| {
            let mut session = self
                .session
                .lock()
                .map_err(|_| "session lock".to_string())?;
            if let Some(previous) = session.as_ref() {
                if previous.process_id == process.id() {
                    process
                        .request(
                            "session.close",
                            serde_json::json!({ "sessionId": previous.id }),
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            let session_id = uuid::Uuid::new_v4().to_string();
            let locator = ProjectLocator {
                engine_id: "duckdb".into(),
                payload: serde_json::json!({ "path": project_path.to_string_lossy() })
                    .as_object()
                    .unwrap()
                    .clone(),
            };
            let effective: EffectiveEngineResources = serde_json::from_value(
                process
                    .request(
                        "session.open",
                        serde_json::json!({
                            "sessionId": session_id,
                            "locator": locator,
                            "resources": requested,
                        }),
                    )
                    .map_err(|error| error.to_string())?,
            )
            .map_err(|error| format!("engine resource decode failed: {error}"))?;
            *session = Some(EngineSession {
                id: session_id,
                project_path: project_path.to_path_buf(),
                process_id: process.id(),
            });
            *self
                .effective_resources
                .lock()
                .map_err(|_| "effective resource lock".to_string())? = Some(effective.clone());
            Ok(effective)
        })
    }

    pub fn status(&self) -> EngineStatus {
        let mut process = match self.process.lock() {
            Ok(process) => process,
            Err(_) => {
                return EngineStatus {
                    state: "failed",
                    process_id: None,
                };
            }
        };
        if process.as_mut().is_some_and(|current| !current.is_usable()) {
            *process = None;
        }
        let process_id = process.as_ref().map(EngineProcess::id);
        let connected = process_id.is_some()
            && self
                .session
                .lock()
                .ok()
                .and_then(|session| session.as_ref().map(|current| current.process_id))
                == process_id;
        EngineStatus {
            state: if connected {
                "connected"
            } else if process_id.is_some() {
                "standby"
            } else {
                "stopped"
            },
            process_id,
        }
    }

    pub fn close_session(&self) -> Result<(), String> {
        let mut process_guard = self
            .process
            .lock()
            .map_err(|_| "engine process lock".to_string())?;
        if process_guard
            .as_mut()
            .is_some_and(|current| !current.is_usable())
        {
            *process_guard = None;
        }
        let current = self
            .session
            .lock()
            .map_err(|_| "session lock".to_string())?
            .take();
        if let Ok(mut effective) = self.effective_resources.lock() {
            *effective = None;
        }
        if let (Some(current), Some(active_process)) = (current, process_guard.as_mut()) {
            if current.process_id == active_process.id() {
                let result = active_process
                    .request(
                        "session.close",
                        serde_json::json!({ "sessionId": current.id }),
                    )
                    .map_err(|error| error.to_string());
                if result.is_err() && !active_process.is_usable() {
                    *process_guard = None;
                }
                result?;
            }
        }
        Ok(())
    }

    fn session_request(&self, method: &str, mut params: Value) -> Result<Value, String> {
        self.with_process(|process| {
            let mut session = self
                .session
                .lock()
                .map_err(|_| "session lock".to_string())?;
            let current = session
                .as_mut()
                .ok_or_else(|| "no engine session is open".to_string())?;
            if current.process_id != process.id() {
                let session_id = uuid::Uuid::new_v4().to_string();
                let locator = ProjectLocator {
                    engine_id: "duckdb".into(),
                    payload: serde_json::json!({ "path": current.project_path.to_string_lossy() })
                        .as_object()
                        .unwrap()
                        .clone(),
                };
                let requested = self
                    .requested_resources
                    .lock()
                    .map_err(|_| "engine resource lock".to_string())?
                    .clone();
                let effective: EffectiveEngineResources = serde_json::from_value(
                    process
                        .request(
                            "session.open",
                            serde_json::json!({
                                "sessionId": session_id,
                                "locator": locator,
                                "resources": requested,
                            }),
                        )
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| format!("engine resource decode failed: {error}"))?;
                *self
                    .effective_resources
                    .lock()
                    .map_err(|_| "effective resource lock".to_string())? = Some(effective);
                current.id = session_id;
                current.process_id = process.id();
            }
            if let Value::Object(map) = &mut params {
                map.insert("sessionId".into(), Value::String(current.id.clone()));
            }
            process
                .request(method, params)
                .map_err(|error| error.to_string())
        })
    }

    pub fn configure_resources(
        &self,
        requested: EngineResourceSettings,
    ) -> Result<EffectiveEngineResources, String> {
        requested.validate().map_err(|error| error.to_string())?;
        let value = self.session_request(
            "session.configure",
            serde_json::json!({ "resources": requested }),
        )?;
        let effective: EffectiveEngineResources = serde_json::from_value(value)
            .map_err(|error| format!("engine resource decode failed: {error}"))?;
        *self
            .requested_resources
            .lock()
            .map_err(|_| "engine resource lock".to_string())? = requested;
        *self
            .effective_resources
            .lock()
            .map_err(|_| "effective resource lock".to_string())? = Some(effective.clone());
        Ok(effective)
    }

    pub fn effective_resources(&self) -> Option<EffectiveEngineResources> {
        self.effective_resources.lock().ok()?.clone()
    }

    pub fn catalog(&self) -> Result<CatalogSnapshot, String> {
        let value = self.session_request("catalog.inspect", serde_json::json!({}))?;
        serde_json::from_value(value).map_err(|error| format!("catalog decode failed: {error}"))
    }

    pub fn classify_agent_sql(
        &self,
        sql: &str,
        registered_sources: &[tarik_engine_protocol::AgentRegisteredSource],
    ) -> Result<AgentSqlClassification, String> {
        let value = self.session_request(
            "agent.sql.classify",
            serde_json::json!({
                "sql": sql,
                "registeredSources": registered_sources,
            }),
        )?;
        serde_json::from_value(value)
            .map_err(|error| format!("agent SQL classification decode failed: {error}"))
    }

    pub fn execute_agent_mutation(
        &self,
        sql: &str,
        catalog_revision: &str,
        registered_sources: &[tarik_engine_protocol::AgentRegisteredSource],
    ) -> Result<AgentMutationResult, String> {
        let value = self.session_request(
            "agent.mutation.execute",
            serde_json::json!({
                "sql": sql,
                "catalogRevision": catalog_revision,
                "registeredSources": registered_sources,
            }),
        )?;
        serde_json::from_value(value)
            .map_err(|error| format!("agent mutation decode failed: {error}"))
    }

    pub fn create_table(&self, definition: &CreateTableDefinition) -> Result<(), String> {
        self.session_request(
            "catalog.create_table",
            serde_json::json!({ "definition": definition }),
        )
        .map(|_| ())
    }

    pub fn drop_catalog_object(
        &self,
        database: &str,
        schema: &str,
        name: &str,
        kind: &str,
    ) -> Result<(), String> {
        self.session_request(
            "catalog.drop_object",
            serde_json::json!({
                "database": database,
                "schema": schema,
                "name": name,
                "kind": kind,
            }),
        )
        .map(|_| ())
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

    pub fn start_import(
        &self,
        import_id: &str,
        request: &tarik_engine_protocol::ImportRequest,
    ) -> Result<tarik_engine_protocol::ImportStatus, String> {
        let value = self.session_request(
            "import.execute",
            serde_json::json!({
                "importId": import_id, "request": request,
            }),
        )?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    pub fn import_status(
        &self,
        import_id: &str,
    ) -> Result<tarik_engine_protocol::ImportStatus, String> {
        let value = self.raw_request(
            "import.status",
            serde_json::json!({ "importId": import_id }),
        )?;
        serde_json::from_value(value).map_err(|error| error.to_string())
    }

    pub fn cancel_import(
        &self,
        import_id: &str,
    ) -> Result<tarik_engine_protocol::ImportStatus, String> {
        let value = self.raw_request(
            "import.cancel",
            serde_json::json!({ "importId": import_id }),
        )?;
        serde_json::from_value(value).map_err(|error| error.to_string())
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

    pub fn validate_quality_read_only(&self, sql: &str) -> Result<(), String> {
        self.session_request(
            "quality.validate_read_only",
            serde_json::json!({ "sql": sql }),
        )
        .map(|_| ())
    }

    /// Parse and bind an immutable SQL revision with DuckDB EXPLAIN. The
    /// sidecar never submits these statements to query execution.
    pub fn validate_query(&self, sql: &str, revision: u64) -> Result<SqlValidation, String> {
        let value = self.session_request(
            "query.validate",
            serde_json::json!({ "sql": sql, "revision": revision }),
        )?;
        serde_json::from_value(value)
            .map_err(|error| format!("SQL validation decode failed: {error}"))
    }

    /// Submit an asynchronous query job for the active engine session. Returns
    /// once the job is queued.
    pub fn execute_query(&self, execution_id: &str, sql: &str) -> Result<(), String> {
        self.execute_query_bounded(execution_id, sql, None)
    }

    pub fn execute_query_bounded(
        &self,
        execution_id: &str,
        sql: &str,
        row_limit: Option<u64>,
    ) -> Result<(), String> {
        self.session_request(
            "query.execute",
            serde_json::json!({
                "executionId": execution_id,
                "sql": sql,
                "cacheDir": self.result_root.to_string_lossy(),
                "rowLimit": row_limit,
            }),
        )
        .map(|_| ())
    }

    /// Submit a bounded asynchronous profile for the active engine session.
    pub fn execute_profile(
        &self,
        profile_id: &str,
        request: &ProfileRequest,
    ) -> Result<(), String> {
        self.session_request(
            "profile.execute",
            serde_json::json!({
                "profileId": profile_id,
                "request": request,
            }),
        )
        .map(|_| ())
    }

    pub fn profile_status(&self, profile_id: &str) -> Result<Option<ProfileStatus>, String> {
        match self.raw_request(
            "profile.status",
            serde_json::json!({ "profileId": profile_id }),
        ) {
            Ok(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|error| format!("profile status decode failed: {error}")),
            Err(error) if Self::structured_code(&error) == Some("profile.missing") => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn cancel_profile(&self, profile_id: &str) -> Result<Option<ProfileStatus>, String> {
        match self.raw_request(
            "profile.cancel",
            serde_json::json!({ "profileId": profile_id }),
        ) {
            Ok(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|error| format!("profile status decode failed: {error}")),
            Err(error) if Self::structured_code(&error) == Some("profile.missing") => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Submit an asynchronous export for the active engine session. Options
    /// are validated by the engine before it starts a worker or creates files.
    pub fn execute_export(
        &self,
        export_id: &str,
        sql: &str,
        options: &ExportOptions,
    ) -> Result<(), String> {
        self.execute_export_bounded(export_id, sql, options, None)
    }

    pub fn execute_export_bounded(
        &self,
        export_id: &str,
        sql: &str,
        options: &ExportOptions,
        maximum_total_bytes: Option<u64>,
    ) -> Result<(), String> {
        self.session_request(
            "export.execute",
            serde_json::json!({
                "exportId": export_id,
                "sql": sql,
                "options": options,
                "maximumTotalBytes": maximum_total_bytes,
            }),
        )
        .map(|_| ())
    }

    pub fn export_status(&self, export_id: &str) -> Result<Option<ExportStatus>, String> {
        match self.raw_request(
            "export.status",
            serde_json::json!({ "exportId": export_id }),
        ) {
            Ok(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|error| format!("export status decode failed: {error}")),
            Err(error) if Self::structured_code(&error) == Some("export.missing") => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn cancel_export(&self, export_id: &str) -> Result<Option<ExportStatus>, String> {
        match self.raw_request(
            "export.cancel",
            serde_json::json!({ "exportId": export_id }),
        ) {
            Ok(value) => serde_json::from_value(value)
                .map(Some)
                .map_err(|error| format!("export status decode failed: {error}")),
            Err(error) if Self::structured_code(&error) == Some("export.missing") => Ok(None),
            Err(error) => Err(error),
        }
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

    pub fn release_all_results(&self) -> Result<u64, String> {
        let mut guard = self
            .process
            .lock()
            .map_err(|_| "engine process lock".to_string())?;
        if guard.as_mut().is_some_and(|process| !process.is_usable()) {
            *guard = None;
        }
        let Some(process) = guard.as_mut() else {
            return Ok(0);
        };
        let value = process
            .request("result.release_all", serde_json::json!({}))
            .map_err(|error| error.to_string())?;
        value
            .as_u64()
            .ok_or_else(|| "result release count decode failed".to_string())
    }

    pub fn shutdown(&self) {
        let process = self.process.lock().ok().and_then(|mut guard| guard.take());
        if let Ok(mut session) = self.session.lock() {
            *session = None;
        }
        if let Ok(mut effective) = self.effective_resources.lock() {
            *effective = None;
        }
        if let Some(process) = process {
            process.shutdown();
        }
    }

    #[cfg(test)]
    fn process_id(&self) -> Option<u32> {
        self.process.lock().ok()?.as_ref().map(EngineProcess::id)
    }

    #[cfg(test)]
    fn terminate_process(&self) {
        if let Ok(mut process) = self.process.lock() {
            if let Some(process) = process.as_mut() {
                process.terminate();
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
            .join("target/debug")
            .join(if cfg!(windows) {
                "tarik-engine-duckdb.exe"
            } else {
                "tarik-engine-duckdb"
            })
    }

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tarik-manager-{name}-{stamp}{suffix}"))
    }

    #[test]
    fn process_is_lazy_and_reused_in_standby() {
        let engine_bin = workspace_engine();
        assert!(
            engine_bin.exists(),
            "engine binary missing; build the DuckDB engine first"
        );
        let first_database = temp_path("standby-first", ".duckdb");
        let second_database = temp_path("standby-second", ".duckdb");
        let result_root = temp_path("standby-results", "");
        let manager = EngineManager::new(engine_bin, result_root.clone());

        assert_eq!(manager.process_id(), None);
        assert_eq!(manager.status().state, "stopped");
        manager.open_session(&first_database).unwrap();
        let process_id = manager.process_id().expect("process started lazily");
        assert_eq!(manager.status().state, "connected");
        manager.close_session().unwrap();
        assert_eq!(manager.process_id(), Some(process_id));
        assert_eq!(manager.status().state, "standby");
        manager.open_session(&second_database).unwrap();
        assert_eq!(manager.process_id(), Some(process_id));
        assert_eq!(manager.status().state, "connected");
        manager.close_session().unwrap();
        manager.shutdown();
        assert_eq!(manager.process_id(), None);
        assert_eq!(manager.status().state, "stopped");

        let _ = std::fs::remove_file(first_database);
        let _ = std::fs::remove_file(second_database);
        let _ = std::fs::remove_dir_all(result_root);
    }

    #[test]
    fn dead_process_restarts_and_reopens_the_active_session() {
        let engine_bin = workspace_engine();
        assert!(
            engine_bin.exists(),
            "engine binary missing; build the DuckDB engine first"
        );
        let database = temp_path("recovery", ".duckdb");
        let result_root = temp_path("recovery-results", "");
        let manager = EngineManager::new(engine_bin, result_root.clone());
        manager.open_session(&database).unwrap();
        let first_process = manager.process_id().unwrap();
        manager.terminate_process();
        assert_eq!(manager.status().state, "stopped");

        let catalog = manager.catalog().unwrap();
        let second_process = manager.process_id().unwrap();
        assert_ne!(first_process, second_process);
        assert!(catalog.objects.is_empty());
        assert_eq!(manager.status().state, "connected");

        manager.close_session().unwrap();
        manager.shutdown();
        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_dir_all(result_root);
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

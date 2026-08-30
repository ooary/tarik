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
    process: Mutex<Option<EngineProcess>>,
    session_id: Mutex<Option<String>>,
}

impl EngineManager {
    pub fn new(engine_bin: PathBuf) -> Self {
        Self {
            engine_bin,
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

    pub fn shutdown(&self) {
        if let Ok(mut guard) = self.process.lock() {
            if let Some(process) = guard.take() {
                process.shutdown();
            }
        }
    }
}

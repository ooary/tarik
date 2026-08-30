use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use serde_json::Value;
use tarik_engine_client::EngineProcess;
use tarik_engine_protocol::{
    CatalogSnapshot, CsvOptions, ImportOptions, ProjectLocator, SourceInspection, SourceRecord,
};

const ACTIVE_SESSION_ID: &str = "active";

pub struct EngineManager {
    engine_bin: PathBuf,
    process: Mutex<Option<EngineProcess>>,
    session_open: Mutex<bool>,
}

impl EngineManager {
    pub fn new(engine_bin: PathBuf) -> Self {
        Self {
            engine_bin,
            process: Mutex::new(None),
            session_open: Mutex::new(false),
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
        let mut open = self
            .session_open
            .lock()
            .map_err(|_| "session lock".to_string())?;
        if *open {
            return Err("an engine session is already open".into());
        }
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
                        "sessionId": ACTIVE_SESSION_ID,
                        "locator": locator,
                    }),
                )
                .map_err(|error| error.to_string())?;
            Ok(())
        })?;
        *open = true;
        Ok(())
    }

    pub fn close_session(&self) -> Result<(), String> {
        let mut open = self
            .session_open
            .lock()
            .map_err(|_| "session lock".to_string())?;
        if *open {
            self.with_process(|process| {
                process
                    .request(
                        "session.close",
                        serde_json::json!({ "sessionId": ACTIVE_SESSION_ID }),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(())
            })?;
            *open = false;
        }
        Ok(())
    }

    fn require_session(&self) -> Result<(), String> {
        let open = self
            .session_open
            .lock()
            .map_err(|_| "session lock".to_string())?;
        if *open {
            Ok(())
        } else {
            Err("no engine session is open".into())
        }
    }

    fn session_request(&self, method: &str, mut params: Value) -> Result<Value, String> {
        self.require_session()?;
        if let Value::Object(map) = &mut params {
            map.insert("sessionId".into(), Value::String(ACTIVE_SESSION_ID.into()));
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

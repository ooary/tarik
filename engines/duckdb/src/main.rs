mod catalog;
mod error;
mod session;
mod sources;

use std::io::{BufRead, Write};

use serde_json::Value;
use tarik_engine_protocol::{
    Capabilities, EngineInfo, ErrorEnvelope, ProjectLocator, RequestEnvelope, ResponseEnvelope,
    SourceState, PROTOCOL_VERSION,
};

use crate::error::EngineError;

const ENGINE_ID: &str = "duckdb";

fn engine_info() -> EngineInfo {
    EngineInfo {
        engine_id: ENGINE_ID.into(),
        engine_name: "DuckDB".into(),
        engine_version: env!("CARGO_PKG_VERSION").into(),
        protocol_version: PROTOCOL_VERSION,
        capabilities: Capabilities {
            link_parquet: true,
            import_csv: true,
            import_parquet: true,
            ..Default::default()
        },
        metadata: serde_json::Map::new(),
    }
}

fn required_string(
    params: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<String, EngineError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| EngineError::MissingField(key.to_string()))
}

fn dispatch(
    request: &RequestEnvelope,
    sessions: &mut session::SessionManager,
) -> Result<Value, EngineError> {
    let params = &request.params;
    match request.method.as_str() {
        "engine.handshake" => Ok(serde_json::to_value(engine_info())?),
        "engine.shutdown" => {
            std::process::exit(0);
        }
        "session.open" => {
            let session_id = required_string(params, "sessionId")?;
            let locator: ProjectLocator = serde_json::from_value(
                params
                    .get("locator")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("locator".into()))?,
            )?;
            let path = ProjectLocator::duckdb_path(&locator)
                .ok_or_else(|| EngineError::InvalidOptions("duckdb locator requires a path"))?;
            // Idempotent open: replace any stale session with the same id so a
            // previously failed or interrupted open cannot block a retry.
            let _ = sessions.close(&session_id);
            sessions.open(session_id, path)?;
            Ok(Value::Null)
        }
        "session.close" => {
            let session_id = required_string(params, "sessionId")?;
            sessions.close(&session_id)?;
            Ok(Value::Null)
        }
        "catalog.inspect" => {
            let session_id = required_string(params, "sessionId")?;
            let connection = sessions.get(&session_id)?;
            Ok(serde_json::to_value(catalog::inspect(connection)?)?)
        }
        "source.inspect" => {
            let path = required_string(params, "path")?;
            let csv = params
                .get("csv")
                .cloned()
                .map(serde_json::from_value::<tarik_engine_protocol::CsvOptions>)
                .transpose()?;
            Ok(serde_json::to_value(sources::inspect(
                std::path::Path::new(&path),
                csv.as_ref(),
            )?)?)
        }
        "duckdb.source.link_parquet" => {
            let session_id = required_string(params, "sessionId")?;
            let project_id = required_string(params, "projectId")?;
            let path = required_string(params, "path")?;
            let view_name = required_string(params, "viewName")?;
            let connection = sessions.get(&session_id)?;
            Ok(serde_json::to_value(sources::link_parquet(
                connection,
                &project_id,
                std::path::Path::new(&path),
                &view_name,
            )?)?)
        }
        "duckdb.source.import_table" => {
            let session_id = required_string(params, "sessionId")?;
            let project_id = required_string(params, "projectId")?;
            let path = required_string(params, "path")?;
            let options: tarik_engine_protocol::ImportOptions = serde_json::from_value(
                params
                    .get("options")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("options".into()))?,
            )?;
            let connection = sessions.get(&session_id)?;
            Ok(serde_json::to_value(sources::import_table(
                connection,
                &project_id,
                std::path::Path::new(&path),
                &options,
            )?)?)
        }
        "duckdb.source.repair_link" => {
            let session_id = required_string(params, "sessionId")?;
            let source: tarik_engine_protocol::SourceRecord = serde_json::from_value(
                params
                    .get("source")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("source".into()))?,
            )?;
            let replacement = required_string(params, "replacement")?;
            let connection = sessions.get(&session_id)?;
            Ok(serde_json::to_value(sources::repair_link(
                connection,
                &source,
                std::path::Path::new(&replacement),
            )?)?)
        }
        "duckdb.source.drop_link" => {
            let session_id = required_string(params, "sessionId")?;
            let source: tarik_engine_protocol::SourceRecord = serde_json::from_value(
                params
                    .get("source")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("source".into()))?,
            )?;
            let connection = sessions.get(&session_id)?;
            sources::drop_link(connection, &source)?;
            Ok(Value::Null)
        }
        "duckdb.source.check_health" => {
            let source: tarik_engine_protocol::SourceRecord = serde_json::from_value(
                params
                    .get("source")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("source".into()))?,
            )?;
            let state: SourceState = sources::check_link_health(&source);
            Ok(serde_json::to_value(state)?)
        }
        "engine.ping" => Ok(Value::String("pong".into())),
        _ => Err(EngineError::MethodNotFound(request.method.clone())),
    }
}

fn main() {
    let mut sessions = session::SessionManager::new();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<RequestEnvelope>(&line) {
            Ok(request) => match dispatch(&request, &mut sessions) {
                Ok(result) => ResponseEnvelope::ok(request.id, result),
                Err(error) => ResponseEnvelope::err(
                    request.id.clone(),
                    ErrorEnvelope {
                        request_id: Some(request.id.clone()),
                        ..ErrorEnvelope::new(error.code(), error.to_string())
                    },
                ),
            },
            Err(_) => ResponseEnvelope::err(
                "server".to_string(),
                ErrorEnvelope::new("request.parse", "invalid JSON frame"),
            ),
        };

        if writeln!(out, "{}", serde_json::to_string(&response).unwrap()).is_err() {
            break;
        }
        if out.flush().is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handshake_reports_duckdb_capabilities() {
        let info = engine_info();
        assert_eq!(info.engine_id, "duckdb");
        assert_eq!(info.protocol_version, PROTOCOL_VERSION);
        assert!(info.capabilities.link_parquet);
        assert!(info.capabilities.import_csv);
        assert!(info.capabilities.bounded_pages);
    }

    #[test]
    fn unknown_method_has_stable_code() {
        let request = RequestEnvelope {
            id: "r1".into(),
            method: "not.real".into(),
            params: serde_json::Map::new(),
        };
        let error = dispatch(&request, &mut session::SessionManager::new()).unwrap_err();
        assert_eq!(error.code(), "method.not_found");
    }

    #[test]
    fn session_open_requires_duckdb_locator() {
        let request = RequestEnvelope {
            id: "r2".into(),
            method: "session.open".into(),
            params: serde_json::json!({
                "sessionId": "s1",
                "locator": { "engineId": "postgres", "payload": { "host": "localhost" } }
            })
            .as_object()
            .unwrap()
            .clone(),
        };
        let error = dispatch(&request, &mut session::SessionManager::new()).unwrap_err();
        assert_eq!(error.code(), "source.invalid_options");
    }
}

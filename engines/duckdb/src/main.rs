mod catalog;
mod error;
pub mod export;
mod export_jobs;
mod jobs;
mod pages;
mod profile;
mod resources;
mod session;
mod sources;
mod sql;
mod validation;

use std::io::{BufRead, Write};

use serde_json::Value;
use tarik_engine_protocol::{
    Capabilities, EngineInfo, ErrorEnvelope, ProjectLocator, RequestEnvelope, ResponseEnvelope,
    PROTOCOL_VERSION,
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
            data_profiling: true,
            resource_controls: true,
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
    jobs: &std::sync::Arc<jobs::JobRegistry>,
    exports: &std::sync::Arc<export_jobs::ExportRegistry>,
    profiles: &std::sync::Arc<profile::ProfileRegistry>,
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
            let resources: tarik_engine_protocol::EngineResourceSettings =
                serde_json::from_value(params.get("resources").cloned().unwrap_or_else(|| {
                    serde_json::to_value(tarik_engine_protocol::EngineResourceSettings::default())
                        .expect("default resources serialize")
                }))?;
            Ok(serde_json::to_value(
                sessions.open(session_id, path, &resources)?,
            )?)
        }
        "catalog.inspect" => {
            let session_id = required_string(params, "sessionId")?;
            let connection = sessions.get(&session_id)?;
            Ok(serde_json::to_value(catalog::inspect(connection)?)?)
        }
        "catalog.create_table" => {
            let session_id = required_string(params, "sessionId")?;
            let definition: tarik_engine_protocol::CreateTableDefinition = serde_json::from_value(
                params
                    .get("definition")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("definition".into()))?,
            )?;
            let connection = sessions.get(&session_id)?;
            sources::create_table(connection, &definition)?;
            Ok(Value::Null)
        }
        "catalog.drop_object" => {
            let session_id = required_string(params, "sessionId")?;
            let database = required_string(params, "database")?;
            let schema = required_string(params, "schema")?;
            let name = required_string(params, "name")?;
            let kind = required_string(params, "kind")?;
            let connection = sessions.get(&session_id)?;
            catalog::drop_object(connection, &database, &schema, &name, &kind)?;
            Ok(Value::Null)
        }
        "source.inspect" => {
            let path = required_string(params, "path")?;
            // JSON null means "no CSV options" (e.g. Parquet or default CSV parse).
            let csv = match params.get("csv") {
                None | Some(Value::Null) => None,
                Some(value) => Some(serde_json::from_value::<tarik_engine_protocol::CsvOptions>(
                    value.clone(),
                )?),
            };
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
        "session.configure" => {
            let session_id = required_string(params, "sessionId")?;
            if jobs.has_active_session(&session_id)?
                || exports.has_active_session(&session_id)?
                || profiles.has_active_session(&session_id)?
            {
                return Err(EngineError::ResourcesBusy);
            }
            let requested: tarik_engine_protocol::EngineResourceSettings = serde_json::from_value(
                params
                    .get("resources")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("resources".into()))?,
            )?;
            Ok(serde_json::to_value(
                sessions.configure(&session_id, &requested)?,
            )?)
        }
        "session.resources" => {
            let session_id = required_string(params, "sessionId")?;
            Ok(serde_json::to_value(sessions.resources(&session_id)?)?)
        }
        "session.close" => {
            let session_id = required_string(params, "sessionId")?;
            // Cancel queued/running jobs first so closing cannot strand work.
            jobs.cancel_session(&session_id);
            exports.cancel_session(&session_id);
            profiles.cancel_session(&session_id);
            sessions.close(&session_id)?;
            Ok(Value::Null)
        }
        "profile.execute" => {
            let session_id = required_string(params, "sessionId")?;
            if jobs.has_active_session(&session_id)?
                || exports.has_active_session(&session_id)?
                || profiles.has_active_session(&session_id)?
            {
                return Err(EngineError::ProfileBusy);
            }
            let profile_id = required_string(params, "profileId")?;
            let profile_request: tarik_engine_protocol::ProfileRequest = serde_json::from_value(
                params
                    .get("request")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("request".into()))?,
            )?;
            let connection = sessions.get(&session_id)?.try_clone()?;
            profiles.execute(&session_id, &profile_id, profile_request, connection)?;
            Ok(serde_json::json!({
                "profileId": profile_id,
                "state": "queued",
            }))
        }
        "profile.status" => {
            let profile_id = required_string(params, "profileId")?;
            Ok(serde_json::to_value(profiles.status(&profile_id)?)?)
        }
        "profile.cancel" => {
            let profile_id = required_string(params, "profileId")?;
            Ok(serde_json::to_value(profiles.cancel(&profile_id)?)?)
        }
        "quality.validate_read_only" => {
            let session_id = required_string(params, "sessionId")?;
            let sql = required_string(params, "sql")?;
            let statement = sql::validate_quality_read_only(&sql)
                .map_err(|message| EngineError::QualitySqlUnsafe(message.into()))?;
            let connection = sessions.get(&session_id)?;
            connection.prepare(&format!("EXPLAIN (FORMAT JSON) {statement}"))?;
            Ok(serde_json::json!({ "readOnly": true }))
        }
        "query.validate" => {
            let session_id = required_string(params, "sessionId")?;
            let sql = required_string(params, "sql")?;
            let revision = params
                .get("revision")
                .and_then(Value::as_u64)
                .ok_or_else(|| EngineError::MissingField("revision".into()))?;
            let connection = sessions.get(&session_id)?;
            Ok(serde_json::to_value(validation::validate(
                connection, &sql, revision,
            ))?)
        }
        "query.execute" => {
            let session_id = required_string(params, "sessionId")?;
            if profiles.has_active_session(&session_id)? {
                return Err(EngineError::ProfileBusy);
            }
            let execution_id = required_string(params, "executionId")?;
            let sql = required_string(params, "sql")?;
            let cache_dir = params.get("cacheDir").and_then(Value::as_str);
            let connection = sessions.get(&session_id)?.try_clone()?;
            jobs.execute(
                &session_id,
                &execution_id,
                &sql,
                connection,
                cache_dir.map(std::path::Path::new),
            )?;
            Ok(serde_json::json!({
                "executionId": execution_id,
                "state": "queued",
            }))
        }
        "export.execute" => {
            let session_id = required_string(params, "sessionId")?;
            if profiles.has_active_session(&session_id)? {
                return Err(EngineError::ProfileBusy);
            }
            let export_id = required_string(params, "exportId")?;
            let sql = required_string(params, "sql")?;
            let options: tarik_engine_protocol::ExportOptions = serde_json::from_value(
                params
                    .get("options")
                    .cloned()
                    .ok_or_else(|| EngineError::MissingField("options".into()))?,
            )?;
            let connection = sessions.get(&session_id)?.try_clone()?;
            exports.execute(&session_id, &export_id, &sql, connection, options)?;
            Ok(serde_json::json!({
                "exportId": export_id,
                "state": "queued",
            }))
        }
        "export.status" => {
            let export_id = required_string(params, "exportId")?;
            Ok(serde_json::to_value(exports.status(&export_id)?)?)
        }
        "export.cancel" => {
            let export_id = required_string(params, "exportId")?;
            Ok(serde_json::to_value(exports.cancel(&export_id)?)?)
        }
        "query.status" => {
            let execution_id = required_string(params, "executionId")?;
            let status = jobs.status(&execution_id)?;
            Ok(serde_json::to_value(status)?)
        }
        "query.cancel" => {
            let execution_id = required_string(params, "executionId")?;
            let status = jobs.cancel(&execution_id)?;
            Ok(serde_json::to_value(status)?)
        }
        "result.get_page" => {
            let result_id = required_string(params, "resultId")?;
            let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let max_rows = params
                .get("maxRows")
                .and_then(Value::as_u64)
                .unwrap_or(u64::from(pages::DEFAULT_PAGE_ROWS))
                .clamp(1, 5_000) as u32;
            let (result, page) = jobs.get_page(&result_id, offset, max_rows)?;
            Ok(serde_json::json!({
                "resultId": result_id,
                "offset": offset,
                "rowTotal": result.row_count,
                "rowTotalExact": result.row_count_exact,
                "columns": result.columns,
                "rows": page.values,
                "truncatedCells": page
                    .truncated_cells
                    .iter()
                    .map(|(r, c)| [r, c])
                    .collect::<Vec<_>>(),
            }))
        }
        "result.release" => {
            let result_id = required_string(params, "resultId")?;
            jobs.release_result(&result_id)?;
            Ok(Value::Null)
        }
        "result.release_all" => Ok(Value::from(jobs.release_all_results()?)),
        _ => Err(EngineError::MethodNotFound(request.method.clone())),
    }
}

fn main() {
    let mut sessions = session::SessionManager::new();
    let jobs = std::sync::Arc::new(jobs::JobRegistry::new());
    let exports = std::sync::Arc::new(export_jobs::ExportRegistry::new());
    let profiles = std::sync::Arc::new(profile::ProfileRegistry::new());
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
            Ok(request) => match dispatch(&request, &mut sessions, &jobs, &exports, &profiles) {
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
        assert!(info.capabilities.data_profiling);
    }

    #[test]
    fn unknown_method_has_stable_code() {
        let request = RequestEnvelope {
            id: "r1".into(),
            method: "not.real".into(),
            params: serde_json::Map::new(),
        };
        let error = dispatch(
            &request,
            &mut session::SessionManager::new(),
            &std::sync::Arc::new(jobs::JobRegistry::new()),
            &std::sync::Arc::new(export_jobs::ExportRegistry::new()),
            &std::sync::Arc::new(profile::ProfileRegistry::new()),
        )
        .unwrap_err();
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
        let error = dispatch(
            &request,
            &mut session::SessionManager::new(),
            &std::sync::Arc::new(jobs::JobRegistry::new()),
            &std::sync::Arc::new(export_jobs::ExportRegistry::new()),
            &std::sync::Arc::new(profile::ProfileRegistry::new()),
        )
        .unwrap_err();
        assert_eq!(error.code(), "source.invalid_options");
    }
}

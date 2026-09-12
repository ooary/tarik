use std::{
    io::{BufRead, BufReader, BufWriter, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value};

struct Engine {
    child: Child,
    stdin: BufWriter<std::process::ChildStdin>,
    stdout: BufReader<std::process::ChildStdout>,
    cache_dir: PathBuf,
}

#[cfg(unix)]
fn duckdb_lib_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../../target/duckdb-download/x86_64-unknown-linux-gnu/1.5.5")
        .canonicalize()
        .unwrap_or_else(|_| manifest.join("../../target/duckdb-download"))
}

fn spawn_engine() -> Engine {
    let bin = env!("CARGO_BIN_EXE_tarik-engine-duckdb");
    let mut command = Command::new(bin);
    #[cfg(unix)]
    command.env("LD_LIBRARY_PATH", duckdb_lib_dir());
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn engine");
    let stdin = BufWriter::new(child.stdin.take().unwrap());
    let stdout = BufReader::new(child.stdout.take().unwrap());
    Engine {
        child,
        stdin,
        stdout,
        cache_dir: temp_path("results", ""),
    }
}

impl Engine {
    fn request(&mut self, method: &str, mut params: Value) -> Value {
        if method == "query.execute" {
            params
                .as_object_mut()
                .expect("request parameters are an object")
                .entry("cacheDir")
                .or_insert_with(|| json!(self.cache_dir));
        }
        let request = json!({
            "id": method,
            "method": method,
            "params": params,
        });
        writeln!(self.stdin, "{request}").unwrap();
        self.stdin.flush().unwrap();

        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();
            if line.trim().is_empty() {
                continue;
            }
            let response: Value = serde_json::from_str(&line).unwrap();
            if response["id"] == method {
                return response;
            }
        }
    }

    fn assert_ok(&mut self, method: &str, params: Value) -> Value {
        let response = self.request(method, params);
        assert!(
            response["ok"].as_bool().unwrap(),
            "{method} failed: {}",
            response["error"]
        );
        response["result"].clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.cache_dir);
    }
}

fn temp_path(name: &str, suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("tarik-engine-{name}-{stamp}{suffix}"))
}

fn exercise_native_paths(root: &std::path::Path) {
    let mut deep = root.to_path_buf();
    for index in 0..6 {
        deep.push(format!(
            "long segment {index} with spaces and enough characters 0123456789"
        ));
    }
    std::fs::create_dir_all(&deep).unwrap();
    assert!(
        deep.as_os_str().len() > 260,
        "fixture must exercise a path beyond the legacy Windows limit"
    );
    let database = deep.join("analisis_日本語.duckdb");
    let source = root.join("pesanan café read only.csv");
    let output = root.join("hasil ekspor_日本語");
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(&source, "id,amount\n1,12.5\n2,30.0\n").unwrap();
    let mut permissions = std::fs::metadata(&source).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&source, permissions).unwrap();

    let mut engine = spawn_engine();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "native-paths",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "duckdb.source.import_table",
        json!({
            "sessionId": "native-paths",
            "projectId": "native-project",
            "path": source,
            "options": {
                "tableName": "pesanan_日本語",
                "csv": { "delimiter": ",", "hasHeader": true, "nullValue": null, "allVarchar": false },
                "columnOverrides": []
            }
        }),
    );
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "native-paths",
            "executionId": "native-path-query",
            "sql": "SELECT sum(amount) AS total FROM \"pesanan_日本語\""
        }),
    );
    let status = poll_terminal(&mut engine, "native-path-query");
    assert_eq!(status["state"], "succeeded");
    let page = engine.assert_ok(
        "result.get_page",
        json!({ "resultId": "native-path-query", "offset": 0, "maxRows": 10 }),
    );
    assert_eq!(page["rows"], json!([[42.5]]));

    engine.assert_ok(
        "export.execute",
        json!({
            "sessionId": "native-paths",
            "exportId": "native-path-export",
            "sql": "SELECT * FROM \"pesanan_日本語\" ORDER BY id",
            "options": {
                "format": "csv",
                "outputDirectory": output,
                "baseName": "orders",
                "rowsPerPart": 10,
                "overwrite": "fail_if_exists",
                "csv": { "delimiter": ",", "includeHeader": true },
                "parquet": null
            }
        }),
    );
    let exported = poll_export_terminal(&mut engine, "native-path-export", 400);
    assert_eq!(exported["state"], "succeeded");
    assert_eq!(exported["rowsWritten"], 2);
    assert!(output.join("orders-part-00001.csv").is_file());
    engine.assert_ok("session.close", json!({ "sessionId": "native-paths" }));
    #[cfg(windows)]
    {
        let mut permissions = std::fs::metadata(&source).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        std::fs::set_permissions(&source, permissions).unwrap();
    }
}

#[test]
fn spaces_unicode_long_paths_and_read_only_sources_round_trip() {
    let root = temp_path("native paths 日本語", "");
    std::fs::create_dir_all(&root).unwrap();
    exercise_native_paths(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(windows)]
#[test]
fn unc_project_source_and_export_paths_round_trip() {
    let Some(root) = std::env::var_os("TARIK_TEST_UNC_ROOT") else {
        eprintln!("TARIK_TEST_UNC_ROOT is not configured; skipping native UNC test");
        return;
    };
    let root = PathBuf::from(root).join(format!("case-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    exercise_native_paths(&root);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn handshake_reports_duckdb_capabilities() {
    let mut engine = spawn_engine();
    let result = engine.assert_ok("engine.handshake", json!({}));
    assert_eq!(result["engineId"], "duckdb");
    assert_eq!(result["protocolVersion"], 2);
    assert_eq!(result["capabilities"]["linkParquet"], true);
    assert_eq!(result["capabilities"]["importCsv"], true);
    assert_eq!(result["capabilities"]["dataProfiling"], true);
    engine.child.kill().ok();
}

#[test]
fn session_open_import_catalog_close_roundtrip() {
    let mut engine = spawn_engine();
    let database = temp_path("session", ".duckdb");
    let csv = temp_path("orders", ".csv");
    std::fs::write(&csv, "id,amount\n1,12.5\n2,30.0\n").unwrap();

    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "s1",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    let imported = engine.assert_ok(
        "duckdb.source.import_table",
        json!({
            "sessionId": "s1",
            "projectId": "p1",
            "path": csv,
            "options": {
                "tableName": "orders",
                "csv": { "delimiter": ",", "hasHeader": true, "nullValue": null, "allVarchar": false },
                "columnOverrides": []
            }
        }),
    );
    assert_eq!(imported["duckdbName"], "orders");
    assert_eq!(imported["options"]["rowCount"], 2);

    let catalog = engine.assert_ok("catalog.inspect", json!({ "sessionId": "s1" }));
    let objects = catalog["objects"].as_array().unwrap();
    assert!(objects
        .iter()
        .any(|o| o["name"] == "orders" && o["kind"] == "table"));

    // Reopening the same session id must replace the stale session, not fail.
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "s1",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok("session.close", json!({ "sessionId": "s1" }));

    let _ = std::fs::remove_file(&database);
    let _ = std::fs::remove_file(&csv);
    let csv = temp_path("inspect-null", ".csv");
    std::fs::write(&csv, "id,amount\n1,12.5\n").unwrap();

    // JSON null for csv options must deserialize as "no CSV options".
    let inspected = engine.assert_ok("source.inspect", json!({ "path": csv, "csv": null }));
    assert_eq!(inspected["format"], "csv");
    assert_eq!(inspected["columns"].as_array().unwrap().len(), 2);

    let _ = std::fs::remove_file(&csv);
    engine.child.kill().ok();
}

#[test]
fn catalog_drop_object_deletes_tables_and_views_with_quoted_names() {
    let mut engine = spawn_engine();
    let database = temp_path("drop-object", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "drop-session",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "drop-session",
            "executionId": "drop-setup",
            "sql": "CREATE TABLE \"order items\" (id INTEGER); CREATE VIEW \"order view\" AS SELECT * FROM \"order items\";",
                    }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "drop-setup")["state"],
        "succeeded"
    );
    let catalog = engine.assert_ok("catalog.inspect", json!({ "sessionId": "drop-session" }));
    let objects = catalog["objects"].as_array().unwrap();
    let table = objects
        .iter()
        .find(|object| object["name"] == "order items")
        .unwrap();
    assert!(objects
        .iter()
        .any(|object| object["name"] == "order view" && object["kind"] == "view"));
    let database_name = table["database"].as_str().unwrap();

    engine.assert_ok(
        "catalog.drop_object",
        json!({
            "sessionId": "drop-session",
            "database": database_name,
            "schema": "main",
            "name": "order view",
            "kind": "view",
        }),
    );
    engine.assert_ok(
        "catalog.drop_object",
        json!({
            "sessionId": "drop-session",
            "database": database_name,
            "schema": "main",
            "name": "order items",
            "kind": "table",
        }),
    );
    let catalog = engine.assert_ok("catalog.inspect", json!({ "sessionId": "drop-session" }));
    let objects = catalog["objects"].as_array().unwrap();
    assert!(!objects.iter().any(|object| object["name"] == "order items"));
    assert!(!objects.iter().any(|object| object["name"] == "order view"));

    // The wrong kind cannot silently delete the remaining relation type.
    let response = engine.request(
        "catalog.drop_object",
        json!({
            "sessionId": "drop-session",
            "database": database_name,
            "schema": "main",
            "name": "not-there",
            "kind": "invalid",
        }),
    );
    assert_eq!(response["error"]["code"], "source.invalid_options");

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
}

#[test]
fn unknown_method_and_missing_session_surface_structured_errors() {
    let mut engine = spawn_engine();

    let response = engine.request("not.real", json!({}));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "method.not_found");

    let response = engine.request("catalog.inspect", json!({ "sessionId": "missing" }));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "session.missing");

    engine.child.kill().ok();
}

#[test]
fn query_execute_reaches_terminal_state_and_persists_rows() {
    let mut engine = spawn_engine();
    let database = temp_path("query", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "qs",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    // Row-returning statement: execute returns queued, then status terminal.
    let enqueued = engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "e1", "sql": "SELECT i FROM range(1, 11) t(i);" }),
    );
    assert_eq!(enqueued["state"], "queued");

    let status = poll_terminal(&mut engine, "e1");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsProduced"], 10);

    // DML and DDL without a row set succeed with no produced rows.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "e2", "sql": "CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1), (2), (3);" }),
    );
    let status = poll_terminal(&mut engine, "e2");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsAffected"], 3);
    assert_eq!(status["rowsProduced"], Value::Null);

    // Multi-statement snapshots execute sequentially; the last row set wins.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "e3", "sql": "CREATE TABLE t2 (a INTEGER); INSERT INTO t2 VALUES (1), (2), (3); SELECT count(*) AS n FROM t2;" }),
    );
    let status = poll_terminal(&mut engine, "e3");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsProduced"], 1);
    assert_eq!(status["rowsAffected"], 3);

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
}

#[test]
fn queued_query_claims_a_connection_after_preceding_catalog_changes() {
    let mut engine = spawn_engine();
    let database = temp_path("claim-catalog", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "claim-catalog",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "claim-catalog",
            "executionId": "catalog-ddl",
            "sql": "CREATE TABLE claimed_at_run AS SELECT 42 AS answer"
        }),
    );
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "claim-catalog",
            "executionId": "catalog-reader",
            "sql": "SELECT answer FROM claimed_at_run"
        }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "catalog-ddl")["state"],
        "succeeded"
    );
    let reader = poll_terminal(&mut engine, "catalog-reader");
    assert_eq!(reader["state"], "succeeded", "{reader}");
    assert_eq!(reader["rowsProduced"], 1);

    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
}

#[test]
fn quality_read_only_validation_binds_without_executing_or_allowing_external_effects() {
    let mut engine = spawn_engine();
    let database = temp_path("quality-validation", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "quality-validation",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    let accepted = engine.assert_ok(
        "quality.validate_read_only",
        json!({ "sessionId": "quality-validation", "sql": "WITH failures AS (SELECT 1 WHERE false) SELECT * FROM failures" }),
    );
    assert_eq!(accepted["readOnly"], true);
    for sql in [
        "SELECT 1; SELECT 2",
        "DELETE FROM missing",
        "WITH changed AS (DELETE FROM missing RETURNING *) SELECT * FROM changed",
        "SELECT * FROM read_parquet('outside.parquet')",
        "COPY (SELECT 1) TO 'outside.csv'",
    ] {
        let response = engine.request(
            "quality.validate_read_only",
            json!({ "sessionId": "quality-validation", "sql": sql }),
        );
        assert_eq!(response["ok"], false, "unexpectedly accepted {sql}");
        assert_eq!(response["error"]["code"], "quality.sql_unsafe");
    }
    engine.assert_ok(
        "session.close",
        json!({ "sessionId": "quality-validation" }),
    );
    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
}

#[test]
fn query_validation_reports_parse_bind_errors_without_executing_mutations() {
    let mut engine = spawn_engine();
    let database = temp_path("validate", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "vs",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "vs",
            "executionId": "validate-setup",
            "sql": "CREATE TABLE sentinel(id INTEGER, amount INTEGER); INSERT INTO sentinel VALUES (1, 10);",
                    }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "validate-setup")["state"],
        "succeeded"
    );

    let valid = engine.assert_ok(
        "query.validate",
        json!({
            "sessionId": "vs",
            "revision": 41,
            "sql": "CREATE TABLE must_not_exist(i INTEGER); INSERT INTO sentinel VALUES (2, 20); UPDATE sentinel SET amount = 99; DELETE FROM sentinel;",
        }),
    );
    assert_eq!(valid["revision"], 41);
    let warnings = valid["diagnostics"].as_array().unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(warnings
        .iter()
        .all(|diagnostic| diagnostic["severity"] == "warning"));
    assert_eq!(warnings[0]["code"], "sql.mutation_without_where");
    assert_eq!(warnings[1]["code"], "sql.mutation_without_where");

    let catalog = engine.assert_ok("catalog.inspect", json!({ "sessionId": "vs" }));
    assert!(!catalog["objects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|object| object["name"] == "must_not_exist"));
    engine.assert_ok(
        "query.execute",
        json!({
        "sessionId": "vs",
        "executionId": "validate-check",
        "sql": "SELECT id, amount FROM sentinel ORDER BY id",
                }),
    );
    let status = poll_terminal(&mut engine, "validate-check");
    assert_eq!(status["rowsProduced"], 1);
    let page = engine.assert_ok(
        "result.get_page",
        json!({ "resultId": "validate-check", "offset": 0, "maxRows": 10 }),
    );
    assert_eq!(page["rows"], json!([[1, 10]]));

    let invalid = engine.assert_ok(
        "query.validate",
        json!({
            "sessionId": "vs",
            "revision": 42,
            "sql": "SELECT missing FROM sentinel; SELECT * FORM sentinel",
        }),
    );
    assert_eq!(invalid["revision"], 42);
    let diagnostics = invalid["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0]["code"], "sql.bind");
    assert_eq!(diagnostics[1]["code"], "sql.syntax");
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic["severity"] == "error"));
    assert!(diagnostics
        .iter()
        .all(|diagnostic| diagnostic["from"].is_number()));

    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
}

#[test]
fn query_execute_reports_structured_sql_errors() {
    let mut engine = spawn_engine();
    let database = temp_path("queryerr", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "qs",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    let enqueued = engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "bad", "sql": "SELECT FROM WHERE" }),
    );
    assert_eq!(enqueued["state"], "queued");

    let mut status =
        engine.request("query.status", json!({ "executionId": "bad" }))["result"].clone();
    for _ in 0..200 {
        if status["state"] == "failed" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        status = engine.request("query.status", json!({ "executionId": "bad" }))["result"].clone();
    }
    assert_eq!(status["state"], "failed");
    assert_eq!(status["error"]["code"], "duckdb.error");
    assert!(status["error"]["message"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains("select"));

    // Empty SQL (only comments) is rejected with a structured error.
    let response = engine.request(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "e0", "sql": "  ; -- nothing" }),
    );
    assert_eq!(response["error"]["code"], "query.invalid");

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
}

fn poll_terminal(engine: &mut Engine, execution_id: &str) -> Value {
    poll_terminal_with_timeout(engine, execution_id, 200)
}

fn poll_profile_terminal(engine: &mut Engine, profile_id: &str, attempts: usize) -> Value {
    for _ in 0..attempts {
        let status =
            engine.request("profile.status", json!({ "profileId": profile_id }))["result"].clone();
        let state = status["state"].as_str().unwrap_or_default();
        if state == "succeeded" || state == "failed" || state == "cancelled" {
            return status;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("profile {profile_id} did not reach a terminal state");
}

fn poll_export_terminal(engine: &mut Engine, export_id: &str, attempts: usize) -> Value {
    for _ in 0..attempts {
        let status =
            engine.request("export.status", json!({ "exportId": export_id }))["result"].clone();
        let state = status["state"].as_str().unwrap_or_default();
        if state == "succeeded" || state == "failed" || state == "cancelled" {
            return status;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("export {export_id} did not reach a terminal state");
}

fn csv_export_options(output_directory: &std::path::Path, rows_per_part: u64) -> Value {
    json!({
        "format": "csv",
        "outputDirectory": output_directory,
        "baseName": "orders",
        "rowsPerPart": rows_per_part,
        "overwrite": "fail_if_exists",
        "csv": { "delimiter": ",", "includeHeader": true },
        "parquet": null
    })
}

fn poll_terminal_with_timeout(engine: &mut Engine, execution_id: &str, attempts: usize) -> Value {
    for _ in 0..attempts {
        let status = engine.request("query.status", json!({ "executionId": execution_id }))
            ["result"]
            .clone();
        if !status.is_null() {
            let state = status["state"].as_str().unwrap_or_default();
            if state == "succeeded" || state == "failed" || state == "cancelled" {
                return status;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("execution {execution_id} did not reach a terminal state");
}

const LONG_QUERY: &str = "SELECT count(*) FROM range(1_000_000_000_000) t(i);";

#[test]
fn cancel_covers_queued_active_and_session_reuse() {
    let mut engine = spawn_engine();
    let database = temp_path("cancel", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "cs",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    // 1. A long-running query occupies the session worker.
    let enqueued = engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "cs", "executionId": "c1", "sql": LONG_QUERY }),
    );
    assert_eq!(enqueued["state"], "queued");

    // 2. A second submission on the same session queues behind the first.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "cs", "executionId": "c2", "sql": "SELECT 41 + 1 AS answer;" }),
    );
    let queued_status =
        engine.request("query.status", json!({ "executionId": "c2" }))["result"].clone();
    assert_eq!(queued_status["state"], "queued");

    // 3. Cancelling the queued job removes it before it starts.
    let cancelled =
        engine.request("query.cancel", json!({ "executionId": "c2" }))["result"].clone();
    assert_eq!(cancelled["state"], "cancelled");

    // 4. Cancelling the active job interrupts DuckDB and becomes terminal.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let interrupted =
        engine.request("query.cancel", json!({ "executionId": "c1" }))["result"].clone();
    assert_eq!(interrupted["state"], "running");
    let status = poll_terminal_with_timeout(&mut engine, "c1", 400);
    assert_eq!(status["state"], "cancelled");
    assert_eq!(status["error"], serde_json::Value::Null);

    // 5. The same session accepts and completes a later query.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "cs", "executionId": "c3", "sql": "SELECT 6 * 7 AS answer;" }),
    );
    let status = poll_terminal_with_timeout(&mut engine, "c3", 400);
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsProduced"], 1);

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
}

#[test]
fn repeat_cancel_on_terminal_job_is_idempotent() {
    let mut engine = spawn_engine();
    let database = temp_path("cancel2", ".duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "cs",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "cs", "executionId": "k1", "sql": "SELECT 1;" }),
    );
    let status = poll_terminal(&mut engine, "k1");
    assert_eq!(status["state"], "succeeded");

    // Cancelling an already-finished job returns its terminal state, and
    // repeated cancels keep returning it unchanged.
    for _ in 0..2 {
        let repeat =
            engine.request("query.cancel", json!({ "executionId": "k1" }))["result"].clone();
        assert_eq!(repeat["state"], "succeeded");
    }

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
}

#[test]
fn paging_reads_windows_across_pages_and_release_removes_artifacts() {
    let mut engine = spawn_engine();
    let database = temp_path("pages", ".duckdb");
    let cache_dir = temp_path("pages-cache", "");
    std::fs::create_dir_all(&cache_dir).unwrap();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "ps",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    // 4999 rows -> 10 pages of 500 rows with a partial final page.
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "ps",
            "executionId": "p1",
            "sql": "SELECT i, 'label-' || (i % 7) AS label FROM range(1, 5000) t(i);",
            "cacheDir": cache_dir,
        }),
    );
    let status = poll_terminal(&mut engine, "p1");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["result"]["rowCount"], 4999);
    assert_eq!(status["result"]["rowId"], Value::Null); // unknown fields ignored
    let columns = status["result"]["columns"].as_array().unwrap();
    assert_eq!(columns.len(), 2);
    assert_eq!(columns[0]["name"], "i");
    assert_eq!(columns[0]["logicalType"], "integer");

    // First page: 500 rows starting at i = 1.
    let page = engine
        .assert_ok("result.get_page", json!({ "resultId": "p1", "offset": 0 }))
        .clone();
    assert_eq!(page["rows"].as_array().unwrap().len(), 500);
    assert_eq!(page["rows"][0][0], 1);
    assert_eq!(page["rowTotal"], 4999);

    // A window that straddles a page boundary stitches both files together.
    let page = engine
        .assert_ok(
            "result.get_page",
            json!({ "resultId": "p1", "offset": 490 }),
        )
        .clone();
    let rows = page["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 500);
    assert_eq!(rows[0][0], 491);
    assert_eq!(rows[499][0], 990);

    // The final partial page returns the remaining rows.
    let page = engine
        .assert_ok(
            "result.get_page",
            json!({ "resultId": "p1", "offset": 4500 }),
        )
        .clone();
    let rows = page["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 499);
    assert_eq!(rows[498][0], 4999);

    // Releasing removes the artifacts; further page reads fail.
    engine.assert_ok("result.release", json!({ "resultId": "p1" }));
    assert_eq!(
        engine.request("result.get_page", json!({ "resultId": "p1", "offset": 0 }))["error"]
            ["code"],
        "result.missing"
    );

    // Wide rows shrink the page size to stay under the byte target.
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "ps",
            "executionId": "p2",
            "sql": "SELECT repeat('x', 20000) AS wide FROM range(1, 3000) t(i);",
            "cacheDir": cache_dir,
        }),
    );
    let status = poll_terminal(&mut engine, "p2");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["result"]["rowCount"], 2999);
    let _ = engine.assert_ok("result.get_page", json!({ "resultId": "p2", "offset": 0 }));
    // Every page artifact stays under the byte target even with 20 KB rows.
    let result_dir = cache_dir.join("p2");
    for entry in std::fs::read_dir(&result_dir).unwrap().flatten() {
        let size = entry.metadata().unwrap().len();
        assert!(size <= 5 * 1024 * 1024, "page artifact too large: {size}");
    }

    // A failed query leaves no result artifacts behind.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "ps", "executionId": "p3", "sql": "SELECT not_a_column", "cacheDir": cache_dir }),
    );
    let status = poll_terminal(&mut engine, "p3");
    assert_eq!(status["state"], "failed");
    let leftovers: Vec<_> = std::fs::read_dir(&cache_dir)
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temporary page dirs leaked");

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn profile_protocol_is_bounded_truthful_stale_safe_and_leaves_no_result_artifacts() {
    let mut engine = spawn_engine();
    let root = temp_path("profile", "");
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("profile.duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "profile-session",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "profile-session",
            "executionId": "profile-fixture",
            "sql": "CREATE TABLE \"odd table\" AS SELECT i % 7 AS id, CASE WHEN i % 5 = 0 THEN NULL ELSE 'label-' || i END AS label FROM range(1000) t(i)",
            "cacheDir": root.join("results")
        }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "profile-fixture")["state"],
        "succeeded"
    );
    let catalog = engine.assert_ok("catalog.inspect", json!({ "sessionId": "profile-session" }));
    let target = catalog["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|object| object["name"] == "odd table")
        .unwrap()
        .clone();
    let request = json!({
        "projectId": "project-1",
        "target": target,
        "columns": [
            { "name": "id", "dataType": "BIGINT" },
            { "name": "label", "dataType": "VARCHAR" }
        ],
        "catalogRevision": catalog["revision"],
        "mode": "approximate"
    });
    let queued = engine.assert_ok(
        "profile.execute",
        json!({ "sessionId": "profile-session", "profileId": "profile-1", "request": request }),
    );
    assert_eq!(queued["state"], "queued");
    let status = poll_profile_terminal(&mut engine, "profile-1", 400);
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["snapshot"]["metrics"][0]["kind"], "row_count");
    assert_eq!(status["snapshot"]["metrics"][0]["value"], 1000);
    let statements = status["snapshot"]["statements"].as_array().unwrap();
    assert_eq!(statements.len(), 6);
    assert_eq!(statements[0]["metricKinds"][0], "row_count");
    assert!(statements.iter().all(|statement| statement["sql"]
        .as_str()
        .is_some_and(|sql| sql.starts_with("SELECT "))));
    assert!(statements.iter().any(|statement| statement["sql"]
        .as_str()
        .is_some_and(|sql| sql.contains("approx_count_distinct(\"id\")"))));
    assert!(status["snapshot"]["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|metric| {
            metric["column"] == "id"
                && metric["kind"] == "distinct_count"
                && metric["provenance"] == "approximate"
        }));
    let lists = status["snapshot"]["metrics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|metric| {
            metric["kind"] == "common_values" || metric["kind"] == "representative_values"
        });
    for metric in lists {
        assert!(metric["value"].as_array().unwrap().len() <= 20);
    }
    assert!(!root.join("results/profile-1").exists());

    let stale = engine.request(
        "profile.execute",
        json!({
            "sessionId": "profile-session",
            "profileId": "profile-stale",
            "request": {
                "projectId": "project-1",
                "target": { "database": "profile", "schema": "main", "name": "odd table", "kind": "table" },
                "columns": [{ "name": "id", "dataType": "BIGINT" }],
                "catalogRevision": "stale-revision",
                "mode": "exact"
            }
        }),
    );
    assert_eq!(stale["ok"], true);
    let stale_status = poll_profile_terminal(&mut engine, "profile-stale", 400);
    assert_eq!(stale_status["state"], "failed");
    assert_eq!(stale_status["error"]["code"], "profile.catalog_stale");

    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "profile-session",
            "executionId": "after-profile",
            "sql": "SELECT 42",
            "cacheDir": root.join("results")
        }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "after-profile")["state"],
        "succeeded"
    );
    engine.assert_ok("session.close", json!({ "sessionId": "profile-session" }));
    engine.child.kill().ok();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn active_profile_cancels_without_residue_and_the_session_remains_usable() {
    let mut engine = spawn_engine();
    let root = temp_path("profile-cancel", "");
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("cancel.duckdb");
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "profile-cancel-session",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "profile-cancel-session",
            "executionId": "profile-cancel-fixture",
            "sql": "CREATE VIEW huge_profile AS SELECT i AS value FROM range(1000000000000) t(i)",
            "cacheDir": root.join("results")
        }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "profile-cancel-fixture")["state"],
        "succeeded"
    );
    let catalog = engine.assert_ok(
        "catalog.inspect",
        json!({ "sessionId": "profile-cancel-session" }),
    );
    let target = catalog["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|object| object["name"] == "huge_profile")
        .unwrap()
        .clone();
    engine.assert_ok(
        "profile.execute",
        json!({
            "sessionId": "profile-cancel-session",
            "profileId": "cancel-profile",
            "request": {
                "projectId": "project-1",
                "target": target,
                "columns": [{ "name": "value", "dataType": "BIGINT" }],
                "catalogRevision": catalog["revision"],
                "mode": "exact"
            }
        }),
    );
    std::thread::sleep(std::time::Duration::from_millis(30));
    let cancelled = engine.assert_ok("profile.cancel", json!({ "profileId": "cancel-profile" }));
    assert!(cancelled["state"] == "running" || cancelled["state"] == "cancelled");
    let status = poll_profile_terminal(&mut engine, "cancel-profile", 800);
    assert_eq!(status["state"], "cancelled");
    assert_eq!(status["snapshot"], Value::Null);
    assert!(!root.join("results/cancel-profile").exists());

    engine.assert_ok(
        "query.execute",
        json!({
            "sessionId": "profile-cancel-session",
            "executionId": "after-profile-cancel",
            "sql": "SELECT 42",
            "cacheDir": root.join("results")
        }),
    );
    assert_eq!(
        poll_terminal(&mut engine, "after-profile-cancel")["state"],
        "succeeded"
    );
    engine.assert_ok(
        "session.close",
        json!({ "sessionId": "profile-cancel-session" }),
    );
    engine.child.kill().ok();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn resource_protocol_applies_reads_back_and_survives_connection_clones() {
    let mut engine = spawn_engine();
    let root = temp_path("resources", "");
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("resources.duckdb");
    let session_id = "resource-session";

    let opened = engine.request(
        "session.open",
        json!({
            "sessionId": session_id,
            "locator": { "engineId": "duckdb", "payload": { "path": database } },
            "resources": { "preset": "low_memory", "memoryLimitMib": 512, "threads": 1 }
        }),
    )["result"]
        .clone();
    assert_eq!(opened["memoryLimitMib"], 512);
    assert_eq!(opened["threads"], 1);

    let configured = engine.request(
        "session.configure",
        json!({
            "sessionId": session_id,
            "resources": { "preset": "custom", "memoryLimitMib": 768, "threads": 2 }
        }),
    )["result"]
        .clone();
    assert_eq!(configured["preset"], "custom");
    assert_eq!(configured["memoryLimitMib"], 768);
    assert_eq!(configured["threads"], 2);

    let execution_id = "resource-readback-query";
    engine.request(
        "query.execute",
        json!({
            "sessionId": session_id,
            "executionId": execution_id,
            "sql": "SELECT current_setting('memory_limit'), current_setting('threads')",
            "cacheDir": root.join("results")
        }),
    );
    let status = poll_terminal(&mut engine, execution_id);
    assert_eq!(status["state"], "succeeded");
    let page = engine.request(
        "result.get_page",
        json!({ "resultId": execution_id, "offset": 0, "maxRows": 1 }),
    );
    assert!(page["result"]["rows"][0][0]
        .as_str()
        .is_some_and(|value| value.contains("768")));
    assert_eq!(page["result"]["rows"][0][1], 2);

    let output = root.join("export");
    std::fs::create_dir(&output).unwrap();
    engine.request(
        "export.execute",
        json!({
            "sessionId": session_id,
            "exportId": "resource-readback-export",
            "sql": "SELECT current_setting('memory_limit') AS memory, current_setting('threads') AS threads",
            "options": {
                "format": "csv",
                "outputDirectory": output,
                "baseName": "resources",
                "rowsPerPart": 10,
                "overwrite": "fail_if_exists",
                "csv": { "delimiter": ",", "includeHeader": true },
                "parquet": null
            }
        }),
    );
    let export = poll_export_terminal(&mut engine, "resource-readback-export", 200);
    assert_eq!(export["state"], "succeeded");
    let csv = std::fs::read_to_string(output.join("resources-part-00001.csv")).unwrap();
    assert!(csv.contains("768"));
    assert!(csv.lines().last().is_some_and(|line| line.ends_with(",2")));

    engine.request("session.close", json!({ "sessionId": session_id }));
    engine.child.kill().ok();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn resource_change_is_rejected_while_session_work_is_active() {
    let mut engine = spawn_engine();
    let root = temp_path("resource-busy", "");
    std::fs::create_dir_all(&root).unwrap();
    let database = root.join("busy.duckdb");
    let session_id = "resource-busy-session";
    engine.request(
        "session.open",
        json!({
            "sessionId": session_id,
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.request(
        "query.execute",
        json!({
            "sessionId": session_id,
            "executionId": "busy-resource-query",
            "sql": "SELECT sum(i) FROM range(1000000000) t(i)",
            "cacheDir": root.join("results")
        }),
    );
    let response = engine.request(
        "session.configure",
        json!({
            "sessionId": session_id,
            "resources": { "preset": "low_memory", "memoryLimitMib": 512, "threads": 1 }
        }),
    );
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"]["code"], "resources.busy");
    engine.request(
        "query.cancel",
        json!({ "executionId": "busy-resource-query" }),
    );
    poll_terminal_with_timeout(&mut engine, "busy-resource-query", 900);
    engine.request("session.close", json!({ "sessionId": session_id }));
    engine.child.kill().ok();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn export_protocol_streams_exact_csv_parts_with_bounded_status() {
    let mut engine = spawn_engine();
    let database = temp_path("export", ".duckdb");
    let output = temp_path("export-output", "");
    std::fs::create_dir(&output).unwrap();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "es",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    let queued = engine.assert_ok(
        "export.execute",
        json!({
            "sessionId": "es",
            "exportId": "x1",
            "sql": "SELECT i, i * 2 AS doubled FROM range(1, 9) t(i)",
            "options": csv_export_options(&output, 3),
        }),
    );
    assert_eq!(queued["state"], "queued");
    let status = poll_export_terminal(&mut engine, "x1", 400);
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsWritten"], 8);
    assert_eq!(status["filesWritten"], 3);
    assert!(status["bytesWritten"].as_u64().unwrap() > 0);
    assert_eq!(status["currentPart"], Value::Null);
    let parts = status["completedParts"].as_array().unwrap();
    assert_eq!(parts.len(), 3);
    assert_eq!(parts[0]["rows"], 3);
    assert_eq!(parts[2]["rows"], 2);
    assert!(output.join("orders-part-00001.csv").is_file());
    assert!(!std::fs::read_dir(&output)
        .unwrap()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".tarik-export-")));

    // Cancelling a terminal export is idempotent and does not delete files.
    let repeat = engine.assert_ok("export.cancel", json!({ "exportId": "x1" }));
    assert_eq!(repeat["state"], "succeeded");
    assert!(output.join("orders-part-00001.csv").is_file());

    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
    let _ = std::fs::remove_dir_all(output);
}

#[test]
fn delegated_export_byte_quota_fails_before_publication() {
    let mut engine = spawn_engine();
    let database = temp_path("export-quota", ".duckdb");
    let output = temp_path("export-quota-output", "");
    std::fs::create_dir(&output).unwrap();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "eq",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "export.execute",
        json!({
            "sessionId": "eq",
            "exportId": "quota-1",
            "sql": "SELECT i, lpad('x', 100, 'x') AS payload FROM range(0, 10) t(i)",
            "options": csv_export_options(&output, 10),
            "maximumTotalBytes": 1,
        }),
    );
    let status = poll_export_terminal(&mut engine, "quota-1", 400);
    assert_eq!(status["state"], "failed");
    assert_eq!(status["error"]["code"], "export.quota_exceeded");
    assert_eq!(status["rowsWritten"], 0);
    assert_eq!(status["filesWritten"], 0);
    assert_eq!(status["bytesWritten"], 0);
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);

    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
    let _ = std::fs::remove_dir_all(output);
}

#[test]
fn export_invalid_options_fail_before_query_or_file_creation() {
    let mut engine = spawn_engine();
    let database = temp_path("export-invalid", ".duckdb");
    let output = temp_path("export-invalid-output", "");
    std::fs::create_dir(&output).unwrap();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "ei",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    let response = engine.request(
        "export.execute",
        json!({
            "sessionId": "ei",
            "exportId": "bad",
            "sql": "CREATE TABLE must_not_exist(i INTEGER)",
            "options": {
                "format": "csv",
                "outputDirectory": output,
                "baseName": "../unsafe",
                "rowsPerPart": 3,
                "overwrite": "fail_if_exists",
                "csv": { "delimiter": ",", "includeHeader": true },
                "parquet": null
            },
        }),
    );
    assert_eq!(response["error"]["code"], "export.invalid_options");
    assert_eq!(std::fs::read_dir(&output).unwrap().count(), 0);
    let catalog = engine.assert_ok("catalog.inspect", json!({ "sessionId": "ei" }));
    assert!(!catalog["objects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|object| object["name"] == "must_not_exist"));

    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
    let _ = std::fs::remove_dir_all(output);
}

#[test]
fn active_export_cancellation_cleans_stage_and_session_remains_usable() {
    let mut engine = spawn_engine();
    let database = temp_path("export-cancel", ".duckdb");
    let output = temp_path("export-cancel-output", "");
    std::fs::create_dir(&output).unwrap();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "ec",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );
    engine.assert_ok(
        "export.execute",
        json!({
            "sessionId": "ec",
            "exportId": "cancel-me",
            "sql": "SELECT i, repeat('x', 1000) AS payload FROM range(1000000000) t(i)",
            "options": csv_export_options(&output, 100_000),
        }),
    );
    // A second export for the session stays queued behind the active export,
    // bounding concurrent Arrow writers and making queued cancellation clean.
    engine.assert_ok(
        "export.execute",
        json!({
            "sessionId": "ec",
            "exportId": "queued-cancel",
            "sql": "SELECT 7 AS value",
            "options": {
                "format": "csv",
                "outputDirectory": output,
                "baseName": "queued",
                "rowsPerPart": 10,
                "overwrite": "fail_if_exists",
                "csv": { "delimiter": ",", "includeHeader": true },
                "parquet": null
            },
        }),
    );
    let queued = engine.assert_ok("export.cancel", json!({ "exportId": "queued-cancel" }));
    assert_eq!(queued["state"], "cancelled");
    assert!(!output.join("queued-part-00001.csv").exists());

    std::thread::sleep(std::time::Duration::from_millis(20));
    let first = engine.assert_ok("export.cancel", json!({ "exportId": "cancel-me" }));
    assert!(first["state"] == "running" || first["state"] == "cancelled");
    let status = poll_export_terminal(&mut engine, "cancel-me", 800);
    assert_eq!(status["state"], "cancelled");
    assert_eq!(status["error"], Value::Null);
    assert!(!std::fs::read_dir(&output)
        .unwrap()
        .flatten()
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".tarik-export-")));

    // The same session can run a later export after interruption.
    engine.assert_ok(
        "export.execute",
        json!({
            "sessionId": "ec",
            "exportId": "after-cancel",
            "sql": "SELECT 42 AS answer",
            "options": {
                "format": "parquet",
                "outputDirectory": output,
                "baseName": "answer",
                "rowsPerPart": 10,
                "overwrite": "fail_if_exists",
                "csv": null,
                "parquet": { "compression": "snappy" }
            },
        }),
    );
    let status = poll_export_terminal(&mut engine, "after-cancel", 400);
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsWritten"], 1);
    assert!(output.join("answer-part-00001.parquet").is_file());

    engine.child.kill().ok();
    let _ = std::fs::remove_file(database);
    let _ = std::fs::remove_dir_all(output);
}

#[test]
fn lifecycle_stress_releases_cursors_without_leaking_directories() {
    let mut engine = spawn_engine();
    let database = temp_path("stress", ".duckdb");
    let cache_dir = temp_path("stress-cache", "");
    std::fs::create_dir_all(&cache_dir).unwrap();
    engine.assert_ok(
        "session.open",
        json!({
            "sessionId": "ss",
            "locator": { "engineId": "duckdb", "payload": { "path": database } }
        }),
    );

    // Repeated run/release cycles must not leak result directories.
    for run in 0..3u32 {
        let execution_id = format!("s{run}");
        engine.assert_ok(
            "query.execute",
            json!({
                "sessionId": "ss",
                "executionId": execution_id,
                "sql": "SELECT i FROM range(1, 1201) t(i);",
                "cacheDir": cache_dir,
            }),
        );
        let status = poll_terminal(&mut engine, &execution_id);
        assert_eq!(status["state"], "succeeded");

        let page = engine
            .assert_ok(
                "result.get_page",
                json!({ "resultId": execution_id, "offset": 1000 }),
            )
            .clone();
        assert_eq!(page["rows"].as_array().unwrap().len(), 200);

        engine.assert_ok("result.release", json!({ "resultId": execution_id }));
        assert_eq!(
            engine.request(
                "result.get_page",
                json!({ "resultId": execution_id, "offset": 0 })
            )["error"]["code"],
            "result.missing"
        );
    }

    // The session still works after the stress cycle.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "ss", "executionId": "final", "sql": "SELECT 1;", "cacheDir": cache_dir }),
    );
    let status = poll_terminal(&mut engine, "final");
    assert_eq!(status["state"], "succeeded");

    // Every released directory is gone; only the final result remains.
    let leftovers: Vec<_> = std::fs::read_dir(&cache_dir).unwrap().collect();
    assert_eq!(leftovers.len(), 1);

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
    let _ = std::fs::remove_dir_all(&cache_dir);
}

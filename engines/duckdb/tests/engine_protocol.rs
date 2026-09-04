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
}

fn duckdb_lib_dir() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .join("../../target/duckdb-download/x86_64-unknown-linux-gnu/1.5.5")
        .canonicalize()
        .unwrap_or_else(|_| manifest.join("../../target/duckdb-download"))
}

fn spawn_engine() -> Engine {
    let bin = env!("CARGO_BIN_EXE_tarik-engine-duckdb");
    let mut child = Command::new(bin)
        .env("LD_LIBRARY_PATH", duckdb_lib_dir())
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
    }
}

impl Engine {
    fn request(&mut self, method: &str, params: Value) -> Value {
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

fn temp_path(name: &str, suffix: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("tarik-engine-{name}-{stamp}{suffix}"))
}

#[test]
fn handshake_reports_duckdb_capabilities() {
    let mut engine = spawn_engine();
    let result = engine.assert_ok("engine.handshake", json!({}));
    assert_eq!(result["engineId"], "duckdb");
    assert_eq!(result["protocolVersion"], 1);
    assert_eq!(result["capabilities"]["linkParquet"], true);
    assert_eq!(result["capabilities"]["importCsv"], true);
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
            "cacheDir": CACHE_DIR,
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
        json!({ "sessionId": "qs", "executionId": "e1", "sql": "SELECT i FROM range(1, 11) t(i);", "cacheDir": CACHE_DIR }),
    );
    assert_eq!(enqueued["state"], "queued");

    let status = poll_terminal(&mut engine, "e1");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsProduced"], 10);

    // DML and DDL without a row set succeed with no produced rows.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "e2", "sql": "CREATE TABLE t (a INTEGER); INSERT INTO t VALUES (1), (2), (3);", "cacheDir": CACHE_DIR }),
    );
    let status = poll_terminal(&mut engine, "e2");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsAffected"], 3);
    assert_eq!(status["rowsProduced"], Value::Null);

    // Multi-statement snapshots execute sequentially; the last row set wins.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "qs", "executionId": "e3", "sql": "CREATE TABLE t2 (a INTEGER); INSERT INTO t2 VALUES (1), (2), (3); SELECT count(*) AS n FROM t2;", "cacheDir": CACHE_DIR }),
    );
    let status = poll_terminal(&mut engine, "e3");
    assert_eq!(status["state"], "succeeded");
    assert_eq!(status["rowsProduced"], 1);
    assert_eq!(status["rowsAffected"], 3);

    engine.child.kill().ok();
    let _ = std::fs::remove_file(&database);
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
            "cacheDir": CACHE_DIR,
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
            "cacheDir": CACHE_DIR,
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

const CACHE_DIR: &str = "/tmp/tarik-engine-test-results";

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
        json!({ "sessionId": "cs", "executionId": "c1", "sql": LONG_QUERY, "cacheDir": CACHE_DIR }),
    );
    assert_eq!(enqueued["state"], "queued");

    // 2. A second submission on the same session queues behind the first.
    engine.assert_ok(
        "query.execute",
        json!({ "sessionId": "cs", "executionId": "c2", "sql": "SELECT 41 + 1 AS answer;", "cacheDir": CACHE_DIR }),
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
        json!({ "sessionId": "cs", "executionId": "c3", "sql": "SELECT 6 * 7 AS answer;", "cacheDir": CACHE_DIR }),
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
        json!({ "sessionId": "cs", "executionId": "k1", "sql": "SELECT 1;", "cacheDir": CACHE_DIR }),
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

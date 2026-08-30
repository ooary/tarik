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

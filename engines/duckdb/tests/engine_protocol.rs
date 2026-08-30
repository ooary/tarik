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

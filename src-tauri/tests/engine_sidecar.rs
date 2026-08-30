use std::path::{Path, PathBuf};

use tarik_engine_client::EngineProcess;

/// Spawn the engine exactly like the desktop does (sibling binary discovery)
/// and drive the same calls the project/source commands make.
fn locate_engine_binary() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .unwrap()
        .join("target/debug/tarik-engine-duckdb")
}

fn temp_path(name: &str, suffix: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("tarik-sidecar-{name}-{stamp}{suffix}"))
}

#[test]
fn engine_spawn_session_and_catalog_roundtrip() {
    let bin = locate_engine_binary();
    assert!(
        bin.exists(),
        "engine binary missing; build with ./scripts/build-engine.sh"
    );

    let mut process = EngineProcess::start(&bin)
        .map_err(|error| panic!("engine start failed: {error}"))
        .unwrap();
    let info = process
        .handshake()
        .map_err(|error| panic!("engine handshake failed: {error}"))
        .unwrap();
    assert_eq!(info.engine_id, "duckdb");
    assert_eq!(info.protocol_version, 1);
    assert!(info.capabilities.link_parquet);

    let database = temp_path("sidecar", ".duckdb");
    let locator = serde_json::json!({
        "engineId": "duckdb",
        "payload": { "path": database.to_string_lossy() }
    });
    process
        .request(
            "session.open",
            serde_json::json!({ "sessionId": "s1", "locator": locator }),
        )
        .unwrap();

    let csv = temp_path("orders", ".csv");
    std::fs::write(&csv, "id,amount\n1,12.5\n").unwrap();
    let imported = process
        .request(
            "duckdb.source.import_table",
            serde_json::json!({
                "sessionId": "s1",
                "projectId": "p1",
                "path": csv.to_string_lossy(),
                "options": {
                    "tableName": "orders",
                    "csv": { "delimiter": ",", "hasHeader": true, "nullValue": null, "allVarchar": false },
                    "columnOverrides": []
                }
            }),
        )
        .unwrap();
    assert_eq!(imported["duckdbName"], "orders");

    let catalog = process
        .request("catalog.inspect", serde_json::json!({ "sessionId": "s1" }))
        .unwrap();
    assert!(catalog["objects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["name"] == "orders"));

    // Idempotent reopen replaces the session instead of failing.
    process
        .request(
            "session.open",
            serde_json::json!({ "sessionId": "s1", "locator": locator }),
        )
        .unwrap();
    let catalog = process
        .request("catalog.inspect", serde_json::json!({ "sessionId": "s1" }))
        .unwrap();
    assert!(catalog["objects"]
        .as_array()
        .unwrap()
        .iter()
        .any(|o| o["name"] == "orders"));

    process.shutdown();

    let _ = std::fs::remove_file(&database);
    let _ = std::fs::remove_file(&csv);
}

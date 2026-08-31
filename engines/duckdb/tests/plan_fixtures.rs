use std::{collections::HashSet, fs, path::PathBuf};

use serde_json::Value;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plans")
}

fn load(name: &str) -> Value {
    let path = fixture_dir().join(name);
    serde_json::from_str(
        &fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!("could not read plan fixture {}: {error}", path.display())
        }),
    )
    .unwrap_or_else(|error| panic!("invalid JSON fixture {}: {error}", path.display()))
}

fn explain_operators(value: &Value, operators: &mut HashSet<String>) -> usize {
    let Some(nodes) = value.as_array() else {
        return 0;
    };
    nodes
        .iter()
        .map(|node| {
            let name = node["name"]
                .as_str()
                .expect("Explain node needs a name")
                .to_string();
            operators.insert(name);
            let children = node["children"]
                .as_array()
                .expect("Explain node children must be an array");
            1 + explain_operators(&Value::Array(children.clone()), operators)
        })
        .sum()
}

fn profile_operators(node: &Value, operators: &mut HashSet<String>) -> usize {
    if let Some(name) = node.get("operator_name").and_then(Value::as_str) {
        operators.insert(name.to_string());
    }
    let children = node["children"]
        .as_array()
        .expect("Profile node children must be an array");
    1 + children
        .iter()
        .map(|child| profile_operators(child, operators))
        .sum::<usize>()
}

#[test]
fn manifest_pins_supported_duckdb_and_lists_every_fixture() {
    let manifest = load("manifest.json");
    assert_eq!(manifest["duckdbVersion"], "1.5.5");
    let listed = manifest["fixtures"].as_array().unwrap();
    assert!(listed.len() >= 9);
    for name in listed.iter().filter_map(Value::as_str) {
        assert!(
            fixture_dir().join(name).is_file(),
            "missing fixture: {name}"
        );
    }
}

#[test]
fn explain_fixtures_cover_required_operator_families_and_are_connected() {
    let fixtures = [
        "scan.explain.json",
        "filter_projection.explain.json",
        "join.explain.json",
        "aggregate.explain.json",
        "sort_limit.explain.json",
        "union.explain.json",
        "cte.explain.json",
        "window.explain.json",
    ];
    let mut operators = HashSet::new();
    let mut node_count = 0;
    for name in fixtures {
        let fixture = load(name);
        let roots = fixture
            .as_array()
            .expect("Explain fixture must be an array");
        assert_eq!(roots.len(), 1, "{name} must have one connected root");
        node_count += explain_operators(&fixture, &mut operators);
    }
    assert!(node_count >= 8);
    for expected in [
        "SEQ_SCAN",
        "PROJECTION",
        "HASH_JOIN",
        "PERFECT_HASH_GROUP_BY",
        "TOP_N",
        "UNION",
        "WINDOW",
    ] {
        assert!(
            operators.contains(expected),
            "missing required operator family {expected}; found {operators:?}"
        );
    }
}

#[test]
fn profile_fixture_has_actual_metrics_and_connected_children() {
    let fixture = load("join_aggregate_sort_limit.profile.json");
    let mut operators = HashSet::new();
    let count = profile_operators(&fixture, &mut operators);
    assert!(count >= 5);
    assert!(operators.contains("HASH_JOIN"));
    assert!(operators.contains("HASH_GROUP_BY"));
    assert!(operators.contains("TOP_N"));
    assert!(fixture.get("latency").is_some());
    assert!(fixture.get("cumulative_rows_scanned").is_some());
}

#[test]
fn fixtures_contain_no_machine_specific_paths() {
    for entry in fs::read_dir(fixture_dir()).unwrap().flatten() {
        let text = fs::read_to_string(entry.path()).unwrap();
        for forbidden in ["/home/", "/tmp/", "tarik-e7-fixture"] {
            assert!(
                !text.contains(forbidden),
                "{} contains machine-specific text {forbidden}",
                entry.path().display()
            );
        }
    }
}

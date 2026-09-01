pub mod commands;

use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tarik_engine_protocol::ExecutionState;

use crate::engine_manager::EngineManager;

const PLAN_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PLAN_POLL_LIMIT: usize = 12_000; // five minutes for explicit Profile

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanMode {
    Explain,
    Profile,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    pub id: String,
    pub operator: String,
    pub native_name: String,
    pub source: Option<String>,
    pub estimated_rows: Option<u64>,
    pub actual_rows: Option<u64>,
    pub timing_ms: Option<f64>,
    pub rows_scanned: Option<u64>,
    pub details: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanEdge {
    pub id: String,
    /// Data flows from child/input toward parent/result.
    pub source: String,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryPlan {
    pub mode: PlanMode,
    pub nodes: Vec<PlanNode>,
    pub edges: Vec<PlanEdge>,
    pub root_ids: Vec<String>,
    pub raw_plan: String,
    pub fallback_reason: Option<String>,
}

impl QueryPlan {
    fn fallback(mode: PlanMode, raw_plan: String, reason: impl Into<String>) -> Self {
        Self {
            mode,
            nodes: Vec::new(),
            edges: Vec::new(),
            root_ids: Vec::new(),
            raw_plan,
            fallback_reason: Some(reason.into()),
        }
    }
}

/// Execute DuckDB's native JSON Explain/Profile through the existing async job
/// lifecycle, retrieve its one-row raw payload, and release the temporary
/// result pages. Profile is explicit because it executes the SQL statement.
pub fn capture_and_normalize(
    engine: &EngineManager,
    sql: &str,
    mode: PlanMode,
) -> Result<QueryPlan, String> {
    let sql = sql.trim();
    if sql.is_empty() {
        return Err("plan.empty: SQL text is empty".into());
    }
    let execution_id = format!("plan-{}", uuid::Uuid::new_v4());
    let prefix = match mode {
        PlanMode::Explain => "EXPLAIN (FORMAT JSON) ",
        PlanMode::Profile => "EXPLAIN (ANALYZE, FORMAT JSON) ",
    };
    engine.execute_query(&execution_id, &format!("{prefix}{sql}"))?;

    let terminal = (0..PLAN_POLL_LIMIT)
        .find_map(|_| {
            let status = match engine.query_status(&execution_id) {
                Ok(status) => status,
                Err(error) => {
                    return Some(Err(error));
                }
            }?;
            if matches!(
                status.state,
                ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
            ) {
                Some(Ok(status))
            } else {
                std::thread::sleep(PLAN_POLL_INTERVAL);
                None
            }
        })
        .ok_or_else(|| "plan.timeout: engine did not finish within five minutes".to_string())??;

    if terminal.state != ExecutionState::Succeeded {
        let error = terminal
            .error
            .map(|error| format!("{}: {}", error.code, error.message))
            .unwrap_or_else(|| format!("plan ended as {:?}", terminal.state));
        return Err(error);
    }
    let decoded = (|| {
        let page = engine.result_page(&execution_id, 0, 10)?;
        page.get("rows")
            .and_then(Value::as_array)
            .and_then(|rows| rows.first())
            .and_then(Value::as_array)
            .and_then(|row| row.get(1))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .ok_or_else(|| "plan.decode: engine result did not contain a plan payload".to_string())
    })();
    // The Explain/Profile result is only an interchange envelope. Release it
    // on both decode success and failure; QueryPlan owns the raw payload.
    let _ = engine.release_result(&execution_id);
    Ok(normalize_plan(mode, &decoded?))
}

pub fn normalize_plan(mode: PlanMode, raw_plan: &str) -> QueryPlan {
    let parsed: Value = match serde_json::from_str(raw_plan) {
        Ok(parsed) => parsed,
        Err(error) => {
            return QueryPlan::fallback(
                mode,
                raw_plan.to_string(),
                format!("invalid JSON: {error}"),
            );
        }
    };
    let mut builder = PlanBuilder::new(mode, raw_plan.to_string());
    let result = match mode {
        PlanMode::Explain => builder.parse_explain_roots(&parsed),
        PlanMode::Profile => builder.parse_profile_root(&parsed),
    };
    match result {
        Ok(()) if !builder.plan.nodes.is_empty() => beginner_plan(builder.plan),
        Ok(()) => QueryPlan::fallback(mode, raw_plan.to_string(), "plan contained no operators"),
        Err(reason) => QueryPlan::fallback(mode, raw_plan.to_string(), reason),
    }
}

struct PlanBuilder {
    plan: QueryPlan,
}

impl PlanBuilder {
    fn new(mode: PlanMode, raw_plan: String) -> Self {
        Self {
            plan: QueryPlan {
                mode,
                nodes: Vec::new(),
                edges: Vec::new(),
                root_ids: Vec::new(),
                raw_plan,
                fallback_reason: None,
            },
        }
    }

    fn next_id(&self) -> String {
        format!("n{}", self.plan.nodes.len())
    }

    fn parse_explain_roots(&mut self, value: &Value) -> Result<(), String> {
        let roots = value
            .as_array()
            .ok_or_else(|| "Explain plan root must be an array".to_string())?;
        for root in roots {
            let id = self.parse_explain_node(root)?;
            self.plan.root_ids.push(id);
        }
        Ok(())
    }

    fn parse_explain_node(&mut self, value: &Value) -> Result<String, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "Explain node must be an object".to_string())?;
        let native = object
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| "Explain node is missing name".to_string())?;
        let details = ordered_details(object.get("extra_info"));
        let id = self.next_id();
        self.plan.nodes.push(PlanNode {
            id: id.clone(),
            operator: normalize_operator(native),
            native_name: native.to_string(),
            source: detail_string(&details, "Table"),
            estimated_rows: detail_u64(&details, "Estimated Cardinality"),
            actual_rows: None,
            timing_ms: None,
            rows_scanned: None,
            details,
            presentation_note: None,
        });
        let children = object
            .get("children")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Explain node {native} children must be an array"))?;
        for child in children {
            let child_id = self.parse_explain_node(child)?;
            self.add_edge(&child_id, &id);
        }
        Ok(id)
    }

    fn parse_profile_root(&mut self, value: &Value) -> Result<(), String> {
        let id = self.parse_profile_node(value, true)?;
        self.plan.root_ids.push(id);
        Ok(())
    }

    fn parse_profile_node(&mut self, value: &Value, root: bool) -> Result<String, String> {
        let object = value
            .as_object()
            .ok_or_else(|| "Profile node must be an object".to_string())?;
        let native = object
            .get("operator_name")
            .or_else(|| object.get("operator_type"))
            .and_then(Value::as_str)
            .unwrap_or(if root { "QUERY" } else { "UNKNOWN" });
        let details = ordered_details(object.get("extra_info"));
        let id = self.next_id();
        self.plan.nodes.push(PlanNode {
            id: id.clone(),
            operator: normalize_operator(native),
            native_name: native.to_string(),
            source: detail_string(&details, "Table"),
            estimated_rows: detail_u64(&details, "Estimated Cardinality"),
            actual_rows: value_u64(object.get("operator_cardinality"))
                .or_else(|| value_u64(object.get("rows_returned"))),
            timing_ms: value_f64(object.get("operator_timing"))
                .or_else(|| value_f64(object.get("latency")))
                .map(|seconds| seconds * 1_000.0),
            rows_scanned: value_u64(object.get("operator_rows_scanned"))
                .or_else(|| value_u64(object.get("cumulative_rows_scanned"))),
            details,
            presentation_note: None,
        });
        let children = object
            .get("children")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("Profile node {native} children must be an array"))?;
        for child in children {
            let child_id = self.parse_profile_node(child, false)?;
            self.add_edge(&child_id, &id);
        }
        Ok(id)
    }

    fn add_edge(&mut self, source: &str, target: &str) {
        let id = format!("e{}", self.plan.edges.len());
        self.plan.edges.push(PlanEdge {
            id,
            source: source.to_string(),
            target: target.to_string(),
        });
    }
}

/// Convert DuckDB's physical tree into a beginner-facing graph without
/// inventing behavior. Administrative Profile wrappers collapse into one
/// result boundary, while a filter explicitly reported inside a scan is
/// expanded so its post-filter cardinality is not mislabeled as rows read.
fn beginner_plan(plan: QueryPlan) -> QueryPlan {
    let mut nodes = Vec::new();
    let mut source_endpoint = BTreeMap::new();
    let mut target_endpoint = BTreeMap::new();

    for node in &plan.nodes {
        if node.operator == "result" {
            continue;
        }
        let read_id = format!("native:read:{}", node.id);
        if node.operator == "scan" && node.details.contains_key("Filters") {
            let mut read = node.clone();
            read.id = read_id.clone();
            read.estimated_rows = None;
            read.actual_rows = None;
            read.details.remove("Filters");
            read.presentation_note = Some(
                "DuckDB applied the filter inside this scan. Tarik shows the filter as the next step for clarity; operator time covers the combined scan and filter work."
                    .to_string(),
            );
            nodes.push(read);

            let filter_id = format!("native:filter:{}", node.id);
            let mut details = BTreeMap::new();
            if let Some(filters) = node.details.get("Filters") {
                details.insert("Filters".to_string(), filters.clone());
            }
            nodes.push(PlanNode {
                id: filter_id.clone(),
                operator: "filter".to_string(),
                native_name: "PUSHED_DOWN_FILTER".to_string(),
                source: None,
                estimated_rows: node.estimated_rows,
                actual_rows: node.actual_rows,
                timing_ms: None,
                rows_scanned: None,
                details,
                presentation_note: Some(
                    "DuckDB applied this filter inside the table scan. Tarik displays it as a separate beginner step; it is not a separate physical operator."
                        .to_string(),
                ),
            });
            source_endpoint.insert(node.id.clone(), filter_id);
            target_endpoint.insert(node.id.clone(), read_id);
        } else {
            let mut kept = node.clone();
            kept.id = read_id.clone();
            nodes.push(kept);
            source_endpoint.insert(node.id.clone(), read_id.clone());
            target_endpoint.insert(node.id.clone(), read_id);
        }
    }

    let result_wrappers: Vec<_> = plan
        .nodes
        .iter()
        .filter(|node| node.operator == "result")
        .map(|node| node.native_name.clone())
        .collect();
    let mut edges = Vec::new();
    for edge in &plan.edges {
        let Some(source) = source_endpoint.get(&edge.source) else {
            continue;
        };
        let Some(target) = target_endpoint.get(&edge.target) else {
            continue;
        };
        edges.push((source.clone(), target.clone()));
    }
    for node in &plan.nodes {
        if node.operator == "scan" && node.details.contains_key("Filters") {
            edges.push((
                format!("native:read:{}", node.id),
                format!("native:filter:{}", node.id),
            ));
        }
    }

    let targeted: std::collections::HashSet<_> =
        edges.iter().map(|(source, _)| source.clone()).collect();
    let physical_roots: Vec<_> = nodes
        .iter()
        .filter(|node| !targeted.contains(&node.id))
        .map(|node| node.id.clone())
        .collect();
    let final_actual = physical_roots
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id)?.actual_rows)
        .sum::<u64>();
    let has_final_actual = physical_roots.iter().any(|id| {
        nodes
            .iter()
            .any(|node| &node.id == id && node.actual_rows.is_some())
    });
    let result_id = "beginner:result".to_string();
    let mut details = BTreeMap::new();
    if !result_wrappers.is_empty() {
        details.insert(
            "Native wrappers".to_string(),
            Value::String(result_wrappers.join(" → ")),
        );
    }
    nodes.push(PlanNode {
        id: result_id.clone(),
        operator: "result".to_string(),
        native_name: "QUERY_RESULT".to_string(),
        source: None,
        estimated_rows: None,
        actual_rows: has_final_actual.then_some(final_actual),
        timing_ms: None,
        rows_scanned: None,
        details,
        presentation_note: Some(
            "Tarik collapses DuckDB's administrative Profile wrappers into one result boundary."
                .to_string(),
        ),
    });
    for root in physical_roots {
        edges.push((root, result_id.clone()));
    }

    let ids: BTreeMap<_, _> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.clone(), format!("n{index}")))
        .collect();
    for node in &mut nodes {
        node.id = ids[&node.id].clone();
    }
    let edges = edges
        .into_iter()
        .enumerate()
        .map(|(index, (source, target))| PlanEdge {
            id: format!("e{index}"),
            source: ids[&source].clone(),
            target: ids[&target].clone(),
        })
        .collect();
    QueryPlan {
        mode: plan.mode,
        nodes,
        edges,
        root_ids: vec![ids[&result_id].clone()],
        raw_plan: plan.raw_plan,
        fallback_reason: plan.fallback_reason,
    }
}

fn ordered_details(value: Option<&Value>) -> BTreeMap<String, Value> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default()
}

fn detail_string(details: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    details.get(key).and_then(|value| match value {
        Value::String(value) => Some(value.clone()),
        Value::Array(values) => Some(
            values
                .iter()
                .map(|value| value.as_str().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", "),
        ),
        _ => None,
    })
}

fn detail_u64(details: &BTreeMap<String, Value>, key: &str) -> Option<u64> {
    details.get(key).and_then(|value| match value {
        Value::String(value) => value.parse().ok(),
        Value::Number(value) => value.as_u64(),
        _ => None,
    })
}

fn value_u64(value: Option<&Value>) -> Option<u64> {
    value.and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
    })
}

fn value_f64(value: Option<&Value>) -> Option<f64> {
    value.and_then(Value::as_f64)
}

fn normalize_operator(native: &str) -> String {
    let upper = native.trim().to_ascii_uppercase();
    if upper.contains("SCAN") {
        "scan"
    } else if upper.contains("JOIN") {
        "join"
    } else if upper.contains("GROUP_BY") || upper.contains("AGGREGATE") {
        "aggregate"
    } else if upper == "FILTER" {
        "filter"
    } else if upper == "PROJECTION" {
        "projection"
    } else if upper == "ORDER_BY" || upper == "TOP_N" {
        "sort"
    } else if upper == "LIMIT" || upper.contains("LIMIT") {
        "limit"
    } else if upper == "UNION" {
        "union"
    } else if upper == "WINDOW" {
        "window"
    } else if upper == "QUERY" || upper == "EXPLAIN_ANALYZE" {
        "result"
    } else {
        "unknown"
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, fs, path::PathBuf};

    use super::*;

    fn fixture(name: &str) -> String {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("engines/duckdb/tests/fixtures/plans")
            .join(name);
        fs::read_to_string(path).unwrap()
    }

    fn assert_connected(plan: &QueryPlan) {
        assert!(!plan.nodes.is_empty());
        assert!(!plan.root_ids.is_empty());
        let ids: HashSet<_> = plan.nodes.iter().map(|node| node.id.as_str()).collect();
        for edge in &plan.edges {
            assert!(ids.contains(edge.source.as_str()));
            assert!(ids.contains(edge.target.as_str()));
        }
        assert_eq!(plan.edges.len() + plan.root_ids.len(), plan.nodes.len());
    }

    #[test]
    fn all_explain_fixtures_normalize_to_deterministic_connected_graphs() {
        for name in [
            "scan.explain.json",
            "filter_projection.explain.json",
            "join.explain.json",
            "aggregate.explain.json",
            "sort_limit.explain.json",
            "union.explain.json",
            "cte.explain.json",
            "window.explain.json",
        ] {
            let raw = fixture(name);
            let first = normalize_plan(PlanMode::Explain, &raw);
            let second = normalize_plan(PlanMode::Explain, &raw);
            assert_eq!(first, second, "{name} was not deterministic");
            assert_connected(&first);
            assert!(first.fallback_reason.is_none());
        }
    }

    #[test]
    fn join_edges_flow_from_two_scans_toward_one_join() {
        let plan = normalize_plan(PlanMode::Explain, &fixture("join.explain.json"));
        let join = plan
            .nodes
            .iter()
            .find(|node| node.operator == "join")
            .unwrap();
        let incoming: Vec<_> = plan
            .edges
            .iter()
            .filter(|edge| edge.target == join.id)
            .collect();
        assert_eq!(incoming.len(), 2);
        assert!(incoming.iter().all(|edge| plan
            .nodes
            .iter()
            .find(|node| node.id == edge.source)
            .is_some_and(|node| node.operator == "scan")));
    }

    #[test]
    fn pushed_down_scan_filter_becomes_read_then_filter_then_result() {
        let raw = r#"[{"name":"PROJECTION","children":[{"name":"SEQ_SCAN","children":[],"extra_info":{"Table":"project.main.ppob_product","Filters":"ProductId='LA100'","Estimated Cardinality":"2","Projections":["ProductId"]}}],"extra_info":{"Estimated Cardinality":"2","Projections":["ProductId"]}}]"#;
        let plan = normalize_plan(PlanMode::Explain, raw);
        assert_connected(&plan);
        assert_eq!(
            plan.nodes
                .iter()
                .map(|node| node.operator.as_str())
                .collect::<Vec<_>>(),
            ["projection", "scan", "filter", "result"]
        );
        let read = plan
            .nodes
            .iter()
            .find(|node| node.operator == "scan")
            .unwrap();
        let filter = plan
            .nodes
            .iter()
            .find(|node| node.operator == "filter")
            .unwrap();
        assert_eq!(read.estimated_rows, None);
        assert!(!read.details.contains_key("Filters"));
        assert_eq!(filter.estimated_rows, Some(2));
        assert_eq!(filter.details["Filters"], "ProductId='LA100'");
        assert!(plan
            .edges
            .iter()
            .any(|edge| edge.source == read.id && edge.target == filter.id));
        assert_eq!(
            plan.nodes
                .iter()
                .filter(|node| node.operator == "result")
                .count(),
            1
        );
    }

    #[test]
    fn filtered_join_input_still_converges_with_the_other_scan() {
        let raw = r#"[{"name":"HASH_JOIN","children":[{"name":"SEQ_SCAN","children":[],"extra_info":{"Table":"project.main.orders","Filters":"amount>20","Estimated Cardinality":"20"}},{"name":"SEQ_SCAN","children":[],"extra_info":{"Table":"project.main.customers","Estimated Cardinality":"10"}}],"extra_info":{"Join Type":"INNER","Conditions":"customer_id=id","Estimated Cardinality":"18"}}]"#;
        let plan = normalize_plan(PlanMode::Explain, raw);
        assert_connected(&plan);
        let join = plan
            .nodes
            .iter()
            .find(|node| node.operator == "join")
            .unwrap();
        let incoming: Vec<_> = plan
            .edges
            .iter()
            .filter(|edge| edge.target == join.id)
            .collect();
        assert_eq!(incoming.len(), 2);
        let input_operators: std::collections::HashSet<_> = incoming
            .iter()
            .map(|edge| {
                plan.nodes
                    .iter()
                    .find(|node| node.id == edge.source)
                    .unwrap()
                    .operator
                    .as_str()
            })
            .collect();
        assert_eq!(
            input_operators,
            std::collections::HashSet::from(["filter", "scan"])
        );
        let filter = plan
            .nodes
            .iter()
            .find(|node| node.operator == "filter")
            .unwrap();
        let filter_input = plan
            .edges
            .iter()
            .find(|edge| edge.target == filter.id)
            .unwrap();
        let filtered_read = plan
            .nodes
            .iter()
            .find(|node| node.id == filter_input.source)
            .unwrap();
        assert_eq!(filtered_read.source.as_deref(), Some("project.main.orders"));
    }

    #[test]
    fn profile_collapses_wrappers_and_redistributes_filtered_scan_metrics() {
        let raw = r#"{"rows_returned":1,"children":[{"operator_name":"EXPLAIN_ANALYZE","operator_cardinality":0,"operator_timing":0,"children":[{"operator_name":"PROJECTION","operator_cardinality":1,"operator_timing":0.00001,"extra_info":{"Estimated Cardinality":"2","Projections":["ProductId"]},"children":[{"operator_name":"SEQ_SCAN","operator_cardinality":1,"operator_rows_scanned":736,"operator_timing":0.00012,"extra_info":{"Table":"project.main.ppob_product","Filters":"ProductId='LA100'","Estimated Cardinality":"2","Projections":["ProductId"]},"children":[]}]}]}]}"#;
        let plan = normalize_plan(PlanMode::Profile, raw);
        assert_connected(&plan);
        assert_eq!(
            plan.nodes
                .iter()
                .filter(|node| node.operator == "result")
                .count(),
            1
        );
        assert!(!plan
            .nodes
            .iter()
            .any(|node| node.native_name == "EXPLAIN_ANALYZE" || node.native_name == "QUERY"));
        let read = plan
            .nodes
            .iter()
            .find(|node| node.operator == "scan")
            .unwrap();
        let filter = plan
            .nodes
            .iter()
            .find(|node| node.operator == "filter")
            .unwrap();
        let projection = plan
            .nodes
            .iter()
            .find(|node| node.operator == "projection")
            .unwrap();
        let result = plan
            .nodes
            .iter()
            .find(|node| node.operator == "result")
            .unwrap();
        assert_eq!(read.rows_scanned, Some(736));
        assert!((read.timing_ms.unwrap() - 0.12).abs() < f64::EPSILON * 10.0);
        assert_eq!((read.estimated_rows, read.actual_rows), (None, None));
        assert_eq!(
            (filter.estimated_rows, filter.actual_rows),
            (Some(2), Some(1))
        );
        assert_eq!(filter.timing_ms, None);
        assert_eq!(
            (projection.estimated_rows, projection.actual_rows),
            (Some(2), Some(1))
        );
        assert!((projection.timing_ms.unwrap() - 0.01).abs() < f64::EPSILON * 10.0);
        assert_eq!(result.actual_rows, Some(1));
        assert_eq!(result.details["Native wrappers"], "QUERY → EXPLAIN_ANALYZE");
    }

    #[test]
    fn profile_preserves_actual_metrics_and_native_details() {
        let plan = normalize_plan(
            PlanMode::Profile,
            &fixture("join_aggregate_sort_limit.profile.json"),
        );
        assert_connected(&plan);
        let join = plan
            .nodes
            .iter()
            .find(|node| node.operator == "join")
            .unwrap();
        assert!(join.actual_rows.is_some());
        assert!(join.timing_ms.is_some());
        assert!(join.details.contains_key("Join Type"));
    }

    #[test]
    fn unknown_operator_is_preserved_truthfully() {
        let raw = r#"[{"name":"FUTURE_OPERATOR","children":[],"extra_info":{"x":"y"}}]"#;
        let plan = normalize_plan(PlanMode::Explain, raw);
        assert_eq!(plan.nodes[0].operator, "unknown");
        assert_eq!(plan.nodes[0].native_name, "FUTURE_OPERATOR");
        assert_eq!(plan.nodes[0].details["x"], "y");
    }

    #[test]
    fn malformed_plan_falls_back_to_raw_without_dropping_it() {
        let raw = "not json at all";
        let plan = normalize_plan(PlanMode::Explain, raw);
        assert!(plan.nodes.is_empty());
        assert_eq!(plan.raw_plan, raw);
        assert!(plan
            .fallback_reason
            .as_deref()
            .unwrap()
            .contains("invalid JSON"));
    }
}

#[cfg(test)]
mod integration_tests {
    use std::path::PathBuf;

    use super::*;

    fn workspace_engine() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("target/debug/tarik-engine-duckdb")
    }

    fn temp_path(name: &str, suffix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tarik-plan-{name}-{}{}",
            uuid::Uuid::new_v4(),
            suffix
        ))
    }

    #[test]
    fn real_sidecar_explain_normalizes_and_releases_temporary_result() {
        let engine_bin = workspace_engine();
        assert!(engine_bin.exists(), "build engine before desktop tests");
        let database = temp_path("db", ".duckdb");
        let result_root = temp_path("results", "");
        let manager = EngineManager::new(engine_bin, result_root.clone());
        manager.open_session(&database).unwrap();

        let plan = capture_and_normalize(
            &manager,
            "SELECT i FROM range(1, 10) t(i) WHERE i > 3",
            PlanMode::Explain,
        )
        .unwrap();
        assert!(!plan.nodes.is_empty());
        assert!(plan.fallback_reason.is_none());
        assert!(plan.nodes.iter().all(|node| !node.native_name.is_empty()));
        assert!(
            std::fs::read_dir(&result_root)
                .map(|mut entries| entries.next().is_none())
                .unwrap_or(true),
            "plan result artifacts were not released"
        );

        manager.close_session().unwrap();
        manager.shutdown();
        let _ = std::fs::remove_file(database);
        let _ = std::fs::remove_dir_all(result_root);
    }
}

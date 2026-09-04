use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use super::{PlanNode, PlanSemantic, SqlTextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
struct QuerySemantics {
    distinct: Option<ByteRange>,
    groups: Vec<Expression>,
    calculations: Vec<Calculation>,
    outputs: Vec<String>,
    selected_group_keys: Vec<String>,
    simple_aggregate_projection: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Expression {
    display: String,
    range: ByteRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Calculation {
    kind: CalculationKind,
    argument: String,
    range: ByteRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CalculationKind {
    CountRows,
    CountNonNull,
    CountMatching,
    CountUnique,
    Sum,
    Average,
    Minimum,
    Maximum,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ByteRange {
    from: usize,
    to: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    text: String,
    upper: String,
    from: usize,
    to: usize,
    depth: usize,
    quoted: bool,
}

pub(super) fn apply(nodes: &mut Vec<PlanNode>, edges: &mut Vec<(String, String)>, sql: &str) {
    let Some(semantics) = parse_query(sql) else {
        return;
    };
    if semantics.calculations.is_empty() {
        annotate_distinct(nodes, sql, &semantics);
        annotate_boundaries(nodes, &semantics);
        return;
    }

    let calculated_aggregates: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.operator == "aggregate" && aggregate_detail_count(&node.details) > 0
        })
        .map(|(index, _)| index)
        .collect();
    if calculated_aggregates.len() != 1 {
        return;
    }
    let aggregate_index = calculated_aggregates[0];
    let native_kinds = native_calculation_kinds(&nodes[aggregate_index].details);
    let sql_kinds: Vec<_> = semantics
        .calculations
        .iter()
        .map(|calculation| calculation.kind)
        .collect();
    if native_kinds != sql_kinds {
        return;
    }

    if semantics.simple_aggregate_projection {
        let aggregate_ids: HashSet<&str> = nodes
            .iter()
            .filter(|node| node.operator == "aggregate")
            .map(|node| node.id.as_str())
            .collect();
        let projection_ids: HashSet<String> = nodes
            .iter()
            .filter(|node| node.operator == "projection")
            .map(|node| node.id.clone())
            .collect();
        let aggregate_connected = projection_ids
            .iter()
            .all(|id| projection_reaches_aggregate(id, edges, &projection_ids, &aggregate_ids));
        if aggregate_connected {
            remove_and_rewire(nodes, edges, &projection_ids);
        }
    }

    let Some(aggregate_index) = nodes
        .iter()
        .position(|node| node.operator == "aggregate" && aggregate_detail_count(&node.details) > 0)
    else {
        return;
    };
    let aggregate_id = nodes[aggregate_index].id.clone();
    let scope = if semantics.groups.is_empty() {
        "for the whole input"
    } else {
        "per group"
    };
    let labels: Vec<String> = semantics
        .calculations
        .iter()
        .map(|calculation| calculation_label(calculation, scope))
        .collect();
    let aggregate_range = if semantics.calculations.len() == 1 {
        Some(utf16_range(sql, semantics.calculations[0].range))
    } else {
        enclosing_range(sql, &semantics.calculations)
    };
    let aggregate = &mut nodes[aggregate_index];
    if labels.len() == 1 {
        aggregate.operator = calculation_operator(semantics.calculations[0].kind).to_string();
        aggregate.semantic = Some(PlanSemantic {
            title: labels[0].clone(),
            summary: calculation_summary(&semantics.calculations[0], !semantics.groups.is_empty()),
            input_label: if semantics.groups.is_empty() {
                "All input rows".into()
            } else {
                "Rows in each group".into()
            },
            output_label: if semantics.groups.is_empty() {
                "One summary row".into()
            } else {
                "One count or value per group".into()
            },
            sql_range: aggregate_range,
            concept_only: false,
        });
    } else {
        aggregate.operator = "summaries".into();
        aggregate.details.insert(
            "Calculations".into(),
            Value::Array(labels.iter().cloned().map(Value::String).collect()),
        );
        aggregate.semantic = Some(PlanSemantic {
            title: "Calculate summaries".into(),
            summary: format!(
                "DuckDB calculates {} summary values together {scope}.",
                labels.len()
            ),
            input_label: if semantics.groups.is_empty() {
                "All input rows".into()
            } else {
                "Rows in each group".into()
            },
            output_label: "Summary values calculated together".into(),
            sql_range: aggregate_range,
            concept_only: false,
        });
    }

    if !semantics.groups.is_empty() {
        let group_title = format!(
            "Group rows by {}",
            semantics
                .groups
                .iter()
                .map(|group| group.display.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let group_id = "semantic:group".to_string();
        let group_range = enclosing_expression_range(sql, &semantics.groups);
        let incoming: Vec<String> = edges
            .iter()
            .filter(|(_, target)| target == &aggregate_id)
            .map(|(source, _)| source.clone())
            .collect();
        edges.retain(|(_, target)| target != &aggregate_id);
        for source in incoming {
            edges.push((source, group_id.clone()));
        }
        edges.push((group_id.clone(), aggregate_id.clone()));
        nodes.push(PlanNode {
            id: group_id,
            operator: "group".into(),
            native_name: "BEGINNER_GROUP_CONCEPT".into(),
            semantic: Some(PlanSemantic {
                title: group_title,
                summary: "Rows with the same grouping values are treated as one group.".into(),
                input_label: "Detailed input rows".into(),
                output_label: "Groups used by the following calculations".into(),
                sql_range: group_range,
                concept_only: true,
            }),
            source: None,
            estimated_rows: None,
            actual_rows: None,
            timing_ms: None,
            rows_scanned: None,
            details: BTreeMap::from([(
                "Groups".into(),
                Value::Array(
                    semantics
                        .groups
                        .iter()
                        .map(|group| Value::String(group.display.clone()))
                        .collect(),
                ),
            )]),
            presentation_note: Some(
                "Grouping and the following calculation are separate beginner concepts backed by one DuckDB aggregate operator. Measured rows and time appear only on the calculation step."
                    .into(),
            ),
        });
    }

    annotate_distinct(nodes, sql, &semantics);
    annotate_boundaries(nodes, &semantics);
}

fn annotate_distinct(nodes: &mut [PlanNode], sql: &str, semantics: &QuerySemantics) {
    let Some(distinct_range) = semantics.distinct else {
        return;
    };
    let empty_aggregates: Vec<usize> = nodes
        .iter()
        .enumerate()
        .filter(|(_, node)| {
            node.operator == "aggregate" && aggregate_detail_count(&node.details) == 0
        })
        .map(|(index, _)| index)
        .collect();
    if empty_aggregates.len() != 1 {
        return;
    }
    let distinct = &mut nodes[empty_aggregates[0]];
    distinct.operator = "distinct".into();
    distinct.semantic = Some(PlanSemantic {
        title: "Remove duplicate result rows".into(),
        summary: "DISTINCT removes repeated combinations from the selected result.".into(),
        input_label: if semantics.groups.is_empty() {
            "Selected result rows".into()
        } else {
            "Grouped result rows".into()
        },
        output_label: "Unique result rows".into(),
        sql_range: Some(utf16_range(sql, distinct_range)),
        concept_only: false,
    });
    if distinct_is_redundant(semantics) {
        distinct.presentation_note = Some(
            "DISTINCT is redundant here because GROUP BY already produces one row for each selected group and its summary values. Removing DISTINCT returns the same rows."
                .into(),
        );
    }
}

fn annotate_boundaries(nodes: &mut [PlanNode], semantics: &QuerySemantics) {
    if let Some(scan) = nodes.iter_mut().find(|node| node.operator == "scan") {
        if let Some(source) = scan.source.as_deref() {
            let source = short_source(source);
            scan.semantic = Some(PlanSemantic {
                title: format!("Read {source}"),
                summary: format!("DuckDB reads the columns needed from {source}."),
                input_label: "Stored or generated data".into(),
                output_label: "Rows read from the source".into(),
                sql_range: None,
                concept_only: false,
            });
        }
    }

    if let Some(result) = nodes.iter_mut().find(|node| node.operator == "result") {
        result.semantic = Some(PlanSemantic {
            title: "Query result".into(),
            summary: if semantics.outputs.is_empty() {
                "DuckDB returns the final rows.".into()
            } else {
                format!("Return: {}.", semantics.outputs.join(", "))
            },
            input_label: "Final operation output".into(),
            output_label: "Rows returned to the query".into(),
            sql_range: None,
            concept_only: false,
        });
    }
}

fn projection_reaches_aggregate(
    projection_id: &str,
    edges: &[(String, String)],
    projection_ids: &HashSet<String>,
    aggregate_ids: &HashSet<&str>,
) -> bool {
    let mut pending = vec![projection_id];
    let mut seen = HashSet::new();
    while let Some(current) = pending.pop() {
        if !seen.insert(current) {
            continue;
        }
        for neighbor in edges.iter().filter_map(|(source, target)| {
            if source == current {
                Some(target.as_str())
            } else if target == current {
                Some(source.as_str())
            } else {
                None
            }
        }) {
            if aggregate_ids.contains(neighbor) {
                return true;
            }
            if projection_ids.contains(neighbor) {
                pending.push(neighbor);
            }
        }
    }
    false
}

fn remove_and_rewire(
    nodes: &mut Vec<PlanNode>,
    edges: &mut Vec<(String, String)>,
    remove: &HashSet<String>,
) {
    for id in remove {
        let incoming: Vec<String> = edges
            .iter()
            .filter(|(_, target)| target == id)
            .map(|(source, _)| source.clone())
            .collect();
        let outgoing: Vec<String> = edges
            .iter()
            .filter(|(source, _)| source == id)
            .map(|(_, target)| target.clone())
            .collect();
        edges.retain(|(source, target)| source != id && target != id);
        for source in &incoming {
            for target in &outgoing {
                if source != target && !edges.contains(&(source.clone(), target.clone())) {
                    edges.push((source.clone(), target.clone()));
                }
            }
        }
    }
    nodes.retain(|node| !remove.contains(&node.id));
}

fn aggregate_detail_count(details: &BTreeMap<String, Value>) -> usize {
    aggregate_detail_strings(details).len()
}

fn aggregate_detail_strings(details: &BTreeMap<String, Value>) -> Vec<&str> {
    match details.get("Aggregates") {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .collect(),
        Some(Value::String(value)) if !value.trim().is_empty() => vec![value],
        _ => Vec::new(),
    }
}

fn native_calculation_kinds(details: &BTreeMap<String, Value>) -> Vec<CalculationKind> {
    aggregate_detail_strings(details)
        .into_iter()
        .filter_map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.starts_with("count_star(") {
                Some(CalculationKind::CountRows)
            } else if normalized.starts_with("count_if(") || normalized.starts_with("countif(") {
                Some(CalculationKind::CountMatching)
            } else if normalized.starts_with("count(distinct ") {
                Some(CalculationKind::CountUnique)
            } else if normalized.starts_with("count(") {
                Some(CalculationKind::CountNonNull)
            } else if normalized.starts_with("sum(") {
                Some(CalculationKind::Sum)
            } else if normalized.starts_with("avg(") {
                Some(CalculationKind::Average)
            } else if normalized.starts_with("min(") {
                Some(CalculationKind::Minimum)
            } else if normalized.starts_with("max(") {
                Some(CalculationKind::Maximum)
            } else {
                None
            }
        })
        .collect()
}

fn calculation_operator(kind: CalculationKind) -> &'static str {
    match kind {
        CalculationKind::CountRows
        | CalculationKind::CountNonNull
        | CalculationKind::CountMatching
        | CalculationKind::CountUnique => "count",
        CalculationKind::Sum => "sum",
        CalculationKind::Average => "average",
        CalculationKind::Minimum => "minimum",
        CalculationKind::Maximum => "maximum",
    }
}

fn calculation_label(calculation: &Calculation, scope: &str) -> String {
    match calculation.kind {
        CalculationKind::CountRows => format!("Count rows {scope}"),
        CalculationKind::CountNonNull => {
            format!("Count non-null {} values {scope}", calculation.argument)
        }
        CalculationKind::CountMatching => format!("Count matching rows {scope}"),
        CalculationKind::CountUnique => {
            format!("Count unique {} values {scope}", calculation.argument)
        }
        CalculationKind::Sum => format!("Sum {} {scope}", calculation.argument),
        CalculationKind::Average => {
            format!("Calculate average {} {scope}", calculation.argument)
        }
        CalculationKind::Minimum => format!("Find minimum {} {scope}", calculation.argument),
        CalculationKind::Maximum => format!("Find maximum {} {scope}", calculation.argument),
    }
}

fn calculation_summary(calculation: &Calculation, grouped: bool) -> String {
    let scope = if grouped {
        " for each group"
    } else {
        " across the whole input"
    };
    match calculation.kind {
        CalculationKind::CountRows => format!("COUNT(*) counts every row{scope}."),
        CalculationKind::CountNonNull => format!(
            "COUNT({}) counts only non-null values{}.",
            calculation.argument, scope
        ),
        CalculationKind::CountMatching => {
            format!("COUNT_IF counts only rows matching its condition{scope}.")
        }
        CalculationKind::CountUnique => format!(
            "COUNT(DISTINCT {}) counts unique non-null values{}.",
            calculation.argument, scope
        ),
        CalculationKind::Sum => format!(
            "SUM adds the non-null {} values{}.",
            calculation.argument, scope
        ),
        CalculationKind::Average => format!(
            "AVG calculates the average non-null {} value{}.",
            calculation.argument, scope
        ),
        CalculationKind::Minimum => format!(
            "MIN finds the smallest non-null {} value{}.",
            calculation.argument, scope
        ),
        CalculationKind::Maximum => format!(
            "MAX finds the largest non-null {} value{}.",
            calculation.argument, scope
        ),
    }
}

fn distinct_is_redundant(semantics: &QuerySemantics) -> bool {
    let selected: HashSet<_> = semantics
        .selected_group_keys
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect();
    let grouped: HashSet<_> = semantics
        .groups
        .iter()
        .map(|group| group.display.to_ascii_lowercase())
        .collect();
    semantics.distinct.is_some()
        && !grouped.is_empty()
        && selected == grouped
        && semantics.simple_aggregate_projection
}

fn short_source(source: &str) -> String {
    source
        .split('.')
        .next_back()
        .unwrap_or(source)
        .trim_matches('"')
        .replace("\"\"", "\"")
}

fn parse_query(sql: &str) -> Option<QuerySemantics> {
    // Byte offsets are converted to JavaScript UTF-16 positions only for the
    // conservative ASCII grammar. Unicode SQL remains fully executable but
    // keeps generic native plan labels rather than risking a wrong range.
    if !sql.is_ascii() {
        return None;
    }
    let tokens = tokenize(sql);
    if tokens
        .iter()
        .any(|token| token.upper == "SELECT" && token.depth > 0)
    {
        return None;
    }
    let select = tokens
        .iter()
        .position(|token| token.depth == 0 && token.upper == "SELECT")?;
    let from = tokens
        .iter()
        .enumerate()
        .skip(select + 1)
        .find(|(_, token)| token.depth == 0 && token.upper == "FROM")?
        .0;
    let distinct = tokens
        .get(select + 1)
        .filter(|token| token.depth == 0 && token.upper == "DISTINCT")
        .map(|token| ByteRange {
            from: token.from,
            to: token.to,
        });
    let select_start = if distinct.is_some() {
        select + 2
    } else {
        select + 1
    };
    let select_ranges = split_expressions(&tokens[select_start..from], 0);
    let outputs = select_ranges
        .iter()
        .map(|range| sql[range.from..range.to].trim().to_string())
        .collect::<Vec<_>>();
    let calculations = parse_calculations(sql, &tokens[select_start..from]);
    let selected_group_keys = select_ranges
        .iter()
        .filter(|range| {
            !calculations.iter().any(|calculation| {
                calculation.range.from >= range.from && calculation.range.to <= range.to
            })
        })
        .map(|range| simple_expression_display(&sql[range.from..range.to]))
        .collect::<Vec<_>>();

    let group_start = tokens
        .iter()
        .enumerate()
        .skip(from + 1)
        .find(|(index, token)| {
            token.depth == 0
                && token.upper == "GROUP"
                && tokens
                    .get(index + 1)
                    .is_some_and(|next| next.depth == 0 && next.upper == "BY")
        })
        .map(|(index, _)| index + 2);
    let groups = group_start
        .map(|start| {
            let end = tokens
                .iter()
                .enumerate()
                .skip(start)
                .find(|(_, token)| {
                    token.depth == 0
                        && matches!(
                            token.upper.as_str(),
                            "HAVING" | "ORDER" | "LIMIT" | "UNION" | "QUALIFY"
                        )
                })
                .map(|(index, _)| index)
                .unwrap_or(tokens.len());
            split_expressions(&tokens[start..end], 0)
                .into_iter()
                .map(|range| Expression {
                    display: simple_expression_display(&sql[range.from..range.to]),
                    range,
                })
                .collect()
        })
        .unwrap_or_default();

    let simple_count = calculations.len();
    let simple_aggregate_projection = !select_ranges.is_empty()
        && select_ranges.iter().all(|range| {
            let expression_tokens: Vec<_> = tokens
                .iter()
                .filter(|token| token.from >= range.from && token.to <= range.to)
                .cloned()
                .collect();
            is_simple_identifier_expression(&expression_tokens)
                || expression_has_one_complete_calculation(sql, range, &calculations)
        })
        && simple_count > 0;

    Some(QuerySemantics {
        distinct,
        groups,
        calculations,
        outputs,
        selected_group_keys,
        simple_aggregate_projection,
    })
}

fn parse_calculations(sql: &str, tokens: &[Token]) -> Vec<Calculation> {
    let mut calculations = Vec::new();
    let mut index = 0;
    while index + 1 < tokens.len() {
        let name = tokens[index].upper.as_str();
        let kind = match name {
            "COUNT" => Some(CalculationKind::CountNonNull),
            "COUNT_IF" | "COUNTIF" => Some(CalculationKind::CountMatching),
            "SUM" => Some(CalculationKind::Sum),
            "AVG" | "AVERAGE" => Some(CalculationKind::Average),
            "MIN" => Some(CalculationKind::Minimum),
            "MAX" => Some(CalculationKind::Maximum),
            _ => None,
        };
        let Some(mut kind) = kind else {
            index += 1;
            continue;
        };
        if tokens[index + 1].text != "(" {
            index += 1;
            continue;
        }
        let open_depth = tokens[index + 1].depth;
        let Some(close) = tokens
            .iter()
            .enumerate()
            .skip(index + 2)
            .find(|(_, token)| token.text == ")" && token.depth == open_depth)
            .map(|(index, _)| index)
        else {
            index += 1;
            continue;
        };
        let argument_tokens = &tokens[index + 2..close];
        let mut argument = sql[tokens[index + 1].to..tokens[close].from]
            .trim()
            .to_string();
        if name == "COUNT" {
            if argument == "*" {
                kind = CalculationKind::CountRows;
                argument = "rows".into();
            } else if argument_tokens
                .first()
                .is_some_and(|token| token.upper == "DISTINCT")
            {
                kind = CalculationKind::CountUnique;
                argument = argument_tokens
                    .first()
                    .map(|token| sql[token.to..tokens[close].from].trim().to_string())
                    .unwrap_or_default();
            }
        }
        calculations.push(Calculation {
            kind,
            argument: simple_expression_display(&argument),
            range: ByteRange {
                from: tokens[index].from,
                to: tokens[close].to,
            },
        });
        index = close + 1;
    }
    calculations
}

fn split_expressions(tokens: &[Token], base_depth: usize) -> Vec<ByteRange> {
    if tokens.is_empty() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    for (index, token) in tokens.iter().enumerate() {
        if token.text == "," && token.depth == base_depth {
            if let (Some(first), Some(last)) =
                (tokens.get(start), tokens.get(index.wrapping_sub(1)))
            {
                ranges.push(ByteRange {
                    from: first.from,
                    to: last.to,
                });
            }
            start = index + 1;
        }
    }
    if let (Some(first), Some(last)) = (tokens.get(start), tokens.last()) {
        ranges.push(ByteRange {
            from: first.from,
            to: last.to,
        });
    }
    ranges
}

fn is_simple_identifier_expression(tokens: &[Token]) -> bool {
    let end = tokens
        .iter()
        .position(|token| token.depth == 0 && token.upper == "AS")
        .unwrap_or(tokens.len());
    let core = &tokens[..end];
    !core.is_empty()
        && core.iter().enumerate().all(|(index, token)| {
            if index % 2 == 0 {
                token.quoted
                    || token
                        .text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            } else {
                token.text == "."
            }
        })
}

fn expression_has_one_complete_calculation(
    sql: &str,
    range: &ByteRange,
    calculations: &[Calculation],
) -> bool {
    let matching: Vec<_> = calculations
        .iter()
        .filter(|calculation| {
            calculation.range.from >= range.from && calculation.range.to <= range.to
        })
        .collect();
    if matching.len() != 1 {
        return false;
    }
    let expression = sql[range.from..range.to].trim();
    let call = sql[matching[0].range.from..matching[0].range.to].trim();
    expression == call
        || expression.strip_prefix(call).is_some_and(|rest| {
            let rest = rest.trim();
            rest.len() > 2 && rest.to_ascii_uppercase().starts_with("AS ")
        })
}

fn simple_expression_display(value: &str) -> String {
    value
        .trim()
        .split('.')
        .next_back()
        .unwrap_or(value.trim())
        .trim_matches('"')
        .replace("\"\"", "\"")
}

fn enclosing_expression_range(sql: &str, expressions: &[Expression]) -> Option<SqlTextRange> {
    Some(utf16_range(
        sql,
        ByteRange {
            from: expressions.first()?.range.from,
            to: expressions.last()?.range.to,
        },
    ))
}

fn enclosing_range(sql: &str, calculations: &[Calculation]) -> Option<SqlTextRange> {
    Some(utf16_range(
        sql,
        ByteRange {
            from: calculations.first()?.range.from,
            to: calculations.last()?.range.to,
        },
    ))
}

fn utf16_range(sql: &str, range: ByteRange) -> SqlTextRange {
    SqlTextRange {
        from: sql[..range.from].encode_utf16().count() as u32,
        to: sql[..range.to].encode_utf16().count() as u32,
    }
}

fn tokenize(sql: &str) -> Vec<Token> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    let mut depth = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'-' && bytes.get(index + 1) == Some(&b'-') {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'*') {
            let mut nested = 1;
            index += 2;
            while index < bytes.len() && nested > 0 {
                if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
                    nested += 1;
                    index += 2;
                } else if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
                    nested -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            continue;
        }
        if byte == b'\'' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\'' && bytes.get(index + 1) == Some(&b'\'') {
                    index += 2;
                } else if bytes[index] == b'\'' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            continue;
        }
        if byte == b'"' {
            let from = index;
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'"' && bytes.get(index + 1) == Some(&b'"') {
                    index += 2;
                } else if bytes[index] == b'"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
            let text = sql[from..index].to_string();
            tokens.push(Token {
                upper: text
                    .trim_matches('"')
                    .replace("\"\"", "\"")
                    .to_ascii_uppercase(),
                text,
                from,
                to: index,
                depth,
                quoted: true,
            });
            continue;
        }
        if byte == b'(' {
            tokens.push(symbol_token(sql, index, depth));
            depth += 1;
            index += 1;
            continue;
        }
        if byte == b')' {
            depth = depth.saturating_sub(1);
            tokens.push(symbol_token(sql, index, depth));
            index += 1;
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let from = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || matches!(bytes[index], b'_' | b'$'))
            {
                index += 1;
            }
            let text = sql[from..index].to_string();
            tokens.push(Token {
                upper: text.to_ascii_uppercase(),
                text,
                from,
                to: index,
                depth,
                quoted: false,
            });
            continue;
        }
        tokens.push(symbol_token(sql, index, depth));
        index += 1;
    }
    tokens
}

fn symbol_token(sql: &str, index: usize, depth: usize) -> Token {
    let text = sql[index..index + 1].to_string();
    Token {
        upper: text.clone(),
        text,
        from: index,
        to: index + 1,
        depth,
        quoted: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_grouped_count_distinct_with_reliable_ranges() {
        let sql = "SELECT DISTINCT commodity, count(market)\nFROM \"main\".\"data_2021\"\nGROUP BY commodity";
        let parsed = parse_query(sql).unwrap();
        assert_eq!(parsed.groups[0].display, "commodity");
        assert_eq!(parsed.calculations[0].kind, CalculationKind::CountNonNull);
        assert_eq!(parsed.calculations[0].argument, "market");
        assert_eq!(
            &sql[parsed.calculations[0].range.from..parsed.calculations[0].range.to],
            "count(market)"
        );
        assert!(parsed.distinct.is_some());
        assert!(parsed.simple_aggregate_projection);
    }

    #[test]
    fn distinguishes_count_variants_and_summary_functions() {
        let sql = "SELECT count(*), count(DISTINCT customer_id), count_if(amount > 10), sum(amount), avg(amount), min(day), max(day) FROM orders";
        let parsed = parse_query(sql).unwrap();
        assert_eq!(
            parsed
                .calculations
                .iter()
                .map(|calculation| calculation.kind)
                .collect::<Vec<_>>(),
            [
                CalculationKind::CountRows,
                CalculationKind::CountUnique,
                CalculationKind::CountMatching,
                CalculationKind::Sum,
                CalculationKind::Average,
                CalculationKind::Minimum,
                CalculationKind::Maximum,
            ]
        );
    }

    #[test]
    fn distinct_redundancy_requires_all_group_keys_in_the_result() {
        let redundant =
            parse_query("SELECT DISTINCT commodity, count(market) FROM data GROUP BY commodity")
                .unwrap();
        assert!(distinct_is_redundant(&redundant));

        let needed = parse_query(
            "SELECT DISTINCT commodity, count(market) FROM data GROUP BY commodity, region",
        )
        .unwrap();
        assert!(!distinct_is_redundant(&needed));
    }

    #[test]
    fn comments_strings_and_nested_commas_do_not_split_select_expressions() {
        let sql = "SELECT commodity, sum(coalesce(amount, 0)) AS total /*, count(*) */ FROM orders GROUP BY commodity";
        let parsed = parse_query(sql).unwrap();
        assert_eq!(parsed.groups.len(), 1);
        assert_eq!(parsed.calculations.len(), 1);
        assert_eq!(parsed.calculations[0].kind, CalculationKind::Sum);
        assert_eq!(parsed.calculations[0].argument, "coalesce(amount, 0)");
    }
}

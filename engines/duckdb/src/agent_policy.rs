use std::{collections::HashSet, ops::ControlFlow};

use duckdb::Connection;
use sqlparser::{
    ast::{
        Expr, FromTable, ObjectName, ObjectNamePart, ObjectType, Statement, TableFactor,
        TableObject, Visit, Visitor,
    },
    dialect::DuckDbDialect,
    parser::Parser,
};
use tarik_engine_protocol::{
    AgentRegisteredSource, AgentSqlClassification, AgentSqlDecision, CatalogSnapshot,
    MAX_AGENT_SQL_BYTES,
};

use crate::{catalog, error::EngineError};

const MAX_AST_DEPTH: usize = 128;
const MAX_AFFECTED_OBJECTS: usize = 100;

pub fn classify(
    connection: &Connection,
    sql: &str,
    registered_sources: &[AgentRegisteredSource],
) -> Result<AgentSqlClassification, EngineError> {
    if sql.trim().is_empty() {
        return Ok(blocked("agent.sql_empty", "empty", ""));
    }
    if sql.len() > MAX_AGENT_SQL_BYTES {
        return Ok(blocked("agent.sql_too_large", "unknown", ""));
    }
    let dialect = DuckDbDialect {};
    let statements = match Parser::new(&dialect)
        .with_recursion_limit(MAX_AST_DEPTH)
        .try_with_sql(sql)
        .and_then(|mut parser| parser.parse_statements())
    {
        Ok(statements) => statements,
        Err(_) => return Ok(blocked("agent.sqlparser_rejected", "unknown", "")),
    };
    if statements.len() != 1 {
        return Ok(blocked("agent.multiple_statements", "multi", ""));
    }
    let statement = &statements[0];
    let duckdb_type = match duckdb_statement_type(connection, sql) {
        Ok(statement_type) => statement_type,
        Err(_) => return Ok(blocked("agent.duckdb_parser_rejected", "unknown", "")),
    };
    if !parsers_agree(statement, &duckdb_type) {
        return Ok(blocked(
            "agent.parser_disagreement",
            statement_label(statement),
            "",
        ));
    }
    let catalog = catalog::inspect(connection)?;
    let catalog_revision = agent_catalog_revision(connection, &catalog, registered_sources)?;
    let mut visitor = PolicyVisitor::new(&catalog, registered_sources);
    if let ControlFlow::Break(reason) = statement.visit(&mut visitor) {
        return Ok(blocked(
            reason,
            statement_label(statement),
            &catalog_revision,
        ));
    }
    if visitor.affected_objects.len() > MAX_AFFECTED_OBJECTS {
        return Ok(blocked(
            "agent.too_many_affected_objects",
            statement_label(statement),
            &catalog_revision,
        ));
    }
    if mutates_existing_relation(statement)
        && visitor
            .mutation_targets
            .iter()
            .any(|target| !visitor.catalog_relations.contains(target))
    {
        return Ok(blocked(
            "agent.mutation_target_unknown",
            statement_label(statement),
            &catalog_revision,
        ));
    }
    let classification = classify_statement(
        statement,
        catalog_revision.clone(),
        visitor.affected_objects,
    );
    if classification.decision == AgentSqlDecision::SafeRead {
        if visitor.external_table_function {
            return Ok(blocked(
                "agent.external_table_function",
                statement_label(statement),
                &catalog_revision,
            ));
        }
        if visitor.user_defined_function {
            return Ok(blocked(
                "agent.function_provenance_unknown",
                statement_label(statement),
                &catalog_revision,
            ));
        }
        if visitor.side_effecting_function {
            return Ok(blocked(
                "agent.side_effecting_function",
                statement_label(statement),
                &catalog_revision,
            ));
        }
        // DuckDB must independently parse, bind, and plan the exact snapshot.
        // EXPLAIN is non-executing and rejects non-SELECT statements here.
        let explained = format!("EXPLAIN (FORMAT JSON) {sql}");
        if connection.prepare(&explained).is_err() {
            return Ok(blocked(
                "agent.duckdb_bind_rejected",
                "select",
                &catalog_revision,
            ));
        }
    }
    Ok(classification)
}

fn duckdb_statement_type(connection: &Connection, sql: &str) -> duckdb::Result<String> {
    let escaped = sql.replace('\\', "\\\\").replace('\'', "''");
    let serialized: String = connection.query_row(
        &format!("SELECT json_serialize_sql('{escaped}')"),
        [],
        |row| row.get(0),
    )?;
    let value: serde_json::Value = serde_json::from_str(&serialized).map_err(|error| {
        duckdb::Error::FromSqlConversionFailure(0, duckdb::types::Type::Text, Box::new(error))
    })?;
    match value.get("error").and_then(serde_json::Value::as_bool) {
        Some(false)
            if value
                .get("statements")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|statements| statements.len() == 1) =>
        {
            value["statements"][0]
                .pointer("/node/type")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .ok_or(duckdb::Error::InvalidQuery)
        }
        Some(true)
            if value.get("error_type").and_then(serde_json::Value::as_str)
                == Some("not implemented")
                && value
                    .get("error_message")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|message| message.contains("Only SELECT statements")) =>
        {
            Ok("NON_SELECT".into())
        }
        _ => Err(duckdb::Error::InvalidQuery),
    }
}

fn agent_catalog_revision(
    connection: &Connection,
    catalog: &CatalogSnapshot,
    registered_sources: &[AgentRegisteredSource],
) -> Result<String, EngineError> {
    let mut hash = 0xcbf29ce484222325u64;
    let mut write = |value: &str| {
        for byte in value.as_bytes().iter().copied().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    write(&catalog.revision);
    for source in registered_sources {
        write(&source.source_id);
        write(&source.database);
        write(&source.schema);
        write(&source.name);
        write(&source.kind);
        write(&source.state);
    }
    let mut statement = connection.prepare(
        "SELECT database_name, schema_name, function_name, function_type,
                coalesce(has_side_effects, true), internal,
                coalesce(macro_definition, '')
         FROM duckdb_functions()
         ORDER BY database_name, schema_name, function_name, function_type,
                  coalesce(macro_definition, '')",
    )?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, bool>(4)?,
            row.get::<_, bool>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    for row in rows {
        let (database, schema, name, kind, side_effects, internal, macro_definition) = row?;
        write(&database);
        write(&schema);
        write(&name);
        write(&kind);
        write(if side_effects { "1" } else { "0" });
        write(if internal { "1" } else { "0" });
        write(&macro_definition);
    }
    Ok(format!("agent-v1-{hash:016x}"))
}

fn parsers_agree(statement: &Statement, duckdb_type: &str) -> bool {
    matches!(
        (statement, duckdb_type),
        (Statement::Query(_), "SELECT_NODE")
            | (Statement::Insert(_), "INSERT_NODE")
            | (Statement::Update(_), "UPDATE_NODE")
            | (Statement::Delete(_), "DELETE_NODE")
            | (Statement::CreateTable(_), "CREATE_NODE")
            | (Statement::CreateView(_), "CREATE_NODE")
            | (Statement::AlterTable(_), "ALTER_NODE")
            | (Statement::Drop { .. }, "DROP_NODE")
            | (Statement::Truncate(_), "DELETE_NODE")
            | (Statement::Insert(_), "NON_SELECT")
            | (Statement::Update(_), "NON_SELECT")
            | (Statement::Delete(_), "NON_SELECT")
            | (Statement::CreateTable(_), "NON_SELECT")
            | (Statement::CreateView(_), "NON_SELECT")
            | (Statement::AlterTable(_), "NON_SELECT")
            | (Statement::Drop { .. }, "NON_SELECT")
            | (Statement::Truncate(_), "NON_SELECT")
    ) || (duckdb_type == "NON_SELECT" && !matches!(statement, Statement::Query(_)))
}

fn mutates_existing_relation(statement: &Statement) -> bool {
    matches!(
        statement,
        Statement::Insert(_)
            | Statement::Update(_)
            | Statement::Delete(_)
            | Statement::AlterTable(_)
            | Statement::Drop { .. }
            | Statement::Truncate(_)
    )
}

fn classify_statement(
    statement: &Statement,
    catalog_revision: String,
    affected_objects: Vec<String>,
) -> AgentSqlClassification {
    let (decision, reason_code, has_top_level_filter) = match statement {
        Statement::Query(query)
            if query.locks.is_empty()
                && query.settings.is_none()
                && query.format_clause.is_none()
                && query.pipe_operators.is_empty() =>
        {
            (AgentSqlDecision::SafeRead, "agent.safe_read", None)
        }
        Statement::Insert(insert) if plain_insert(insert) => (
            AgentSqlDecision::ApprovalRequired,
            "agent.insert_requires_approval",
            None,
        ),
        Statement::Update(update) => {
            let filtered = update.selection.is_some();
            (
                if filtered {
                    AgentSqlDecision::ApprovalRequired
                } else {
                    AgentSqlDecision::CriticalConfirmation
                },
                if filtered {
                    "agent.update_requires_approval"
                } else {
                    "agent.unfiltered_update_critical"
                },
                Some(filtered),
            )
        }
        Statement::Delete(delete) => {
            let filtered = delete.selection.is_some();
            (
                if filtered {
                    AgentSqlDecision::ApprovalRequired
                } else {
                    AgentSqlDecision::CriticalConfirmation
                },
                if filtered {
                    "agent.delete_requires_approval"
                } else {
                    "agent.unfiltered_delete_critical"
                },
                Some(filtered),
            )
        }
        Statement::CreateTable(table) if controlled_create_table(table) => (
            if table.or_replace {
                AgentSqlDecision::CriticalConfirmation
            } else {
                AgentSqlDecision::ApprovalRequired
            },
            if table.or_replace {
                "agent.replace_table_critical"
            } else {
                "agent.create_table_requires_approval"
            },
            None,
        ),
        Statement::CreateView(view) if controlled_create_view(view) => (
            if view.or_replace || view.or_alter {
                AgentSqlDecision::CriticalConfirmation
            } else {
                AgentSqlDecision::ApprovalRequired
            },
            if view.or_replace || view.or_alter {
                "agent.replace_view_critical"
            } else {
                "agent.create_view_requires_approval"
            },
            None,
        ),
        Statement::AlterTable(_) => (
            AgentSqlDecision::ApprovalRequired,
            "agent.alter_table_requires_approval",
            None,
        ),
        Statement::Drop {
            object_type: ObjectType::Table | ObjectType::View,
            ..
        }
        | Statement::Truncate(_) => (
            AgentSqlDecision::CriticalConfirmation,
            "agent.destructive_ddl_critical",
            None,
        ),
        _ => {
            return blocked(
                "agent.statement_blocked",
                statement_label(statement),
                &catalog_revision,
            )
        }
    };
    AgentSqlClassification {
        decision,
        reason_code: reason_code.into(),
        statement_type: statement_label(statement).into(),
        catalog_revision,
        affected_objects,
        has_top_level_filter,
    }
}

fn plain_insert(insert: &sqlparser::ast::Insert) -> bool {
    matches!(insert.table, TableObject::TableName(_))
        && !insert.overwrite
        && !insert.replace_into
        && insert.settings.is_none()
        && insert.format_clause.is_none()
        && insert.multi_table_insert_type.is_none()
        && insert.multi_table_into_clauses.is_empty()
        && insert.multi_table_when_clauses.is_empty()
        && insert.multi_table_else_clause.is_none()
}

fn controlled_create_table(table: &sqlparser::ast::CreateTable) -> bool {
    !table.external
        && !table.dynamic
        && !table.iceberg
        && !table.snapshot
        && table.location.is_none()
        && table.on_cluster.is_none()
        && table.external_volume.is_none()
        && table.base_location.is_none()
        && table.catalog.is_none()
        && table.catalog_sync.is_none()
        && table.warehouse.is_none()
}

fn controlled_create_view(view: &sqlparser::ast::CreateView) -> bool {
    !view.materialized
        && !view.secure
        && !view.with_no_schema_binding
        && view.to.is_none()
        && view.params.is_none()
}

fn blocked(reason: &str, statement_type: &str, catalog_revision: &str) -> AgentSqlClassification {
    AgentSqlClassification {
        decision: AgentSqlDecision::Blocked,
        reason_code: reason.into(),
        statement_type: statement_type.into(),
        catalog_revision: catalog_revision.into(),
        affected_objects: Vec::new(),
        has_top_level_filter: None,
    }
}

fn statement_label(statement: &Statement) -> &'static str {
    match statement {
        Statement::Query(_) => "select",
        Statement::Insert(_) => "insert",
        Statement::Update(_) => "update",
        Statement::Delete(_) => "delete",
        Statement::CreateTable(_) => "create_table",
        Statement::CreateView(_) => "create_view",
        Statement::AlterTable(_) => "alter_table",
        Statement::Drop { .. } => "drop",
        Statement::Truncate(_) => "truncate",
        Statement::Install { .. } => "install",
        Statement::Load { .. } => "load",
        Statement::CreateSecret { .. } | Statement::DropSecret { .. } => "secret",
        Statement::Copy { .. } => "copy",
        Statement::AttachDatabase { .. } | Statement::AttachDuckDBDatabase { .. } => "attach",
        Statement::DetachDuckDBDatabase { .. } => "detach",
        Statement::Set(_) => "set",
        Statement::Call(_) => "call",
        Statement::StartTransaction { .. }
        | Statement::Commit { .. }
        | Statement::Rollback { .. } => "transaction",
        _ => "unknown",
    }
}

struct PolicyVisitor {
    cte_names: HashSet<String>,
    catalog_relations: HashSet<String>,
    mutation_targets: HashSet<String>,
    affected_objects: Vec<String>,
    external_table_function: bool,
    user_defined_function: bool,
    side_effecting_function: bool,
}

impl PolicyVisitor {
    fn new(catalog: &CatalogSnapshot, registered_sources: &[AgentRegisteredSource]) -> Self {
        let registered_views = registered_sources
            .iter()
            .filter(|source| source.state == "ready")
            .flat_map(|source| {
                [
                    source.name.to_lowercase(),
                    format!("{}.{}", source.schema, source.name).to_lowercase(),
                    format!("{}.{}.{}", source.database, source.schema, source.name).to_lowercase(),
                ]
            })
            .collect::<HashSet<_>>();
        let catalog_relations = catalog
            .objects
            .iter()
            .filter(|object| {
                object.kind == "table" || registered_views.contains(&object.name.to_lowercase())
            })
            .flat_map(|object| {
                [
                    object.name.to_lowercase(),
                    format!("{}.{}", object.schema, object.name).to_lowercase(),
                    format!("{}.{}.{}", object.database, object.schema, object.name).to_lowercase(),
                ]
            })
            .collect();
        Self {
            cte_names: HashSet::new(),
            catalog_relations,
            mutation_targets: HashSet::new(),
            affected_objects: Vec::new(),
            external_table_function: false,
            user_defined_function: false,
            side_effecting_function: false,
        }
    }

    fn record_object(&mut self, name: &ObjectName) -> Result<(), &'static str> {
        let normalized = normalized_name(name).ok_or("agent.dynamic_identifier")?;
        self.mutation_targets.insert(normalized.clone());
        if !self.affected_objects.contains(&normalized) {
            self.affected_objects.push(normalized);
        }
        Ok(())
    }
}

impl Visitor for PolicyVisitor {
    type Break = &'static str;

    fn pre_visit_query(&mut self, query: &sqlparser::ast::Query) -> ControlFlow<Self::Break> {
        if let Some(with) = &query.with {
            for cte in &with.cte_tables {
                self.cte_names.insert(cte.alias.name.value.to_lowercase());
            }
        }
        if !query.locks.is_empty()
            || query.settings.is_some()
            || query.format_clause.is_some()
            || !query.pipe_operators.is_empty()
        {
            return ControlFlow::Break("agent.query_effect_blocked");
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, select: &sqlparser::ast::Select) -> ControlFlow<Self::Break> {
        if select.into.is_some() {
            return ControlFlow::Break("agent.select_into_blocked");
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_relation(&mut self, relation: &ObjectName) -> ControlFlow<Self::Break> {
        let Some(name) = normalized_name(relation) else {
            return ControlFlow::Break("agent.dynamic_identifier");
        };
        if self.cte_names.contains(&name)
            || self.catalog_relations.contains(&name)
            || self.mutation_targets.contains(&name)
        {
            ControlFlow::Continue(())
        } else {
            ControlFlow::Break("agent.relation_provenance_unknown")
        }
    }

    fn pre_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        match factor {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            }
            | TableFactor::Function { name, .. } => {
                if safe_table_function(name) {
                    if let Some(name) = normalized_name(name) {
                        self.cte_names.insert(name);
                    }
                    ControlFlow::Continue(())
                } else {
                    self.external_table_function = true;
                    ControlFlow::Break("agent.table_function_blocked")
                }
            }
            TableFactor::TableFunction { .. }
            | TableFactor::JsonTable { .. }
            | TableFactor::OpenJsonTable { .. }
            | TableFactor::XmlTable { .. }
            | TableFactor::SemanticView { .. } => {
                self.external_table_function = true;
                ControlFlow::Break("agent.table_function_blocked")
            }
            _ => ControlFlow::Continue(()),
        }
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        let Expr::Function(function) = expr else {
            return ControlFlow::Continue(());
        };
        let Some(name) = normalized_name(&function.name) else {
            return ControlFlow::Break("agent.dynamic_function_name");
        };
        if blocked_function(&name) {
            self.side_effecting_function = true;
            return ControlFlow::Break("agent.function_blocked");
        }
        if safe_scalar_function(&name) {
            ControlFlow::Continue(())
        } else {
            self.user_defined_function = true;
            ControlFlow::Break("agent.function_provenance_unknown")
        }
    }

    fn pre_visit_statement(&mut self, statement: &Statement) -> ControlFlow<Self::Break> {
        match statement {
            Statement::Insert(insert) => match &insert.table {
                TableObject::TableName(name) => {
                    if self.record_object(name).is_err() {
                        return ControlFlow::Break("agent.dynamic_identifier");
                    }
                }
                _ => return ControlFlow::Break("agent.dynamic_mutation_target"),
            },
            Statement::Update(update) => {
                if let TableFactor::Table {
                    name, args: None, ..
                } = &update.table.relation
                {
                    if self.record_object(name).is_err() {
                        return ControlFlow::Break("agent.dynamic_identifier");
                    }
                } else {
                    return ControlFlow::Break("agent.dynamic_mutation_target");
                }
            }
            Statement::Delete(delete) => {
                let tables = match &delete.from {
                    FromTable::WithFromKeyword(tables) | FromTable::WithoutKeyword(tables) => {
                        tables
                    }
                };
                if tables.len() != 1 {
                    return ControlFlow::Break("agent.multiple_mutation_targets");
                }
                if let TableFactor::Table {
                    name, args: None, ..
                } = &tables[0].relation
                {
                    if self.record_object(name).is_err() {
                        return ControlFlow::Break("agent.dynamic_identifier");
                    }
                } else {
                    return ControlFlow::Break("agent.dynamic_mutation_target");
                }
            }
            Statement::CreateTable(table) => {
                if self.record_object(&table.name).is_err() {
                    return ControlFlow::Break("agent.dynamic_identifier");
                }
            }
            Statement::CreateView(view) => {
                if self.record_object(&view.name).is_err() {
                    return ControlFlow::Break("agent.dynamic_identifier");
                }
            }
            Statement::AlterTable(table) => {
                if table.location.is_some()
                    || table.on_cluster.is_some()
                    || table.table_type.is_some()
                {
                    return ControlFlow::Break("agent.external_alter_blocked");
                }
                if self.record_object(&table.name).is_err() {
                    return ControlFlow::Break("agent.dynamic_identifier");
                }
            }
            Statement::Drop { names, .. } => {
                for name in names {
                    if self.record_object(name).is_err() {
                        return ControlFlow::Break("agent.dynamic_identifier");
                    }
                }
            }
            Statement::Truncate(truncate) => {
                for table in &truncate.table_names {
                    if self.record_object(&table.name).is_err() {
                        return ControlFlow::Break("agent.dynamic_identifier");
                    }
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

fn normalized_name(name: &ObjectName) -> Option<String> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(identifier) => Some(identifier.value.to_lowercase()),
            ObjectNamePart::Function(_) => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(|parts| parts.join("."))
}

fn safe_table_function(name: &ObjectName) -> bool {
    matches!(
        normalized_name(name).as_deref(),
        Some("range") | Some("generate_series") | Some("unnest")
    )
}

fn blocked_function(name: &str) -> bool {
    name.starts_with("read_")
        || name.starts_with("scan_")
        || name.contains("http")
        || name.contains("secret")
        || matches!(
            name,
            "glob"
                | "parquet_scan"
                | "csv_scan"
                | "read_csv"
                | "read_csv_auto"
                | "read_json"
                | "read_json_auto"
                | "read_ndjson"
                | "sqlite_scan"
                | "postgres_scan"
                | "mysql_scan"
                | "shell"
                | "system"
                | "current_setting"
                | "getenv"
                | "query"
                | "query_table"
        )
}

fn safe_scalar_function(name: &str) -> bool {
    if blocked_function(name) {
        return false;
    }
    const SAFE: &[&str] = &[
        "abs",
        "approx_count_distinct",
        "arg_max",
        "arg_min",
        "array_agg",
        "avg",
        "bit_and",
        "bit_count",
        "bit_or",
        "bool_and",
        "bool_or",
        "cast",
        "ceil",
        "ceiling",
        "coalesce",
        "concat",
        "concat_ws",
        "count",
        "count_if",
        "date_diff",
        "date_part",
        "date_trunc",
        "day",
        "dayofweek",
        "dayofyear",
        "ends_with",
        "epoch",
        "floor",
        "greatest",
        "hour",
        "ifnull",
        "isfinite",
        "isinf",
        "isnan",
        "last_day",
        "least",
        "left",
        "length",
        "list",
        "list_contains",
        "list_extract",
        "lower",
        "lpad",
        "ltrim",
        "max",
        "md5",
        "median",
        "min",
        "minute",
        "month",
        "nullif",
        "percentile_cont",
        "position",
        "regexp_extract",
        "regexp_matches",
        "replace",
        "reverse",
        "right",
        "round",
        "row_number",
        "rpad",
        "rtrim",
        "second",
        "sha256",
        "sign",
        "split_part",
        "starts_with",
        "stddev",
        "stddev_pop",
        "stddev_samp",
        "string_agg",
        "strpos",
        "substring",
        "sum",
        "trim",
        "try_cast",
        "typeof",
        "upper",
        "var_pop",
        "var_samp",
        "variance",
        "week",
        "year",
    ];
    SAFE.binary_search(&name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch("CREATE TABLE orders(id INTEGER, amount DOUBLE, note VARCHAR);")
            .unwrap();
        connection
    }

    #[test]
    fn safe_reads_require_known_relations_and_functions() {
        let connection = connection();
        for sql in [
            "SELECT id, sum(amount) FROM orders GROUP BY id",
            "WITH x AS (SELECT * FROM orders) SELECT count(*) FROM x",
            "SELECT * FROM range(5)",
            "SELECT $$semi;colon$$ AS value",
        ] {
            let classification = classify(&connection, sql, &[]).unwrap();
            assert_eq!(
                classification.decision,
                AgentSqlDecision::SafeRead,
                "{sql}: {classification:?}"
            );
        }
        for sql in [
            "SELECT * FROM missing",
            "SELECT random()",
            "SELECT current_setting('memory_limit')",
            "SELECT * FROM read_parquet('/tmp/private.parquet')",
            "SELECT * FROM read_csv_auto('https://example.invalid/x.csv')",
        ] {
            assert_eq!(
                classify(&connection, sql, &[]).unwrap().decision,
                AgentSqlDecision::Blocked,
                "{sql}"
            );
        }
    }

    #[test]
    fn mutations_are_never_safe_reads() {
        let connection = connection();
        let fixtures = [
            (
                "INSERT INTO orders VALUES (1, 2, 'x')",
                AgentSqlDecision::ApprovalRequired,
            ),
            (
                "UPDATE orders SET amount = 2 WHERE id = 1",
                AgentSqlDecision::ApprovalRequired,
            ),
            (
                "UPDATE orders SET amount = 2",
                AgentSqlDecision::CriticalConfirmation,
            ),
            (
                "DELETE FROM orders WHERE id = 1",
                AgentSqlDecision::ApprovalRequired,
            ),
            ("DELETE FROM orders", AgentSqlDecision::CriticalConfirmation),
            (
                "CREATE TABLE copy(id INTEGER)",
                AgentSqlDecision::ApprovalRequired,
            ),
            (
                "CREATE OR REPLACE TABLE copy AS SELECT * FROM orders",
                AgentSqlDecision::CriticalConfirmation,
            ),
            ("DROP TABLE orders", AgentSqlDecision::CriticalConfirmation),
            ("TRUNCATE orders", AgentSqlDecision::CriticalConfirmation),
        ];
        for (sql, expected) in fixtures {
            let classification = classify(&connection, sql, &[]).unwrap();
            assert_eq!(
                classification.decision, expected,
                "{sql}: {classification:?}"
            );
        }
    }

    #[test]
    fn ordinary_views_are_blocked_but_registered_ready_sources_are_bound() {
        let connection = connection();
        connection
            .execute_batch("CREATE VIEW linked_orders AS SELECT * FROM orders;")
            .unwrap();
        assert_eq!(
            classify(&connection, "SELECT * FROM linked_orders", &[])
                .unwrap()
                .decision,
            AgentSqlDecision::Blocked
        );
        let source = AgentRegisteredSource {
            source_id: "source-1".into(),
            database: "memory".into(),
            schema: "main".into(),
            name: "linked_orders".into(),
            kind: "linked_parquet".into(),
            state: "ready".into(),
        };
        let classified = classify(&connection, "SELECT * FROM linked_orders", &[source]).unwrap();
        assert_eq!(classified.decision, AgentSqlDecision::SafeRead);
        assert!(classified.catalog_revision.starts_with("agent-v1-"));
    }

    #[test]
    fn uncontrollable_operations_have_no_approval_route() {
        let connection = connection();
        for sql in [
            "SELECT 1; DELETE FROM orders",
            "INSTALL httpfs",
            "LOAD httpfs",
            "CREATE SECRET s (TYPE S3, KEY_ID 'x', SECRET 'y')",
            "COPY orders TO '/tmp/out.parquet' (FORMAT PARQUET)",
            "ATTACH '/tmp/other.duckdb' AS other",
            "SET memory_limit='1GB'",
            "PRAGMA threads=8",
            "CALL checkpoint()",
            "BEGIN TRANSACTION",
            "CREATE MACRO add_one(x) AS x + 1",
        ] {
            assert_eq!(
                classify(&connection, sql, &[]).unwrap().decision,
                AgentSqlDecision::Blocked,
                "{sql}"
            );
        }
    }
}

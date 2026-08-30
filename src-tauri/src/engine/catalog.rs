use duckdb::Connection;
use serde::{Deserialize, Serialize};

use super::{CatalogColumn, CatalogObject, CatalogObjectKind, EngineError, ProjectCatalog};

pub(super) fn inspect(connection: &Connection) -> Result<ProjectCatalog, EngineError> {
    let mut objects_statement = connection.prepare(
        "SELECT database_name, schema_name, table_name, 'TABLE', estimated_size
         FROM duckdb_tables()
         UNION ALL
         SELECT database_name, schema_name, view_name, 'VIEW', NULL::BIGINT
         FROM duckdb_views()
         WHERE internal = false
         ORDER BY database_name, schema_name, table_name",
    )?;
    let objects = objects_statement
        .query_map([], |row| {
            let kind: String = row.get(3)?;
            Ok(CatalogObject {
                database: row.get(0)?,
                schema: row.get(1)?,
                name: row.get(2)?,
                kind: if kind.eq_ignore_ascii_case("VIEW") {
                    CatalogObjectKind::View
                } else {
                    CatalogObjectKind::Table
                },
                estimated_row_count: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut columns_statement = connection.prepare(
        "SELECT database_name, schema_name, table_name, column_name, data_type,
                column_index, is_nullable
         FROM duckdb_columns()
         ORDER BY database_name, schema_name, table_name, column_index",
    )?;
    let columns = columns_statement
        .query_map([], |row| {
            let nullable: bool = row.get(6)?;
            Ok(CatalogColumn {
                database: row.get(0)?,
                schema: row.get(1)?,
                object: row.get(2)?,
                name: row.get(3)?,
                data_type: row.get(4)?,
                position: row.get(5)?,
                nullable,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(ProjectCatalog { objects, columns })
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogFixture {
    objects: Vec<CatalogObject>,
    columns: Vec<CatalogColumn>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_multiple_schemas_views_and_unusual_identifiers() {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE SCHEMA analytics;
                 CREATE TABLE analytics.\"order lines\"(\"order id\" BIGINT NOT NULL, amount DECIMAL(18,2));
                 CREATE VIEW analytics.order_totals AS SELECT \"order id\", sum(amount) total
                 FROM analytics.\"order lines\" GROUP BY \"order id\";",
            )
            .unwrap();

        let catalog = inspect(&connection).unwrap();

        assert!(catalog.objects.iter().any(|object| {
            object.schema == "analytics"
                && object.name == "order lines"
                && object.kind == CatalogObjectKind::Table
                && object.estimated_row_count == Some(0)
        }));
        assert!(catalog.objects.iter().any(|object| {
            object.schema == "analytics"
                && object.name == "order_totals"
                && object.kind == CatalogObjectKind::View
        }));
        assert!(catalog.columns.iter().any(|column| {
            column.object == "order lines" && column.name == "order id" && !column.nullable
        }));
    }
}

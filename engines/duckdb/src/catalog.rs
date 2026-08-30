use duckdb::Connection;
use tarik_engine_protocol::{CatalogColumn, CatalogObject, CatalogSnapshot};

use crate::error::EngineError;

pub fn inspect(connection: &Connection) -> Result<CatalogSnapshot, EngineError> {
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
            let estimated: Option<i64> = row.get(4)?;
            Ok(CatalogObject {
                database: row.get(0)?,
                schema: row.get(1)?,
                name: row.get(2)?,
                kind: kind.to_ascii_lowercase(),
                estimated_row_count: estimated.map(|value| value.max(0) as u64),
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

    Ok(CatalogSnapshot { objects, columns })
}

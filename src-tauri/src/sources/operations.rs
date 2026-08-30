use std::path::{Path, PathBuf};

use duckdb::Connection;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::metadata::sources::{SourceKind, SourceRecord, SourceState};

use super::{
    detect_format, quote_identifier, relation_sql, ImportOptions, SourceError, SourceFormat,
    SourceInspection,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMutationResult {
    pub source: SourceRecord,
    pub inspection: SourceInspection,
}

pub fn link_parquet(
    connection: &Connection,
    project_id: &str,
    path: &Path,
    view_name: &str,
) -> Result<SourceMutationResult, SourceError> {
    if detect_format(path)? != SourceFormat::Parquet {
        return Err(SourceError::InvalidOptions(
            "only Parquet files can be linked",
        ));
    }
    let inspection = super::inspect(path, None)?;
    let identifier = quote_identifier(view_name)?;
    let relation = relation_sql(path, SourceFormat::Parquet, None)?;
    connection.execute_batch("BEGIN TRANSACTION")?;
    let result = connection.execute_batch(&format!(
        "CREATE VIEW {identifier} AS SELECT * FROM {relation}"
    ));
    match result {
        Ok(()) => connection.execute_batch("COMMIT")?,
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error.into());
        }
    }
    let columns = inspection
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    Ok(SourceMutationResult {
        source: source_record(
            project_id,
            view_name,
            SourceKind::LinkedParquet,
            Some(path.to_path_buf()),
            serde_json::json!({"mode": "link", "columns": columns}),
        ),
        inspection,
    })
}

pub fn import_table(
    connection: &Connection,
    project_id: &str,
    path: &Path,
    options: &ImportOptions,
) -> Result<SourceMutationResult, SourceError> {
    let format = detect_format(path)?;
    validate_import_options(options)?;
    let inspection = super::inspect(path, options.csv.as_ref())?;
    let identifier = quote_identifier(&options.table_name)?;
    let relation = relation_sql(path, format, options.csv.as_ref())?;
    connection.execute_batch("BEGIN TRANSACTION")?;
    let import_result = (|| -> Result<(), SourceError> {
        connection.execute_batch(&format!(
            "CREATE TABLE {identifier} AS SELECT * FROM {relation}"
        ))?;
        for override_column in &options.column_overrides {
            let column = quote_identifier(&override_column.column)?;
            validate_type(&override_column.data_type)?;
            connection.execute_batch(&format!(
                "ALTER TABLE {identifier} ALTER COLUMN {column} SET DATA TYPE {}",
                override_column.data_type
            ))?;
        }
        Ok(())
    })();
    match import_result {
        Ok(()) => connection.execute_batch("COMMIT")?,
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            return Err(error);
        }
    }
    let exact_imported_rows: u64 = connection.query_row(
        &format!("SELECT count(*)::UBIGINT FROM {identifier}"),
        [],
        |row| row.get(0),
    )?;
    Ok(SourceMutationResult {
        source: source_record(
            project_id,
            &options.table_name,
            SourceKind::DuckdbTable,
            Some(path.to_path_buf()),
            serde_json::json!({
                "mode": "import",
                "format": format,
                "rowCount": exact_imported_rows,
                "rowCountExact": true,
                "fileSizeBytes": inspection.file_size_bytes
            }),
        ),
        inspection,
    })
}

pub fn repair_link(
    connection: &Connection,
    source: &SourceRecord,
    replacement: &Path,
) -> Result<SourceMutationResult, SourceError> {
    if source.kind != SourceKind::LinkedParquet {
        return Err(SourceError::InvalidOptions(
            "only linked Parquet sources can be repaired",
        ));
    }
    let replacement_inspection = super::inspect(replacement, None)?;
    let expected_columns = source
        .options
        .get("columns")
        .and_then(serde_json::Value::as_array)
        .map(|columns| {
            columns
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let actual_columns = replacement_inspection
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect::<Vec<_>>();
    if !expected_columns.is_empty() && expected_columns != actual_columns {
        return Err(SourceError::IncompatibleSchema {
            expected: expected_columns,
            actual: actual_columns,
        });
    }
    let identifier = quote_identifier(&source.duckdb_name)?;
    let relation = relation_sql(replacement, SourceFormat::Parquet, None)?;
    connection.execute_batch(&format!(
        "CREATE OR REPLACE VIEW {identifier} AS SELECT * FROM {relation}"
    ))?;
    let mut repaired = source.clone();
    repaired.state = SourceState::Ready;
    repaired.source_path = Some(replacement.to_string_lossy().into_owned());
    Ok(SourceMutationResult {
        source: repaired,
        inspection: replacement_inspection,
    })
}

pub fn check_link_health(source: &SourceRecord) -> SourceState {
    match source.kind {
        SourceKind::LinkedParquet | SourceKind::LinkedCsv => source
            .source_path
            .as_ref()
            .map(Path::new)
            .filter(|path| path.is_file())
            .map(|_| SourceState::Ready)
            .unwrap_or(SourceState::Missing),
        SourceKind::DuckdbTable => SourceState::Ready,
    }
}

pub fn drop_link(connection: &Connection, source: &SourceRecord) -> Result<(), SourceError> {
    if matches!(
        source.kind,
        SourceKind::LinkedParquet | SourceKind::LinkedCsv
    ) {
        connection.execute_batch(&format!(
            "DROP VIEW IF EXISTS {}",
            quote_identifier(&source.duckdb_name)?
        ))?;
    }
    Ok(())
}

fn source_record(
    project_id: &str,
    name: &str,
    kind: SourceKind,
    path: Option<PathBuf>,
    options: serde_json::Value,
) -> SourceRecord {
    SourceRecord {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_owned(),
        display_name: name.to_owned(),
        kind,
        state: SourceState::Ready,
        source_path: path.map(|path| path.to_string_lossy().into_owned()),
        duckdb_name: name.to_owned(),
        options,
        created_at: now_text(),
        updated_at: now_text(),
    }
}

fn now_text() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn validate_import_options(options: &ImportOptions) -> Result<(), SourceError> {
    quote_identifier(&options.table_name)?;
    for override_column in &options.column_overrides {
        quote_identifier(&override_column.column)?;
        validate_type(&override_column.data_type)?;
    }
    Ok(())
}

fn validate_type(data_type: &str) -> Result<(), SourceError> {
    let allowed = [
        "BOOLEAN",
        "TINYINT",
        "SMALLINT",
        "INTEGER",
        "BIGINT",
        "HUGEINT",
        "FLOAT",
        "DOUBLE",
        "DECIMAL",
        "VARCHAR",
        "DATE",
        "TIME",
        "TIMESTAMP",
        "BLOB",
    ];
    let upper = data_type.trim().to_ascii_uppercase();
    let base = upper.split(['(', '[', ' ']).next().unwrap_or_default();
    if allowed.contains(&base)
        && upper.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '(' | ')' | ',' | ' ')
        })
    {
        Ok(())
    } else {
        Err(SourceError::InvalidDataType(data_type.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn root() -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("tarik-operations-{stamp}"));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn links_parquet_with_quote_in_path_and_joins_imported_table() {
        let root = root();
        let parquet = root.join("customer's orders.parquet");
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "COPY (SELECT 1 id, 12 amount) TO {} (FORMAT PARQUET); CREATE TABLE customers(id INTEGER, name VARCHAR); INSERT INTO customers VALUES (1, 'Ari');",
                super::super::sql_literal(parquet.to_str().unwrap())
            ))
            .unwrap();

        link_parquet(&connection, "p1", &parquet, "order lines").unwrap();
        let name: String = connection
            .query_row(
                "SELECT name FROM \"order lines\" JOIN customers USING(id)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(name, "Ari");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn import_survives_original_file_move_and_rolls_back_duplicate() {
        let root = root();
        let csv = root.join("orders.csv");
        std::fs::write(&csv, "id,amount\n1,12\n").unwrap();
        let connection = Connection::open_in_memory().unwrap();
        let options = ImportOptions {
            table_name: "orders".into(),
            csv: Some(super::super::CsvOptions::default()),
            column_overrides: Vec::new(),
        };
        let result = import_table(&connection, "p1", &csv, &options).unwrap();
        assert_eq!(result.source.options["rowCount"], 1);
        assert_eq!(result.source.options["rowCountExact"], true);
        std::fs::remove_file(&csv).unwrap();
        let count: i64 = connection
            .query_row("SELECT count(*) FROM orders", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
        assert!(import_table(&connection, "p1", &csv, &options).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn detects_missing_link_and_rejects_incompatible_replacement() {
        let root = root();
        let old = root.join("old.parquet");
        let replacement = root.join("replacement.parquet");
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "COPY (SELECT 1 id) TO {} (FORMAT PARQUET); COPY (SELECT 1 other) TO {} (FORMAT PARQUET)",
                super::super::sql_literal(old.to_str().unwrap()),
                super::super::sql_literal(replacement.to_str().unwrap())
            ))
            .unwrap();
        let mut source = link_parquet(&connection, "p1", &old, "orders")
            .unwrap()
            .source;
        source.options["columns"] = serde_json::json!(["id"]);
        std::fs::remove_file(&old).unwrap();
        assert_eq!(check_link_health(&source), SourceState::Missing);
        assert!(matches!(
            repair_link(&connection, &source, &replacement),
            Err(SourceError::IncompatibleSchema { .. })
        ));
        let _ = std::fs::remove_dir_all(root);
    }
}

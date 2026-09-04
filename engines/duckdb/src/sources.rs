use std::path::{Path, PathBuf};

use duckdb::{types::ValueRef, Connection};
use tarik_engine_protocol::{
    CreateTableDefinition, CsvOptions, ImportOptions, SourceColumn, SourceInspection, SourceKind,
    SourceRecord, SourceState,
};
use uuid::Uuid;

use crate::error::EngineError;

pub fn detect_format(path: &Path) -> Result<&'static str, EngineError> {
    if has_glob(path) {
        let matches = glob::glob(path.to_string_lossy().as_ref())
            .map_err(|_| EngineError::InvalidGlob(path.to_path_buf()))?
            .filter_map(Result::ok)
            .filter(|candidate| candidate.is_file())
            .count();
        if matches == 0 {
            return Err(EngineError::Missing(path.to_path_buf()));
        }
    } else if !path.is_file() {
        return Err(EngineError::Missing(path.to_path_buf()));
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => Ok("csv"),
        Some("parquet") | Some("pq") => Ok("parquet"),
        _ => Err(EngineError::Unsupported(path.to_path_buf())),
    }
}

pub fn inspect(path: &Path, csv: Option<&CsvOptions>) -> Result<SourceInspection, EngineError> {
    let format = detect_format(path)?;
    let connection = Connection::open_in_memory()?;
    let relation = relation_sql(path, format, csv)?;
    let description = format!("DESCRIBE SELECT * FROM {relation}");
    let mut statement = connection.prepare(&description)?;
    let columns = statement
        .query_map([], |row| {
            let nullable: String = row.get(2)?;
            Ok(SourceColumn {
                name: row.get(0)?,
                data_type: row.get(1)?,
                nullable: nullable.eq_ignore_ascii_case("YES"),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let preview_sql = format!("SELECT * FROM {relation} LIMIT 25");
    let mut preview_statement = connection.prepare(&preview_sql)?;
    let mut rows = preview_statement.query([])?;
    let column_count = rows
        .as_ref()
        .map(duckdb::Statement::column_count)
        .unwrap_or(0);
    let mut preview_rows = Vec::new();
    while let Some(row) = rows.next()? {
        preview_rows.push(
            (0..column_count)
                .map(|index| row.get_ref(index).map(value_to_json))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }

    let file_size_bytes = source_size(path)?;
    let (row_count, row_count_exact) =
        source_cardinality(&connection, path, format, csv, file_size_bytes)?;

    Ok(SourceInspection {
        path: path.to_string_lossy().into_owned(),
        format: format.to_string(),
        suggested_name: suggested_name(path),
        file_size_bytes,
        row_count,
        row_count_exact,
        columns,
        preview_rows,
        csv_options: (format == "csv").then(|| csv.cloned().unwrap_or_default()),
        warnings: Vec::new(),
    })
}

pub fn link_parquet(
    connection: &Connection,
    project_id: &str,
    path: &Path,
    view_name: &str,
) -> Result<SourceRecord, EngineError> {
    if detect_format(path)? != "parquet" {
        return Err(EngineError::InvalidOptions(
            "only Parquet files can be linked",
        ));
    }
    let inspection = inspect(path, None)?;
    let identifier = quote_identifier(view_name)?;
    let relation = relation_sql(path, "parquet", None)?;
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
    Ok(source_record(
        project_id,
        view_name,
        SourceKind::LinkedParquet,
        Some(path.to_path_buf()),
        serde_json::json!({ "mode": "link", "columns": columns }),
    ))
}

pub fn create_table(
    connection: &Connection,
    definition: &CreateTableDefinition,
) -> Result<(), EngineError> {
    let table = quote_identifier(&definition.name)?;
    if definition.columns.is_empty() {
        return Err(EngineError::InvalidOptions(
            "a table requires at least one column",
        ));
    }
    let mut observed = std::collections::HashSet::new();
    let mut columns = Vec::with_capacity(definition.columns.len());
    for column in &definition.columns {
        let name = column.name.trim();
        let folded = name.to_lowercase();
        if !observed.insert(folded) {
            return Err(EngineError::InvalidOptions("column names must be unique"));
        }
        let identifier = quote_identifier(name)?;
        let data_type = validate_simple_type(&column.data_type)?;
        columns.push(format!(
            "{identifier} {data_type}{}",
            if column.nullable { "" } else { " NOT NULL" }
        ));
    }
    connection.execute_batch(&format!("CREATE TABLE {table} ({})", columns.join(", ")))?;
    Ok(())
}

pub fn import_table(
    connection: &Connection,
    project_id: &str,
    path: &Path,
    options: &ImportOptions,
) -> Result<SourceRecord, EngineError> {
    let format = detect_format(path)?;
    validate_import_options(options)?;
    let inspection = inspect(path, options.csv.as_ref())?;
    let identifier = quote_identifier(&options.table_name)?;
    let relation = relation_sql(path, format, options.csv.as_ref())?;
    connection.execute_batch("BEGIN TRANSACTION")?;
    let import_result = (|| -> Result<(), EngineError> {
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
    Ok(source_record(
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
    ))
}

pub fn repair_link(
    connection: &Connection,
    source: &SourceRecord,
    replacement: &Path,
) -> Result<SourceRecord, EngineError> {
    if source.kind != SourceKind::LinkedParquet {
        return Err(EngineError::InvalidOptions(
            "only linked Parquet sources can be repaired",
        ));
    }
    let replacement_inspection = inspect(replacement, None)?;
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
        return Err(EngineError::IncompatibleSchema {
            expected: expected_columns,
            actual: actual_columns,
        });
    }
    let identifier = quote_identifier(&source.duckdb_name)?;
    let relation = relation_sql(replacement, "parquet", None)?;
    connection.execute_batch(&format!(
        "CREATE OR REPLACE VIEW {identifier} AS SELECT * FROM {relation}"
    ))?;
    let mut repaired = source.clone();
    repaired.state = SourceState::Ready;
    repaired.source_path = Some(replacement.to_string_lossy().into_owned());
    Ok(repaired)
}

pub fn drop_link(connection: &Connection, source: &SourceRecord) -> Result<(), EngineError> {
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
        options: options.as_object().cloned().unwrap_or_default(),
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

fn has_glob(path: &Path) -> bool {
    path.to_string_lossy()
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

fn source_size(path: &Path) -> Result<u64, EngineError> {
    if has_glob(path) {
        glob::glob(path.to_string_lossy().as_ref())
            .map_err(|_| EngineError::InvalidGlob(path.to_path_buf()))?
            .filter_map(Result::ok)
            .try_fold(0_u64, |total, candidate| {
                candidate
                    .metadata()
                    .map(|metadata| total.saturating_add(metadata.len()))
                    .map_err(|source| EngineError::Io {
                        path: candidate,
                        source,
                    })
            })
    } else {
        path.metadata()
            .map(|metadata| metadata.len())
            .map_err(|source| EngineError::Io {
                path: path.to_path_buf(),
                source,
            })
    }
}

const EXACT_CSV_COUNT_MAX_BYTES: u64 = 8 * 1024 * 1024;

fn source_cardinality(
    connection: &Connection,
    path: &Path,
    format: &str,
    csv: Option<&CsvOptions>,
    file_size_bytes: u64,
) -> Result<(u64, bool), EngineError> {
    if format == "parquet" || file_size_bytes <= EXACT_CSV_COUNT_MAX_BYTES {
        let relation = relation_sql(path, format, csv)?;
        let count: u64 = connection.query_row(
            &format!("SELECT count(*)::UBIGINT FROM {relation}"),
            [],
            |row| row.get(0),
        )?;
        return Ok((count, true));
    }

    let sample_bytes = read_sample(path, 1024 * 1024)?;
    let lines = sample_bytes.iter().filter(|byte| **byte == b'\n').count() as u64;
    if lines == 0 || sample_bytes.is_empty() {
        return Ok((0, false));
    }
    let average_line_bytes = sample_bytes.len() as f64 / lines as f64;
    let mut estimate = (file_size_bytes as f64 / average_line_bytes).round() as u64;
    if csv.cloned().unwrap_or_default().has_header {
        estimate = estimate.saturating_sub(1);
    }
    Ok((estimate, false))
}

fn read_sample(path: &Path, limit: u64) -> Result<Vec<u8>, EngineError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|source| EngineError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut sample = Vec::new();
    file.take(limit)
        .read_to_end(&mut sample)
        .map_err(|source| EngineError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(sample)
}

pub(crate) fn relation_sql(
    path: &Path,
    format: &str,
    csv: Option<&CsvOptions>,
) -> Result<String, EngineError> {
    let path_literal = sql_literal(
        path.to_str()
            .ok_or_else(|| EngineError::InvalidPath(path.to_path_buf()))?,
    );
    Ok(match format {
        "parquet" => format!("read_parquet({path_literal})"),
        "csv" => {
            let options = csv.cloned().unwrap_or_default();
            let delimiter = options.delimiter.chars().collect::<Vec<_>>();
            if delimiter.len() != 1 {
                return Err(EngineError::InvalidOptions(
                    "CSV delimiter must contain exactly one character",
                ));
            }
            let mut arguments = vec![
                format!("header = {}", options.has_header),
                format!("delim = {}", sql_literal(&options.delimiter)),
                format!("all_varchar = {}", options.all_varchar),
                "sample_size = 20480".into(),
            ];
            if let Some(null_value) = options.null_value {
                arguments.push(format!("nullstr = {}", sql_literal(&null_value)));
            }
            format!("read_csv({path_literal}, {})", arguments.join(", "))
        }
        _ => return Err(EngineError::Unsupported(path.to_path_buf())),
    })
}

pub(crate) fn quote_identifier(identifier: &str) -> Result<String, EngineError> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return Err(EngineError::InvalidIdentifier);
    }
    Ok(format!("\"{}\"", identifier.replace('"', "\"\"")))
}

pub(crate) fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(crate) fn suggested_name(path: &Path) -> String {
    let raw = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported_data");
    let mut name = String::new();
    for character in raw.chars() {
        if character.is_alphanumeric() || character == '_' {
            name.push(character.to_ascii_lowercase());
        } else if !name.ends_with('_') && !name.is_empty() {
            name.push('_');
        }
    }
    let name = name.trim_matches('_');
    if name.is_empty() {
        "imported_data".into()
    } else if name
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_digit())
    {
        format!("data_{name}")
    } else {
        name.into()
    }
}

fn validate_import_options(options: &ImportOptions) -> Result<(), EngineError> {
    quote_identifier(&options.table_name)?;
    for override_column in &options.column_overrides {
        quote_identifier(&override_column.column)?;
        validate_type(&override_column.data_type)?;
    }
    Ok(())
}

fn validate_simple_type(data_type: &str) -> Result<&'static str, EngineError> {
    let allowed = [
        "BOOLEAN",
        "INTEGER",
        "BIGINT",
        "DOUBLE",
        "DECIMAL",
        "VARCHAR",
        "DATE",
        "TIMESTAMP",
    ];
    let upper = data_type.trim().to_ascii_uppercase();
    allowed
        .into_iter()
        .find(|candidate| *candidate == upper)
        .ok_or_else(|| EngineError::InvalidDataType(data_type.to_owned()))
}

fn validate_type(data_type: &str) -> Result<(), EngineError> {
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
        Err(EngineError::InvalidDataType(data_type.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tarik_engine_protocol::{CreateTableColumn, CreateTableDefinition};

    fn definition(name: &str, columns: Vec<(&str, &str, bool)>) -> CreateTableDefinition {
        CreateTableDefinition {
            name: name.into(),
            columns: columns
                .into_iter()
                .map(|(name, data_type, nullable)| CreateTableColumn {
                    name: name.into(),
                    data_type: data_type.into(),
                    nullable,
                })
                .collect(),
        }
    }

    #[test]
    fn creates_safely_quoted_empty_table_with_nullability() {
        let connection = Connection::open_in_memory().unwrap();
        create_table(
            &connection,
            &definition(
                "order summary",
                vec![("select", "BIGINT", false), ("net value", "DOUBLE", true)],
            ),
        )
        .unwrap();
        let count: u64 = connection
            .query_row("SELECT count(*) FROM \"order summary\"", [], |row| {
                row.get(0)
            })
            .unwrap();
        let not_null: bool = connection
            .query_row(
                "SELECT \"notnull\" FROM pragma_table_info('order summary') WHERE name = 'select'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert!(not_null);
    }

    #[test]
    fn rejects_duplicate_columns_and_unlisted_type_without_creating_table() {
        let connection = Connection::open_in_memory().unwrap();
        assert!(create_table(
            &connection,
            &definition(
                "unsafe",
                vec![("id", "BIGINT", true), ("ID", "VARCHAR", true)]
            ),
        )
        .is_err());
        assert!(create_table(
            &connection,
            &definition("unsafe", vec![("payload", "VARCHAR); DROP TABLE x;", true)]),
        )
        .is_err());
        let count: u64 = connection
            .query_row(
                "SELECT count(*) FROM information_schema.tables WHERE table_name = 'unsafe'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}

fn value_to_json(value: ValueRef<'_>) -> serde_json::Value {
    match value {
        ValueRef::Null => serde_json::Value::Null,
        ValueRef::Boolean(value) => value.into(),
        ValueRef::TinyInt(value) => value.into(),
        ValueRef::SmallInt(value) => value.into(),
        ValueRef::Int(value) => value.into(),
        ValueRef::BigInt(value) => value.into(),
        ValueRef::UTinyInt(value) => value.into(),
        ValueRef::USmallInt(value) => value.into(),
        ValueRef::UInt(value) => value.into(),
        ValueRef::UBigInt(value) => value.into(),
        ValueRef::Float(value) => serde_json::json!(value),
        ValueRef::Double(value) => serde_json::json!(value),
        ValueRef::Text(value) => String::from_utf8_lossy(value).into_owned().into(),
        ValueRef::Blob(value) | ValueRef::Geometry(value) => {
            format!("<{} bytes>", value.len()).into()
        }
        other => format!("{other:?}").into(),
    }
}

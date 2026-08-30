pub mod operations;

use std::path::{Path, PathBuf};

use duckdb::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Csv,
    Parquet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInspection {
    pub path: PathBuf,
    pub format: SourceFormat,
    pub suggested_name: String,
    pub file_size_bytes: u64,
    pub row_count: u64,
    pub row_count_exact: bool,
    pub columns: Vec<SourceColumn>,
    pub preview_rows: Vec<Vec<serde_json::Value>>,
    pub csv_options: Option<CsvOptions>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvOptions {
    pub delimiter: String,
    pub has_header: bool,
    pub null_value: Option<String>,
    pub all_varchar: bool,
}

impl Default for CsvOptions {
    fn default() -> Self {
        Self {
            delimiter: ",".into(),
            has_header: true,
            null_value: None,
            all_varchar: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnOverride {
    pub column: String,
    pub data_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOptions {
    pub table_name: String,
    pub csv: Option<CsvOptions>,
    pub column_overrides: Vec<ColumnOverride>,
}

const EXACT_CSV_COUNT_MAX_BYTES: u64 = 8 * 1024 * 1024;

pub fn detect_format(path: &Path) -> Result<SourceFormat, SourceError> {
    if has_glob(path) {
        let matches = glob::glob(path.to_string_lossy().as_ref())
            .map_err(|_| SourceError::InvalidGlob(path.to_path_buf()))?
            .filter_map(Result::ok)
            .filter(|candidate| candidate.is_file())
            .count();
        if matches == 0 {
            return Err(SourceError::Missing(path.to_path_buf()));
        }
    } else if !path.is_file() {
        return Err(SourceError::Missing(path.to_path_buf()));
    }
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("csv") => Ok(SourceFormat::Csv),
        Some("parquet") | Some("pq") => Ok(SourceFormat::Parquet),
        _ => Err(SourceError::Unsupported(path.to_path_buf())),
    }
}

pub fn inspect(path: &Path, csv: Option<&CsvOptions>) -> Result<SourceInspection, SourceError> {
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
        path: path.to_path_buf(),
        format,
        suggested_name: suggested_name(path),
        file_size_bytes,
        row_count,
        row_count_exact,
        columns,
        preview_rows,
        csv_options: (format == SourceFormat::Csv).then(|| csv.cloned().unwrap_or_default()),
        warnings: Vec::new(),
    })
}

fn source_size(path: &Path) -> Result<u64, SourceError> {
    if has_glob(path) {
        glob::glob(path.to_string_lossy().as_ref())
            .map_err(|_| SourceError::InvalidGlob(path.to_path_buf()))?
            .filter_map(Result::ok)
            .try_fold(0_u64, |total, candidate| {
                candidate
                    .metadata()
                    .map(|metadata| total.saturating_add(metadata.len()))
                    .map_err(|source| SourceError::Metadata {
                        path: candidate,
                        source,
                    })
            })
    } else {
        path.metadata()
            .map(|metadata| metadata.len())
            .map_err(|source| SourceError::Metadata {
                path: path.to_path_buf(),
                source,
            })
    }
}

fn source_cardinality(
    connection: &Connection,
    path: &Path,
    format: SourceFormat,
    csv: Option<&CsvOptions>,
    file_size_bytes: u64,
) -> Result<(u64, bool), SourceError> {
    if format == SourceFormat::Parquet || file_size_bytes <= EXACT_CSV_COUNT_MAX_BYTES {
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

fn read_sample(path: &Path, limit: u64) -> Result<Vec<u8>, SourceError> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|source| SourceError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let mut sample = Vec::new();
    file.take(limit)
        .read_to_end(&mut sample)
        .map_err(|source| SourceError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    Ok(sample)
}

fn has_glob(path: &Path) -> bool {
    path.to_string_lossy()
        .chars()
        .any(|character| matches!(character, '*' | '?' | '['))
}

pub(crate) fn relation_sql(
    path: &Path,
    format: SourceFormat,
    csv: Option<&CsvOptions>,
) -> Result<String, SourceError> {
    let path = sql_literal(
        path.to_str()
            .ok_or_else(|| SourceError::InvalidPath(path.to_path_buf()))?,
    );
    Ok(match format {
        SourceFormat::Parquet => format!("read_parquet({path})"),
        SourceFormat::Csv => {
            let options = csv.cloned().unwrap_or_default();
            let delimiter = options.delimiter.chars().collect::<Vec<_>>();
            if delimiter.len() != 1 {
                return Err(SourceError::InvalidOptions(
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
            format!("read_csv({path}, {})", arguments.join(", "))
        }
    })
}

pub(crate) fn quote_identifier(identifier: &str) -> Result<String, SourceError> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return Err(SourceError::InvalidIdentifier);
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

#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("source file does not exist: {0}")]
    Missing(PathBuf),
    #[error("unsupported source type: {0}")]
    Unsupported(PathBuf),
    #[error("source path is not valid UTF-8: {0}")]
    InvalidPath(PathBuf),
    #[error("could not read source metadata at {path}: {source}")]
    Metadata {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read source sample at {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("source glob is invalid: {0}")]
    InvalidGlob(PathBuf),
    #[error("source name cannot be empty")]
    InvalidIdentifier,
    #[error("invalid source options: {0}")]
    InvalidOptions(&'static str),
    #[error("invalid column type override: {0}")]
    InvalidDataType(String),
    #[error("replacement schema differs; expected {expected:?}, found {actual:?}")]
    IncompatibleSchema {
        expected: Vec<String>,
        actual: Vec<String>,
    },
    #[error(transparent)]
    DuckDb(#[from] duckdb::Error),
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn fixture_path(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tarik-source-{stamp}-{name}"))
    }

    #[test]
    fn inspects_csv_with_bounded_preview() {
        let path = fixture_path("orders.csv");
        let body = "id,customer,amount\n".to_owned()
            + &(1..=40)
                .map(|index| format!("{index},Customer {index},{}", index * 10))
                .collect::<Vec<_>>()
                .join("\n");
        std::fs::write(&path, body).unwrap();

        let inspection = inspect(&path, None).unwrap();

        assert_eq!(inspection.format, SourceFormat::Csv);
        assert_eq!(inspection.columns.len(), 3);
        assert_eq!(inspection.row_count, 40);
        assert!(inspection.row_count_exact);
        assert!(inspection.file_size_bytes > 0);
        assert_eq!(inspection.preview_rows.len(), 25);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn inspects_parquet_schema_and_preview() {
        let path = fixture_path("orders.parquet");
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(&format!(
                "COPY (SELECT 1::BIGINT id, 'Ari'::VARCHAR customer) TO {} (FORMAT PARQUET)",
                sql_literal(path.to_str().unwrap())
            ))
            .unwrap();

        let inspection = inspect(&path, None).unwrap();

        assert_eq!(inspection.format, SourceFormat::Parquet);
        assert_eq!(inspection.columns[0].name, "id");
        assert_eq!(inspection.row_count, 1);
        assert!(inspection.row_count_exact);
        assert_eq!(inspection.preview_rows.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn estimates_large_csv_without_full_count_scan() {
        let path = fixture_path("large.csv");
        let mut file = std::fs::File::create(&path).unwrap();
        use std::io::Write;
        file.write_all(b"id,value\n").unwrap();
        let row = b"1,abcdefghijklmnopqrstuvwxyz0123456789\n";
        while file.metadata().unwrap().len() <= EXACT_CSV_COUNT_MAX_BYTES + 1024 {
            file.write_all(row).unwrap();
        }
        drop(file);

        let inspection = inspect(&path, None).unwrap();

        assert!(!inspection.row_count_exact);
        assert!(inspection.row_count > 100_000);
        assert_eq!(inspection.preview_rows.len(), 25);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_missing_unsupported_and_invalid_delimiter() {
        let missing = fixture_path("missing.csv");
        assert!(matches!(
            inspect(&missing, None),
            Err(SourceError::Missing(_))
        ));
        let unsupported = fixture_path("orders.xlsx");
        std::fs::write(&unsupported, "data").unwrap();
        assert!(matches!(
            inspect(&unsupported, None),
            Err(SourceError::Unsupported(_))
        ));
        let csv = fixture_path("orders.csv");
        std::fs::write(&csv, "a||b\n1||2").unwrap();
        let options = CsvOptions {
            delimiter: "||".into(),
            ..Default::default()
        };
        assert!(matches!(
            inspect(&csv, Some(&options)),
            Err(SourceError::InvalidOptions(_))
        ));
        let _ = std::fs::remove_file(unsupported);
        let _ = std::fs::remove_file(csv);
    }

    #[test]
    fn quotes_paths_and_identifiers() {
        assert_eq!(
            sql_literal("C:\\Data\\O'Brien.csv"),
            "'C:\\Data\\O''Brien.csv'"
        );
        assert_eq!(quote_identifier("order lines").unwrap(), "\"order lines\"");
        assert_eq!(quote_identifier("odd\"name").unwrap(), "\"odd\"\"name\"");
        assert_eq!(
            suggested_name(Path::new("2026 Sales-data.csv")),
            "data_2026_sales_data"
        );
    }
}

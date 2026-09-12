//! Bounded Arrow IPC page artifacts.
//!
//! The job worker streams DuckDB record batches through [`PageWriter`], which
//! splits them into page files of at most `page_rows` rows and roughly
//! `MAX_PAGE_BYTES` bytes. [`read_page`] reads one page artifact back and
//! converts the requested window into JSON-safe values. No stage ever holds
//! the full result: batches arrive one at a time and pages live on disk.

use std::{
    fs,
    path::{Path, PathBuf},
};

use arrow_ipc::reader::FileReader;
use arrow_ipc::writer::FileWriter;
use duckdb::arrow::array::{
    Array, BooleanArray, Float32Array, Float64Array, Int16Array, Int32Array, Int64Array, Int8Array,
    LargeStringArray, StringArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use duckdb::arrow::datatypes::{DataType, SchemaRef};
use duckdb::arrow::record_batch::RecordBatch;
use serde_json::{Map, Number, Value};

use crate::error::EngineError;

/// Rows per page unless the byte target forces fewer.
pub const DEFAULT_PAGE_ROWS: u32 = 500;
/// Soft target for the size of one page artifact.
pub const MAX_PAGE_BYTES: usize = 4 * 1024 * 1024;
/// Largest string cell transferred to the WebView before truncation.
pub const MAX_CELL_BYTES: usize = 64 * 1024;

/// Rows are safe as JSON numbers only up to JavaScript's integer precision.
const JS_SAFE_MAX: i64 = 9_007_199_254_740_991; // 2^53 - 1

pub struct PageWriter {
    dir: PathBuf,
    schema: Option<SchemaRef>,
    page_rows: u32,
    buffered: Vec<RecordBatch>,
    buffered_rows: usize,
    page_index: usize,
    total_rows: u64,
    bytes_written: u64,
    maximum_bytes: Option<u64>,
    bytes_per_row: Option<usize>,
}

impl PageWriter {
    pub fn create_bounded(dir: PathBuf, maximum_bytes: Option<u64>) -> Result<Self, EngineError> {
        fs::create_dir_all(&dir).map_err(|error| EngineError::CacheIo {
            path: dir.clone(),
            source: error.into(),
        })?;
        Ok(Self {
            dir,
            schema: None,
            page_rows: DEFAULT_PAGE_ROWS,
            buffered: Vec::new(),
            buffered_rows: 0,
            page_index: 0,
            total_rows: 0,
            bytes_written: 0,
            maximum_bytes,
            bytes_per_row: None,
        })
    }

    pub fn page_rows(&self) -> u32 {
        self.page_rows
    }

    pub fn total_rows(&self) -> u64 {
        self.total_rows
    }

    pub fn bytes_written(&self) -> u64 {
        self.bytes_written
    }

    /// Accept one streamed batch, splitting it across page boundaries.
    pub fn write(&mut self, batch: &RecordBatch) -> Result<(), EngineError> {
        if batch.num_rows() == 0 {
            return Ok(());
        }
        if self.schema.is_none() {
            self.schema = Some(batch.schema());
            self.bytes_per_row =
                Some((batch.get_array_memory_size() / batch.num_rows().max(1)).max(1));
            let by_bytes = (MAX_PAGE_BYTES / self.bytes_per_row.unwrap()).max(1) as u32;
            self.page_rows = self.page_rows.min(by_bytes).max(1);
        }
        self.total_rows += batch.num_rows() as u64;
        self.append(batch)
    }

    fn append(&mut self, batch: &RecordBatch) -> Result<(), EngineError> {
        let mut offset = 0usize;
        let remaining_rows = batch.num_rows();
        while offset < remaining_rows {
            let capacity = self.page_rows as usize - self.buffered_rows;
            if capacity == 0 {
                self.flush()?;
                continue;
            }
            let take = capacity.min(remaining_rows - offset);
            self.buffered.push(batch.slice(offset, take));
            self.buffered_rows += take;
            offset += take;
            if self.buffered_rows >= self.page_rows as usize {
                self.flush()?;
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), EngineError> {
        if self.buffered.is_empty() {
            return Ok(());
        }
        let schema = self
            .schema
            .clone()
            .expect("schema exists once batches were written");
        let path = self.page_path(self.page_index);
        let file = fs::File::create(&path).map_err(|error| EngineError::CacheIo {
            path: path.clone(),
            source: error.into(),
        })?;
        let mut writer =
            FileWriter::try_new(file, &schema).map_err(|error| EngineError::CacheIo {
                path: path.clone(),
                source: error.into(),
            })?;
        for batch in self.buffered.drain(..) {
            writer.write(&batch).map_err(|error| EngineError::CacheIo {
                path: path.clone(),
                source: error.into(),
            })?;
        }
        writer.finish().map_err(|error| EngineError::CacheIo {
            path: path.clone(),
            source: error.into(),
        })?;
        let page_bytes = fs::metadata(&path)
            .map_err(|error| EngineError::CacheIo {
                path: path.clone(),
                source: error.into(),
            })?
            .len();
        self.bytes_written = self.bytes_written.saturating_add(page_bytes);
        if self
            .maximum_bytes
            .is_some_and(|maximum| self.bytes_written > maximum)
        {
            return Err(EngineError::ResultQuotaExceeded);
        }
        self.page_index += 1;
        self.buffered_rows = 0;
        Ok(())
    }

    /// Write the final (possibly partial) page. Empty results produce no file.
    pub fn finish(&mut self) -> Result<(), EngineError> {
        self.flush()
    }

    /// Atomically publish the finished page directory.
    pub fn publish(mut self, final_dir: &Path) -> Result<(), EngineError> {
        self.finish()?;
        if final_dir.exists() {
            let _ = fs::remove_dir_all(final_dir);
        }
        fs::rename(&self.dir, final_dir).map_err(|error| EngineError::CacheIo {
            path: final_dir.to_path_buf(),
            source: error.into(),
        })
    }

    fn page_path(&self, index: usize) -> PathBuf {
        self.dir.join(format!("page-{index:06}.arrow"))
    }
}

/// Discard a temporary directory; missing directories are fine.
pub fn discard_dir(dir: &Path) {
    let _ = fs::remove_dir_all(dir);
}

/// Read the page file covering `offset` and convert up to `max_rows` rows of
/// the window into JSON-safe cells.
pub struct PageRead {
    pub values: Vec<Vec<Value>>,
    pub truncated_cells: Vec<(u32, u32)>,
}

pub fn read_page_window(
    page_dir: &Path,
    page_rows: u32,
    offset: u64,
    max_rows: u32,
    total_rows: u64,
) -> Result<PageRead, EngineError> {
    let mut values = Vec::new();
    let mut truncated_cells = Vec::new();
    if total_rows == 0 || max_rows == 0 {
        return Ok(PageRead {
            values,
            truncated_cells,
        });
    }
    let end = (offset + u64::from(max_rows)).min(total_rows);
    let mut page_index = (offset / u64::from(page_rows)) as usize;
    let mut window_cursor = offset;
    while window_cursor < end {
        let path = page_dir.join(format!("page-{page_index:06}.arrow"));
        let file = fs::File::open(&path).map_err(|error| EngineError::CacheIo {
            path: path.clone(),
            source: error.into(),
        })?;
        let reader = FileReader::try_new(file, None).map_err(|error| EngineError::CacheIo {
            path: path.clone(),
            source: error.into(),
        })?;
        let page_start = u64::from(page_rows) * page_index as u64;
        let mut page_rows_seen = 0u64;
        for batch in reader {
            let batch = batch.map_err(|error| EngineError::CacheIo {
                path: path.clone(),
                source: error.into(),
            })?;
            let batch_start = page_start + page_rows_seen;
            let batch_end = batch_start + batch.num_rows() as u64;
            page_rows_seen += batch.num_rows() as u64;

            let from = window_cursor.max(batch_start);
            let to = end.min(batch_end);
            if from >= to {
                continue;
            }
            let slice = batch.slice((from - batch_start) as usize, (to - from) as usize);
            let (mut rows, mut truncated) = convert_batch(&slice, MAX_CELL_BYTES)?;
            values.append(&mut rows);
            truncated_cells.append(&mut truncated);
            window_cursor = to;
        }
        page_index += 1;
    }
    Ok(PageRead {
        values,
        truncated_cells,
    })
}

/// One converted batch: JSON-safe cells plus the (row, column) indices of
/// truncated cells.
pub type ConvertedBatch = (Vec<Vec<Value>>, Vec<(u32, u32)>);

fn convert_batch(
    batch: &RecordBatch,
    max_cell_bytes: usize,
) -> Result<ConvertedBatch, EngineError> {
    let mut rows = Vec::with_capacity(batch.num_rows());
    let mut truncated_cells = Vec::new();
    let columns: Vec<ColumnValues> = batch
        .columns()
        .iter()
        .map(|column| ColumnValues::new(column.as_ref(), max_cell_bytes))
        .collect::<Result<Vec<_>, EngineError>>()?;
    for row in 0..batch.num_rows() {
        let mut cells = Vec::with_capacity(columns.len());
        for (col_index, column) in columns.iter().enumerate() {
            let (value, truncated) = column.value(row)?;
            if truncated {
                truncated_cells.push((row as u32, col_index as u32));
            }
            cells.push(value);
        }
        rows.push(cells);
    }
    Ok((rows, truncated_cells))
}

enum ColumnValues {
    Boolean(BooleanArray),
    Int8(Int8Array),
    Int16(Int16Array),
    Int32(Int32Array),
    Int64(Int64Array),
    UInt8(UInt8Array),
    UInt16(UInt16Array),
    UInt32(UInt32Array),
    UInt64(UInt64Array),
    Float32(Float32Array),
    Float64(Float64Array),
    Utf8 {
        values: StringArray,
        max_bytes: usize,
    },
    LargeUtf8 {
        values: LargeStringArray,
        max_bytes: usize,
    },
    Display(duckdb::arrow::array::ArrayRef),
}

impl ColumnValues {
    fn new(array: &dyn Array, max_bytes: usize) -> Result<Self, EngineError> {
        Ok(match array.data_type() {
            DataType::Boolean => Self::Boolean(
                array
                    .as_any()
                    .downcast_ref::<BooleanArray>()
                    .expect("boolean array")
                    .clone(),
            ),
            DataType::Int8 => Self::Int8(
                array
                    .as_any()
                    .downcast_ref::<Int8Array>()
                    .expect("int8 array")
                    .clone(),
            ),
            DataType::Int16 => Self::Int16(
                array
                    .as_any()
                    .downcast_ref::<Int16Array>()
                    .expect("int16 array")
                    .clone(),
            ),
            DataType::Int32 => Self::Int32(
                array
                    .as_any()
                    .downcast_ref::<Int32Array>()
                    .expect("int32 array")
                    .clone(),
            ),
            DataType::Int64 => Self::Int64(
                array
                    .as_any()
                    .downcast_ref::<Int64Array>()
                    .expect("int64 array")
                    .clone(),
            ),
            DataType::UInt8 => Self::UInt8(
                array
                    .as_any()
                    .downcast_ref::<UInt8Array>()
                    .expect("uint8 array")
                    .clone(),
            ),
            DataType::UInt16 => Self::UInt16(
                array
                    .as_any()
                    .downcast_ref::<UInt16Array>()
                    .expect("uint16 array")
                    .clone(),
            ),
            DataType::UInt32 => Self::UInt32(
                array
                    .as_any()
                    .downcast_ref::<UInt32Array>()
                    .expect("uint32 array")
                    .clone(),
            ),
            DataType::UInt64 => Self::UInt64(
                array
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .expect("uint64 array")
                    .clone(),
            ),
            DataType::Float32 => Self::Float32(
                array
                    .as_any()
                    .downcast_ref::<Float32Array>()
                    .expect("float32 array")
                    .clone(),
            ),
            DataType::Float64 => Self::Float64(
                array
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .expect("float64 array")
                    .clone(),
            ),
            DataType::Utf8 => Self::Utf8 {
                values: array
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("utf8 array")
                    .clone(),
                max_bytes,
            },
            DataType::LargeUtf8 => Self::LargeUtf8 {
                values: array
                    .as_any()
                    .downcast_ref::<LargeStringArray>()
                    .expect("large utf8 array")
                    .clone(),
                max_bytes,
            },
            _ => Self::Display(array.slice(0, array.len())),
        })
    }

    /// Returns the JSON value plus whether the cell was truncated.
    fn value(&self, index: usize) -> Result<(Value, bool), EngineError> {
        match self {
            Self::Boolean(values) => Ok((bool_json(values, index), false)),
            Self::Int8(values) => Ok((
                int_json(i64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::Int16(values) => Ok((
                int_json(i64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::Int32(values) => Ok((
                int_json(i64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::Int64(values) => {
                Ok((int_json(values.value(index), values.is_null(index)), false))
            }
            Self::UInt8(values) => Ok((
                uint_json(u64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::UInt16(values) => Ok((
                uint_json(u64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::UInt32(values) => Ok((
                uint_json(u64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::UInt64(values) => {
                Ok((uint_json(values.value(index), values.is_null(index)), false))
            }
            Self::Float32(values) => Ok((
                float_json(f64::from(values.value(index)), values.is_null(index)),
                false,
            )),
            Self::Float64(values) => Ok((
                float_json(values.value(index), values.is_null(index)),
                false,
            )),
            Self::Utf8 { values, max_bytes } => Ok(string_json(values, index, *max_bytes)),
            Self::LargeUtf8 { values, max_bytes } => {
                Ok(string_json_large(values, index, *max_bytes))
            }
            Self::Display(array) => {
                if array.is_null(index) {
                    return Ok((Value::Null, false));
                }
                let text =
                    duckdb::arrow::util::display::array_value_to_string(array.as_ref(), index)
                        .map_err(|error| EngineError::CacheDisplay(error.to_string()))?;
                Ok((Value::String(text), false))
            }
        }
    }
}

fn bool_json(values: &BooleanArray, index: usize) -> Value {
    if values.is_null(index) {
        Value::Null
    } else {
        Value::Bool(values.value(index))
    }
}

fn int_json(value: i64, is_null: bool) -> Value {
    if is_null {
        return Value::Null;
    }
    if value.abs() > JS_SAFE_MAX {
        Value::String(value.to_string())
    } else {
        Value::Number(Number::from(value))
    }
}

fn uint_json(value: u64, is_null: bool) -> Value {
    if is_null {
        return Value::Null;
    }
    if value > JS_SAFE_MAX as u64 {
        Value::String(value.to_string())
    } else {
        Value::Number(Number::from(value))
    }
}

fn float_json(value: f64, is_null: bool) -> Value {
    if is_null {
        return Value::Null;
    }
    Number::from_f64(value).map_or_else(|| Value::String(format_float(value)), Value::Number)
}

fn format_float(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value > 0.0 {
        "Infinity".into()
    } else {
        "-Infinity".into()
    }
}

fn string_json(values: &StringArray, index: usize, max_bytes: usize) -> (Value, bool) {
    if values.is_null(index) {
        return (Value::Null, false);
    }
    let text = values.value(index);
    truncate_string(text, max_bytes)
}

fn string_json_large(values: &LargeStringArray, index: usize, max_bytes: usize) -> (Value, bool) {
    if values.is_null(index) {
        return (Value::Null, false);
    }
    let text = values.value(index);
    truncate_string(text, max_bytes)
}

fn truncate_string(text: &str, max_bytes: usize) -> (Value, bool) {
    if text.len() <= max_bytes {
        return (Value::String(text.to_string()), false);
    }
    let mut cut = max_bytes;
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    (Value::String(text[..cut].to_string()), true)
}

/// Generic logical type used by the frontend for formatting.
pub fn logical_type(data_type: &DataType) -> &'static str {
    match data_type {
        DataType::Boolean => "boolean",
        DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64 => "integer",
        DataType::Float16 | DataType::Float32 | DataType::Float64 => "float",
        DataType::Decimal32(_, _)
        | DataType::Decimal64(_, _)
        | DataType::Decimal128(_, _)
        | DataType::Decimal256(_, _) => "decimal",
        DataType::Date32 | DataType::Date64 => "date",
        DataType::Time32(_) | DataType::Time64(_) => "time",
        DataType::Timestamp(_, _) => "timestamp",
        DataType::Duration(_) => "duration",
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "string",
        DataType::Binary
        | DataType::FixedSizeBinary(_)
        | DataType::LargeBinary
        | DataType::BinaryView => "binary",
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => "list",
        DataType::Struct(_) => "struct",
        DataType::Map(_, _) => "map",
        DataType::Null => "null",
        DataType::Dictionary(_, value) => logical_type(value),
        _ => "unknown",
    }
}

/// Column metadata from the result schema.
pub fn column_metadata(schema: &SchemaRef) -> Vec<Map<String, Value>> {
    schema
        .fields()
        .iter()
        .map(|field| {
            let mut map = Map::new();
            map.insert("name".into(), Value::String(field.name().clone()));
            map.insert(
                "logicalType".into(),
                Value::String(logical_type(field.data_type()).to_string()),
            );
            map.insert(
                "nativeType".into(),
                Value::String(field.data_type().to_string()),
            );
            map.insert("nullable".into(), Value::Bool(field.is_nullable()));
            map
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_beyond_the_cell_limit_are_truncated_with_char_boundaries() {
        let (value, truncated) = truncate_string(&"x".repeat(MAX_CELL_BYTES), MAX_CELL_BYTES);
        assert_eq!(value.as_str().unwrap().len(), MAX_CELL_BYTES);
        assert!(!truncated);

        let multibyte = "\u{e4}".repeat(MAX_CELL_BYTES); // 2 bytes per char
        let (value, truncated) = truncate_string(&multibyte, MAX_CELL_BYTES);
        assert!(truncated);
        let text = value.as_str().unwrap();
        assert!(text.len() <= MAX_CELL_BYTES);
        // The cut must still be valid UTF-8.
        assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    }

    #[test]
    fn json_values_respect_javascript_safe_range() {
        assert_eq!(int_json(4_000, false), Value::Number(Number::from(4_000)));
        assert_eq!(
            int_json(9_007_199_254_740_992, false),
            Value::String("9007199254740992".to_string())
        );
        assert_eq!(int_json(5, true), Value::Null);
        assert_eq!(
            uint_json(JS_SAFE_MAX as u64 + 1, false),
            Value::String("9007199254740992".to_string())
        );
        assert_eq!(float_json(f64::NAN, false), Value::String("NaN".into()));
        assert_eq!(
            float_json(1.5, false),
            Value::Number(Number::from_f64(1.5).unwrap())
        );
    }

    #[test]
    fn logical_types_cover_common_columns() {
        assert_eq!(logical_type(&DataType::Int64), "integer");
        assert_eq!(logical_type(&DataType::Utf8), "string");
        assert_eq!(logical_type(&DataType::Decimal128(18, 2)), "decimal");
        assert_eq!(
            logical_type(&DataType::Timestamp(
                duckdb::arrow::datatypes::TimeUnit::Microsecond,
                None
            )),
            "timestamp"
        );
        assert_eq!(
            logical_type(&DataType::List(
                duckdb::arrow::datatypes::Field::new("x", DataType::Int64, true).into()
            )),
            "list"
        );
    }
}

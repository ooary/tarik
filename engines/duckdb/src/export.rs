//! One-pass exact-row CSV and Parquet export writers.
//!
//! DuckDB record batches are sliced at part boundaries and written directly to
//! hidden staging files. A part is visible only after its writer closes and the
//! completed stage is renamed into place. This module owns no async lifecycle;
//! `ExportRegistry` (E9-T3) will provide queuing, progress, and cancellation.

use std::{collections::VecDeque, fs, path::PathBuf, sync::Arc};

use arrow_csv::WriterBuilder as CsvWriterBuilder;
use duckdb::{arrow::datatypes::SchemaRef, arrow::record_batch::RecordBatch, Connection};
use parquet::{arrow::ArrowWriter, basic::Compression, file::properties::WriterProperties};
use tarik_engine_protocol::{
    ExportFormat, ExportOptions, ExportOverwritePolicy, ExportPartSummary, ParquetCompression,
    ValidatedExportOptions, MAX_REPORTED_EXPORT_PARTS,
};

use crate::{error::EngineError, sql::split_statements};

/// Completed output from one SQL execution. Empty results produce no parts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOutcome {
    pub rows_written: u64,
    pub files_written: u64,
    pub bytes_written: u64,
    /// Bounded newest part summaries; aggregate counters remain exact.
    pub completed_parts: Vec<ExportPartSummary>,
}

pub trait ExportObserver {
    /// Called before every batch slice. A cancellation observer returns
    /// `ExportCancelled` here; dropping the writer removes any current stage.
    fn check_cancelled(&self, _current_part: u64) -> Result<(), EngineError> {
        Ok(())
    }

    fn rows_written(&self, _rows: u64, _current_part: u64) {}

    fn part_completed(&self, _part: &ExportPartSummary) {}
}

struct NoopObserver;
impl ExportObserver for NoopObserver {}

/// Validate synchronously, then execute the SQL exactly once and stream its
/// final row-returning statement into exact-row files.
pub fn execute_export(
    connection: &Connection,
    sql: &str,
    options: ExportOptions,
) -> Result<ExportOutcome, EngineError> {
    if sql.trim().is_empty() || split_statements(sql).is_empty() {
        return Err(EngineError::InvalidQuery("sql text contains no statements"));
    }
    let options = options
        .validate()
        .map_err(|error| EngineError::ExportInvalid(error.to_string()))?;
    execute_validated_export(connection, sql, &options, &NoopObserver)
}

pub fn execute_validated_export(
    connection: &Connection,
    sql: &str,
    options: &ValidatedExportOptions,
    observer: &dyn ExportObserver,
) -> Result<ExportOutcome, EngineError> {
    let statements = split_statements(sql);
    let (last, prior) = statements
        .split_last()
        .ok_or(EngineError::InvalidQuery("sql text contains no statements"))?;

    // Earlier statements are intentionally executed once in order. Their row
    // sets are drained without buffering. Only the final statement is exported,
    // matching Tarik's query-result semantics.
    for statement_sql in prior {
        observer.check_cancelled(1)?;
        let mut statement = connection.prepare(statement_sql)?;
        let batches = statement.stream_arrow([])?;
        for _ in batches {
            observer.check_cancelled(1)?;
        }
    }

    let mut statement = connection.prepare(last)?;
    let mut batches = statement.stream_arrow([])?;
    let schema = batches.get_schema();
    let mut writer = ChunkedExportWriter::new(options, schema, observer);
    for batch in batches.by_ref() {
        writer.write(&batch)?
    }
    observer.check_cancelled(writer.next_part)?;
    writer.finish()
}

struct ChunkedExportWriter<'a> {
    options: &'a ValidatedExportOptions,
    schema: SchemaRef,
    current: Option<PartWriter>,
    current_rows: u64,
    next_part: u64,
    rows_written: u64,
    files_written: u64,
    bytes_written: u64,
    completed_parts: VecDeque<ExportPartSummary>,
    observer: &'a dyn ExportObserver,
}

impl<'a> ChunkedExportWriter<'a> {
    fn new(
        options: &'a ValidatedExportOptions,
        schema: SchemaRef,
        observer: &'a dyn ExportObserver,
    ) -> Self {
        Self {
            options,
            schema,
            current: None,
            current_rows: 0,
            next_part: 1,
            rows_written: 0,
            files_written: 0,
            bytes_written: 0,
            completed_parts: VecDeque::new(),
            observer,
        }
    }

    fn write(&mut self, batch: &RecordBatch) -> Result<(), EngineError> {
        let mut offset = 0usize;
        while offset < batch.num_rows() {
            self.observer.check_cancelled(self.next_part)?;
            if self.current.is_none() {
                self.current = Some(PartWriter::create(
                    self.next_part,
                    self.options,
                    Arc::clone(&self.schema),
                )?);
            }
            let capacity = self.options.rows_per_part - self.current_rows;
            let take = usize::try_from(capacity.min((batch.num_rows() - offset) as u64)).map_err(
                |_| EngineError::ExportInvalid("part size exceeds platform range".into()),
            )?;
            let slice = batch.slice(offset, take);
            self.current
                .as_mut()
                .expect("part exists before writing")
                .write(&slice)?;
            self.current_rows += take as u64;
            self.rows_written += take as u64;
            self.observer.rows_written(take as u64, self.next_part);
            offset += take;

            if self.current_rows == self.options.rows_per_part {
                self.complete_current()?;
            }
        }
        Ok(())
    }

    fn complete_current(&mut self) -> Result<(), EngineError> {
        let Some(current) = self.current.take() else {
            return Ok(());
        };
        let part_number = self.next_part;
        let rows = self.current_rows;
        let (path, bytes) = current.publish(self.options.overwrite)?;
        let summary = ExportPartSummary {
            part_number,
            path: path.to_string_lossy().into_owned(),
            rows,
            bytes,
        };
        self.files_written += 1;
        self.bytes_written += bytes;
        self.observer.part_completed(&summary);
        self.completed_parts.push_back(summary);
        if self.completed_parts.len() > MAX_REPORTED_EXPORT_PARTS {
            self.completed_parts.pop_front();
        }
        self.next_part = self
            .next_part
            .checked_add(1)
            .ok_or_else(|| EngineError::ExportInvalid("part number overflow".into()))?;
        self.current_rows = 0;
        Ok(())
    }

    fn finish(mut self) -> Result<ExportOutcome, EngineError> {
        if self.current_rows > 0 {
            self.complete_current()?;
        }
        Ok(ExportOutcome {
            rows_written: self.rows_written,
            files_written: self.files_written,
            bytes_written: self.bytes_written,
            completed_parts: self.completed_parts.into_iter().collect(),
        })
    }
}

struct PartWriter {
    stage_path: PathBuf,
    final_path: PathBuf,
    writer: Option<Writer>,
}

impl PartWriter {
    fn create(
        part_number: u64,
        options: &ValidatedExportOptions,
        schema: SchemaRef,
    ) -> Result<Self, EngineError> {
        let final_path = options
            .part_path(part_number)
            .map_err(|error| EngineError::ExportInvalid(error.to_string()))?;
        if options.overwrite == ExportOverwritePolicy::FailIfExists && final_path.exists() {
            return Err(EngineError::ExportCollision(final_path));
        }
        let stage_name = format!(".tarik-export-{}.tmp", uuid::Uuid::new_v4());
        let stage_path = options.output_directory.join(stage_name);
        let file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage_path)
            .map_err(|source| EngineError::ExportIo {
                path: stage_path.clone(),
                source,
            })?;
        let writer = match options.format {
            ExportFormat::Csv => {
                let csv = options.csv.as_ref().expect("validated CSV options");
                Writer::Csv(
                    CsvWriterBuilder::new()
                        .with_delimiter(csv.delimiter.as_bytes()[0])
                        .with_header(csv.include_header)
                        .build(file),
                )
            }
            ExportFormat::Parquet => {
                let parquet = options.parquet.as_ref().expect("validated Parquet options");
                let compression = match parquet.compression {
                    ParquetCompression::Uncompressed => Compression::UNCOMPRESSED,
                    ParquetCompression::Snappy => Compression::SNAPPY,
                    ParquetCompression::Gzip => Compression::GZIP(Default::default()),
                    ParquetCompression::Zstd => Compression::ZSTD(Default::default()),
                };
                let properties = WriterProperties::builder()
                    .set_compression(compression)
                    .set_max_row_group_bytes(Some(4 * 1024 * 1024))
                    .build();
                let writer =
                    ArrowWriter::try_new(file, schema, Some(properties)).map_err(|error| {
                        let _ = fs::remove_file(&stage_path);
                        EngineError::ExportWrite {
                            path: stage_path.clone(),
                            message: error.to_string(),
                        }
                    })?;
                Writer::Parquet(writer)
            }
        };
        Ok(Self {
            stage_path,
            final_path,
            writer: Some(writer),
        })
    }

    fn write(&mut self, batch: &RecordBatch) -> Result<(), EngineError> {
        let path = self.stage_path.clone();
        match self.writer.as_mut().expect("part writer is open") {
            Writer::Csv(writer) => writer
                .write(batch)
                .map_err(|error| EngineError::ExportWrite {
                    path,
                    message: error.to_string(),
                }),
            Writer::Parquet(writer) => {
                writer
                    .write(batch)
                    .map_err(|error| EngineError::ExportWrite {
                        path,
                        message: error.to_string(),
                    })
            }
        }
    }

    fn publish(mut self, overwrite: ExportOverwritePolicy) -> Result<(PathBuf, u64), EngineError> {
        let writer = self.writer.take().expect("part writer is open");
        writer.close(&self.stage_path)?;
        let backup = if overwrite == ExportOverwritePolicy::Replace && self.final_path.exists() {
            let backup = self
                .final_path
                .with_file_name(format!(".tarik-export-backup-{}", uuid::Uuid::new_v4()));
            fs::rename(&self.final_path, &backup).map_err(|source| EngineError::ExportIo {
                path: self.final_path.clone(),
                source,
            })?;
            Some(backup)
        } else {
            None
        };
        if let Err(source) = fs::rename(&self.stage_path, &self.final_path) {
            if let Some(backup) = backup.as_ref() {
                let _ = fs::rename(backup, &self.final_path);
            }
            return Err(EngineError::ExportIo {
                path: self.final_path.clone(),
                source,
            });
        }
        if let Some(backup) = backup {
            // Publication already succeeded. Backup cleanup is best effort so
            // a cleanup-only failure cannot misreport the new completed part.
            let _ = fs::remove_file(backup);
        }
        let bytes = fs::metadata(&self.final_path)
            .map_err(|source| EngineError::ExportIo {
                path: self.final_path.clone(),
                source,
            })?
            .len();
        Ok((self.final_path.clone(), bytes))
    }
}

impl Drop for PartWriter {
    fn drop(&mut self) {
        // A published part no longer has a stage path. On any write/close/publish
        // error this best-effort cleanup removes only the incomplete stage.
        let _ = fs::remove_file(&self.stage_path);
    }
}

enum Writer {
    Csv(arrow_csv::Writer<fs::File>),
    Parquet(ArrowWriter<fs::File>),
}

impl Writer {
    fn close(self, path: &std::path::Path) -> Result<(), EngineError> {
        match self {
            Self::Csv(writer) => {
                let file = writer.into_inner();
                file.sync_all().map_err(|source| EngineError::ExportIo {
                    path: path.to_path_buf(),
                    source,
                })
            }
            Self::Parquet(writer) => {
                writer
                    .close()
                    .map(|_| ())
                    .map_err(|error| EngineError::ExportWrite {
                        path: path.to_path_buf(),
                        message: error.to_string(),
                    })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::Read,
        sync::atomic::{AtomicU64, Ordering},
    };

    use arrow_csv::ReaderBuilder as CsvReaderBuilder;
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use tarik_engine_protocol::{CsvExportOptions, ExportOverwritePolicy, ParquetExportOptions};

    use super::*;

    fn output_dir(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("tarik-export-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }

    fn csv_options(directory: &std::path::Path, rows_per_part: u64) -> ExportOptions {
        ExportOptions {
            format: ExportFormat::Csv,
            output_directory: directory.to_string_lossy().into_owned(),
            base_name: "orders".into(),
            rows_per_part,
            overwrite: ExportOverwritePolicy::FailIfExists,
            csv: Some(CsvExportOptions::default()),
            parquet: None,
        }
    }

    fn parquet_options(directory: &std::path::Path, rows_per_part: u64) -> ExportOptions {
        ExportOptions {
            format: ExportFormat::Parquet,
            output_directory: directory.to_string_lossy().into_owned(),
            base_name: "orders".into(),
            rows_per_part,
            overwrite: ExportOverwritePolicy::FailIfExists,
            csv: None,
            parquet: Some(ParquetExportOptions::default()),
        }
    }

    fn connection() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    #[test]
    fn csv_parts_have_exact_rows_and_a_header_in_every_part() {
        let directory = output_dir("csv-exact");
        let outcome = execute_export(
            &connection(),
            "SELECT i, 'row-' || i AS label FROM range(1, 9) t(i)",
            csv_options(&directory, 3),
        )
        .unwrap();

        assert_eq!(outcome.rows_written, 8);
        assert_eq!(outcome.completed_parts.len(), 3);
        assert_eq!(
            outcome
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![3, 3, 2]
        );
        let mut values = Vec::new();
        for part in &outcome.completed_parts {
            let text = fs::read_to_string(&part.path).unwrap();
            assert!(text.starts_with("i,label\n"), "missing header in {text:?}");
            values.extend(
                text.lines()
                    .skip(1)
                    .map(|line| line.split(',').next().unwrap().parse::<u64>().unwrap()),
            );
        }
        assert_eq!(values, (1..=8).collect::<Vec<_>>());
        assert!(!fs::read_dir(&directory)
            .unwrap()
            .flatten()
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with(".tarik-export-")));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn one_large_batch_splits_and_zero_rows_create_no_files() {
        let directory = output_dir("boundaries");
        let outcome = execute_export(
            &connection(),
            "SELECT i FROM range(0, 10) t(i)",
            csv_options(&directory, 4),
        )
        .unwrap();
        assert_eq!(
            outcome
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![4, 4, 2]
        );

        let empty_dir = output_dir("zero");
        let empty = execute_export(
            &connection(),
            "SELECT i FROM range(0) t(i)",
            csv_options(&empty_dir, 4),
        )
        .unwrap();
        assert_eq!(empty.rows_written, 0);
        assert!(empty.completed_parts.is_empty());
        assert_eq!(fs::read_dir(&empty_dir).unwrap().count(), 0);
        fs::remove_dir_all(directory).unwrap();
        fs::remove_dir_all(empty_dir).unwrap();
    }

    #[test]
    fn exact_boundary_does_not_create_an_empty_trailing_part() {
        let directory = output_dir("exact-boundary");
        let outcome = execute_export(
            &connection(),
            "SELECT i FROM range(0, 6) t(i)",
            csv_options(&directory, 3),
        )
        .unwrap();
        assert_eq!(
            outcome
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![3, 3]
        );
        assert!(!directory.join("orders-part-00003.csv").exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn many_input_batches_keep_exact_part_boundaries() {
        let directory = output_dir("many-batches");
        let outcome = execute_export(
            &connection(),
            "SELECT i FROM range(0, 10001) t(i)",
            csv_options(&directory, 4_000),
        )
        .unwrap();
        assert_eq!(outcome.rows_written, 10_001);
        assert_eq!(
            outcome
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![4_000, 4_000, 2_001]
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn parquet_parts_round_trip_with_exact_rows_and_order() {
        let directory = output_dir("parquet");
        let outcome = execute_export(
            &connection(),
            "SELECT i, i * 10 AS amount FROM range(1, 8) t(i)",
            parquet_options(&directory, 3),
        )
        .unwrap();
        assert_eq!(
            outcome
                .completed_parts
                .iter()
                .map(|part| part.rows)
                .collect::<Vec<_>>(),
            vec![3, 3, 1]
        );

        let mut rows = 0usize;
        for part in &outcome.completed_parts {
            let file = fs::File::open(&part.path).unwrap();
            let reader = ParquetRecordBatchReaderBuilder::try_new(file)
                .unwrap()
                .build()
                .unwrap();
            rows += reader.map(|batch| batch.unwrap().num_rows()).sum::<usize>();
        }
        assert_eq!(rows, 7);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn collision_fails_before_stage_creation_and_replace_publishes_complete_file() {
        let directory = output_dir("collision");
        let target = directory.join("orders-part-00001.csv");
        fs::write(&target, "existing\n").unwrap();
        let error = execute_export(&connection(), "SELECT 1 AS i", csv_options(&directory, 10))
            .unwrap_err();
        assert!(matches!(error, EngineError::ExportCollision(path) if path == target));
        assert_eq!(fs::read_to_string(&target).unwrap(), "existing\n");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);

        let mut replace = csv_options(&directory, 10);
        replace.overwrite = ExportOverwritePolicy::Replace;
        execute_export(&connection(), "SELECT 42 AS i", replace).unwrap();
        let mut text = String::new();
        fs::File::open(&target)
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        assert_eq!(text, "i\n42\n");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn prior_statements_execute_once_and_only_final_rows_are_exported() {
        let directory = output_dir("once");
        let connection = connection();
        execute_export(
            &connection,
            "CREATE TABLE tally (n INTEGER); INSERT INTO tally VALUES (1); SELECT n FROM tally;",
            csv_options(&directory, 10),
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM tally", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            fs::read_to_string(directory.join("orders-part-00001.csv")).unwrap(),
            "n\n1\n"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    struct FailAfterRows {
        rows: AtomicU64,
        threshold: u64,
        cancelled: bool,
    }

    impl ExportObserver for FailAfterRows {
        fn check_cancelled(&self, _current_part: u64) -> Result<(), EngineError> {
            if self.rows.load(Ordering::SeqCst) >= self.threshold {
                if self.cancelled {
                    Err(EngineError::ExportCancelled)
                } else {
                    Err(EngineError::ExportWrite {
                        path: PathBuf::from("injected"),
                        message: "simulated disk full".into(),
                    })
                }
            } else {
                Ok(())
            }
        }

        fn rows_written(&self, rows: u64, _current_part: u64) {
            self.rows.fetch_add(rows, Ordering::SeqCst);
        }
    }

    #[test]
    fn failure_or_cancel_keeps_completed_parts_and_removes_current_stage() {
        for cancelled in [false, true] {
            let directory = output_dir(if cancelled {
                "partial-cancel"
            } else {
                "partial-disk-full"
            });
            let options = csv_options(&directory, 2_000).validate().unwrap();
            let observer = FailAfterRows {
                rows: AtomicU64::new(0),
                threshold: 2_001,
                cancelled,
            };
            let connection = connection();
            let error = execute_validated_export(
                &connection,
                "SELECT i, repeat('x', 20) AS payload FROM range(0, 5000) t(i)",
                &options,
                &observer,
            )
            .unwrap_err();
            if cancelled {
                assert!(matches!(error, EngineError::ExportCancelled));
            } else {
                assert!(matches!(error, EngineError::ExportWrite { .. }));
            }
            assert!(directory.join("orders-part-00001.csv").is_file());
            assert!(!directory.join("orders-part-00002.csv").exists());
            assert!(!fs::read_dir(&directory).unwrap().flatten().any(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".tarik-export-")
            }));
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn csv_without_header_and_custom_delimiter_round_trips() {
        let directory = output_dir("csv-options");
        let mut options = csv_options(&directory, 2);
        options.csv = Some(CsvExportOptions {
            delimiter: "|".into(),
            include_header: false,
        });
        let outcome = execute_export(
            &connection(),
            "SELECT i, i + 1 AS j FROM range(1, 4) t(i)",
            options,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(&outcome.completed_parts[0].path).unwrap(),
            "1|2\n2|3\n"
        );
        assert_eq!(
            fs::read_to_string(&outcome.completed_parts[1].path).unwrap(),
            "3|4\n"
        );

        // Ensure Arrow's matching reader accepts the emitted delimiter/headers.
        let schema = Arc::new(duckdb::arrow::datatypes::Schema::new(vec![
            duckdb::arrow::datatypes::Field::new(
                "i",
                duckdb::arrow::datatypes::DataType::Int64,
                true,
            ),
            duckdb::arrow::datatypes::Field::new(
                "j",
                duckdb::arrow::datatypes::DataType::Int64,
                true,
            ),
        ]));
        let file = fs::File::open(&outcome.completed_parts[0].path).unwrap();
        let mut reader = CsvReaderBuilder::new(schema)
            .with_header(false)
            .with_delimiter(b'|')
            .build(file)
            .unwrap();
        assert_eq!(reader.next().unwrap().unwrap().num_rows(), 2);
        fs::remove_dir_all(directory).unwrap();
    }
}

//! Versioned engine protocol types shared by the Tarik desktop app and engine
//! adapters. This crate intentionally depends only on serde and uuid so the
//! desktop build never pulls in database or Arrow crates.

use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_EXPORT_BASE_NAME_BYTES: usize = 128;
pub const MAX_EXPORT_ROWS_PER_PART: u64 = i64::MAX as u64;
/// Status and history retain only the most recent part summaries; aggregate
/// counters remain exact even when an export produces more files.
pub const MAX_REPORTED_EXPORT_PARTS: usize = 100;
pub const MIN_ENGINE_MEMORY_MIB: u64 = 128;
pub const MAX_ENGINE_MEMORY_MIB: u64 = 262_144;
pub const MIN_ENGINE_THREADS: u16 = 1;
pub const MAX_ENGINE_THREADS: u16 = 256;
pub const MAX_PROFILE_COLUMNS: usize = 100;
pub const MAX_PROFILE_SCALAR_BATCH_COLUMNS: usize = 25;
pub const MAX_PROFILE_VALUES: usize = 20;
pub const MAX_PROFILE_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_PROFILE_SNAPSHOT_BYTES: usize = 256 * 1024;
pub const MAX_AGENT_SQL_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineResourcePreset {
    LowMemory,
    Balanced,
    Fast,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineResourceSettings {
    pub preset: EngineResourcePreset,
    pub memory_limit_mib: u64,
    pub threads: u16,
}

impl Default for EngineResourceSettings {
    fn default() -> Self {
        Self::preset(EngineResourcePreset::Balanced)
    }
}

impl EngineResourceSettings {
    pub fn preset(preset: EngineResourcePreset) -> Self {
        let (memory_limit_mib, threads) = match preset {
            EngineResourcePreset::LowMemory => (512, 1),
            EngineResourcePreset::Balanced => (2_048, 2),
            EngineResourcePreset::Fast => (8_192, 4),
            EngineResourcePreset::Custom => (2_048, 2),
        };
        Self {
            preset,
            memory_limit_mib,
            threads,
        }
    }

    pub fn validate(&self) -> Result<(), EngineResourceValidationError> {
        if !(MIN_ENGINE_MEMORY_MIB..=MAX_ENGINE_MEMORY_MIB).contains(&self.memory_limit_mib) {
            return Err(EngineResourceValidationError::MemoryOutOfRange);
        }
        if !(MIN_ENGINE_THREADS..=MAX_ENGINE_THREADS).contains(&self.threads) {
            return Err(EngineResourceValidationError::ThreadsOutOfRange);
        }
        if self.preset != EngineResourcePreset::Custom {
            let canonical = Self::preset(self.preset);
            if self.memory_limit_mib != canonical.memory_limit_mib
                || self.threads != canonical.threads
            {
                return Err(EngineResourceValidationError::PresetMismatch);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineResourceValidationError {
    MemoryOutOfRange,
    ThreadsOutOfRange,
    PresetMismatch,
}

impl fmt::Display for EngineResourceValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MemoryOutOfRange => "memory limit must be between 128 MiB and 256 GiB",
            Self::ThreadsOutOfRange => "thread count must be between 1 and 256",
            Self::PresetMismatch => "preset values do not match the canonical profile",
        })
    }
}

impl std::error::Error for EngineResourceValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveEngineResources {
    pub preset: EngineResourcePreset,
    pub memory_limit_mib: u64,
    pub memory_limit_display: String,
    pub threads: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileTarget {
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileColumn {
    pub name: String,
    pub data_type: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMode {
    Approximate,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRequest {
    pub project_id: String,
    pub target: ProfileTarget,
    pub columns: Vec<ProfileColumn>,
    pub catalog_revision: String,
    pub mode: ProfileMode,
}

impl ProfileRequest {
    pub fn validate(&self) -> Result<(), ProfileValidationError> {
        if self.project_id.trim().is_empty()
            || self.target.database.trim().is_empty()
            || self.target.schema.trim().is_empty()
            || self.target.name.trim().is_empty()
            || self.catalog_revision.trim().is_empty()
        {
            return Err(ProfileValidationError::MissingIdentity);
        }
        if !matches!(self.target.kind.as_str(), "table" | "view") {
            return Err(ProfileValidationError::UnsupportedTarget);
        }
        if self.columns.is_empty() {
            return Err(ProfileValidationError::NoColumns);
        }
        if self.columns.len() > MAX_PROFILE_COLUMNS {
            return Err(ProfileValidationError::TooManyColumns);
        }
        if self
            .columns
            .iter()
            .any(|column| column.name.trim().is_empty() || column.data_type.trim().is_empty())
        {
            return Err(ProfileValidationError::InvalidColumn);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileValidationError {
    MissingIdentity,
    UnsupportedTarget,
    NoColumns,
    TooManyColumns,
    InvalidColumn,
}

impl fmt::Display for ProfileValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingIdentity => "profile identity fields cannot be empty",
            Self::UnsupportedTarget => "profile target must be a table or view",
            Self::NoColumns => "at least one profile column is required",
            Self::TooManyColumns => "profile requests support at most 100 columns",
            Self::InvalidColumn => "profile column name and type cannot be empty",
        })
    }
}

impl std::error::Error for ProfileValidationError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricProvenance {
    Exact,
    Approximate,
    Sampled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileMetricKind {
    RowCount,
    NullCount,
    NullRate,
    DistinctCount,
    Minimum,
    Maximum,
    Average,
    TextLengthMinimum,
    TextLengthMaximum,
    TextLengthAverage,
    CommonValues,
    RepresentativeValues,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMetric {
    pub column: Option<String>,
    pub kind: ProfileMetricKind,
    pub value: Option<serde_json::Value>,
    pub provenance: MetricProvenance,
    pub unavailable_reason: Option<String>,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSqlEvidence {
    pub columns: Vec<String>,
    pub metric_kinds: Vec<ProfileMetricKind>,
    pub sql: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSnapshot {
    pub project_id: String,
    pub target: ProfileTarget,
    pub catalog_revision: String,
    pub mode: ProfileMode,
    pub observed_at_unix_ms: u64,
    pub metrics: Vec<ProfileMetric>,
    pub statements: Vec<ProfileSqlEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileStatus {
    pub profile_id: String,
    pub state: ProfileState,
    pub duration_ms: u64,
    pub snapshot: Option<ProfileSnapshot>,
    pub error: Option<ErrorEnvelope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Parquet,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Parquet => "parquet",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportOverwritePolicy {
    FailIfExists,
    Replace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParquetCompression {
    Uncompressed,
    Snappy,
    Gzip,
    Zstd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvExportOptions {
    pub delimiter: String,
    pub include_header: bool,
}

impl Default for CsvExportOptions {
    fn default() -> Self {
        Self {
            delimiter: ",".into(),
            include_header: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParquetExportOptions {
    pub compression: ParquetCompression,
}

impl Default for ParquetExportOptions {
    fn default() -> Self {
        Self {
            compression: ParquetCompression::Snappy,
        }
    }
}

/// Untrusted export options crossing the desktop → engine boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub format: ExportFormat,
    pub output_directory: String,
    pub base_name: String,
    pub rows_per_part: u64,
    pub overwrite: ExportOverwritePolicy,
    pub csv: Option<CsvExportOptions>,
    pub parquet: Option<ParquetExportOptions>,
}

/// Canonical, internally trusted options. Construct only with
/// [`ExportOptions::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedExportOptions {
    pub format: ExportFormat,
    pub output_directory: PathBuf,
    pub base_name: String,
    pub rows_per_part: u64,
    pub overwrite: ExportOverwritePolicy,
    pub csv: Option<CsvExportOptions>,
    pub parquet: Option<ParquetExportOptions>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExportValidationError {
    OutputDirectoryEmpty,
    OutputDirectoryNotAbsolute,
    OutputDirectoryMissing,
    OutputPathNotDirectory,
    OutputDirectoryUnreadable,
    BaseNameEmpty,
    BaseNameTooLong,
    BaseNameUnsafe,
    RowsPerPartZero,
    RowsPerPartTooLarge,
    CsvOptionsRequired,
    CsvOptionsUnexpected,
    CsvDelimiterInvalid,
    ParquetOptionsRequired,
    ParquetOptionsUnexpected,
    PartNumberZero,
}

impl ExportValidationError {
    pub fn field(&self) -> &'static str {
        match self {
            Self::OutputDirectoryEmpty
            | Self::OutputDirectoryNotAbsolute
            | Self::OutputDirectoryMissing
            | Self::OutputPathNotDirectory
            | Self::OutputDirectoryUnreadable => "outputDirectory",
            Self::BaseNameEmpty | Self::BaseNameTooLong | Self::BaseNameUnsafe => "baseName",
            Self::RowsPerPartZero | Self::RowsPerPartTooLarge => "rowsPerPart",
            Self::CsvOptionsRequired | Self::CsvOptionsUnexpected => "csv",
            Self::CsvDelimiterInvalid => "csv.delimiter",
            Self::ParquetOptionsRequired | Self::ParquetOptionsUnexpected => "parquet",
            Self::PartNumberZero => "partNumber",
        }
    }
}

impl fmt::Display for ExportValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::OutputDirectoryEmpty => "output directory cannot be empty",
            Self::OutputDirectoryNotAbsolute => "output directory must be an absolute path",
            Self::OutputDirectoryMissing => "output directory does not exist",
            Self::OutputPathNotDirectory => "output path is not a directory",
            Self::OutputDirectoryUnreadable => "output directory could not be resolved",
            Self::BaseNameEmpty => "base name cannot be empty",
            Self::BaseNameTooLong => "base name is too long",
            Self::BaseNameUnsafe => {
                "base name may contain only ASCII letters, digits, hyphens, and underscores"
            }
            Self::RowsPerPartZero => "rows per part must be greater than zero",
            Self::RowsPerPartTooLarge => "rows per part exceeds the supported range",
            Self::CsvOptionsRequired => "CSV options are required for CSV export",
            Self::CsvOptionsUnexpected => "CSV options are not valid for Parquet export",
            Self::CsvDelimiterInvalid => {
                "CSV delimiter must be one ASCII byte other than NUL, quote, CR, or LF"
            }
            Self::ParquetOptionsRequired => "Parquet options are required for Parquet export",
            Self::ParquetOptionsUnexpected => "Parquet options are not valid for CSV export",
            Self::PartNumberZero => "part number must start at one",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for ExportValidationError {}

impl ExportOptions {
    /// Validate and normalize without executing SQL or creating output files.
    pub fn validate(self) -> Result<ValidatedExportOptions, ExportValidationError> {
        let output = self.output_directory.trim();
        if output.is_empty() {
            return Err(ExportValidationError::OutputDirectoryEmpty);
        }
        let output = PathBuf::from(output);
        if !output.is_absolute() {
            return Err(ExportValidationError::OutputDirectoryNotAbsolute);
        }
        if !output.exists() {
            return Err(ExportValidationError::OutputDirectoryMissing);
        }
        if !output.is_dir() {
            return Err(ExportValidationError::OutputPathNotDirectory);
        }
        let output_directory = output
            .canonicalize()
            .map_err(|_| ExportValidationError::OutputDirectoryUnreadable)?;

        let base_name = self.base_name.trim();
        if base_name.is_empty() {
            return Err(ExportValidationError::BaseNameEmpty);
        }
        if base_name.len() > MAX_EXPORT_BASE_NAME_BYTES {
            return Err(ExportValidationError::BaseNameTooLong);
        }
        if !base_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(ExportValidationError::BaseNameUnsafe);
        }
        if self.rows_per_part == 0 {
            return Err(ExportValidationError::RowsPerPartZero);
        }
        if self.rows_per_part > MAX_EXPORT_ROWS_PER_PART {
            return Err(ExportValidationError::RowsPerPartTooLarge);
        }

        match self.format {
            ExportFormat::Csv => {
                let csv = self
                    .csv
                    .as_ref()
                    .ok_or(ExportValidationError::CsvOptionsRequired)?;
                if self.parquet.is_some() {
                    return Err(ExportValidationError::ParquetOptionsUnexpected);
                }
                let delimiter = csv.delimiter.as_bytes();
                if delimiter.len() != 1
                    || !delimiter[0].is_ascii()
                    || matches!(delimiter[0], 0 | b'\"' | b'\r' | b'\n')
                {
                    return Err(ExportValidationError::CsvDelimiterInvalid);
                }
            }
            ExportFormat::Parquet => {
                if self.csv.is_some() {
                    return Err(ExportValidationError::CsvOptionsUnexpected);
                }
                if self.parquet.is_none() {
                    return Err(ExportValidationError::ParquetOptionsRequired);
                }
            }
        }

        Ok(ValidatedExportOptions {
            format: self.format,
            output_directory,
            base_name: base_name.to_string(),
            rows_per_part: self.rows_per_part,
            overwrite: self.overwrite,
            csv: self.csv,
            parquet: self.parquet,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPartSummary {
    pub part_number: u64,
    pub path: String,
    pub rows: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportStatus {
    pub export_id: String,
    pub state: ExportState,
    pub duration_ms: u64,
    pub rows_written: u64,
    pub files_written: u64,
    pub bytes_written: u64,
    pub current_part: Option<u64>,
    pub completed_parts: Vec<ExportPartSummary>,
    pub error: Option<ErrorEnvelope>,
}

impl ValidatedExportOptions {
    pub fn to_options(&self) -> ExportOptions {
        ExportOptions {
            format: self.format,
            output_directory: self.output_directory.to_string_lossy().into_owned(),
            base_name: self.base_name.clone(),
            rows_per_part: self.rows_per_part,
            overwrite: self.overwrite,
            csv: self.csv.clone(),
            parquet: self.parquet.clone(),
        }
    }

    pub fn part_file_name(&self, part_number: u64) -> Result<String, ExportValidationError> {
        if part_number == 0 {
            return Err(ExportValidationError::PartNumberZero);
        }
        Ok(format!(
            "{}-part-{part_number:05}.{}",
            self.base_name,
            self.format.extension()
        ))
    }

    pub fn part_path(&self, part_number: u64) -> Result<PathBuf, ExportValidationError> {
        Ok(self
            .output_directory
            .join(self.part_file_name(part_number)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub schemas: bool,
    pub views: bool,
    pub cancel_query: bool,
    pub explain: bool,
    pub profile: bool,
    #[serde(default)]
    pub data_profiling: bool,
    pub link_parquet: bool,
    pub import_csv: bool,
    pub import_parquet: bool,
    pub transactions: bool,
    pub bounded_pages: bool,
    pub export_csv: bool,
    pub export_parquet: bool,
    #[serde(default)]
    pub resource_controls: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            schemas: true,
            views: true,
            cancel_query: true,
            explain: true,
            profile: true,
            data_profiling: false,
            link_parquet: false,
            import_csv: false,
            import_parquet: false,
            transactions: true,
            bounded_pages: true,
            export_csv: true,
            export_parquet: true,
            resource_controls: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineInfo {
    pub engine_id: String,
    pub engine_name: String,
    pub engine_version: String,
    pub protocol_version: u32,
    pub capabilities: Capabilities,
    /// Engine-specific free-form metadata.
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectLocator {
    pub engine_id: String,
    /// Engine-specific locator payload, e.g. `{ "path": "/data/retail.duckdb" }`.
    pub payload: serde_json::Map<String, serde_json::Value>,
}

impl ProjectLocator {
    pub fn duckdb_path(locator: &ProjectLocator) -> Option<&str> {
        if locator.engine_id != "duckdb" {
            return None;
        }
        locator
            .payload
            .get("path")
            .and_then(serde_json::Value::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTableColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTableDefinition {
    pub name: String,
    pub columns: Vec<CreateTableColumn>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnInfo {
    pub name: String,
    /// Generic logical type used for frontend formatting, e.g. `decimal`.
    pub logical_type: String,
    /// Engine-native type, e.g. `DECIMAL(18,2)`.
    pub native_type: String,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultInfo {
    pub result_id: String,
    pub columns: Vec<ColumnInfo>,
    pub row_count: u64,
    pub row_count_exact: bool,
    /// Location of the bounded page artifact directory.
    pub page_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSqlDecision {
    SafeRead,
    ApprovalRequired,
    CriticalConfirmation,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRegisteredSource {
    pub source_id: String,
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSqlClassification {
    pub decision: AgentSqlDecision,
    pub reason_code: String,
    pub statement_type: String,
    pub catalog_revision: String,
    pub affected_objects: Vec<String>,
    pub has_top_level_filter: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogObject {
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: String,
    pub estimated_row_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogColumn {
    pub database: String,
    pub schema: String,
    pub object: String,
    pub name: String,
    pub data_type: String,
    pub position: u32,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub revision: String,
    pub objects: Vec<CatalogObject>,
    pub columns: Vec<CatalogColumn>,
}

pub fn catalog_revision(objects: &[CatalogObject], columns: &[CatalogColumn]) -> String {
    // Stable FNV-1a over ordered catalog identity. This is a staleness token,
    // not a cryptographic digest; the catalog query supplies deterministic order.
    let mut hash = 0xcbf29ce484222325u64;
    let mut write = |value: &str| {
        for byte in value.as_bytes().iter().copied().chain(std::iter::once(0)) {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    };
    for object in objects {
        write(&object.database);
        write(&object.schema);
        write(&object.name);
        write(&object.kind);
    }
    for column in columns {
        write(&column.database);
        write(&column.schema);
        write(&column.object);
        write(&column.name);
        write(&column.data_type);
        write(if column.nullable { "1" } else { "0" });
    }
    format!("{hash:016x}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    DuckdbTable,
    LinkedParquet,
    LinkedCsv,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceState {
    Ready,
    Missing,
    InvalidSchema,
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
    pub path: String,
    pub format: String,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRecord {
    pub id: String,
    pub project_id: String,
    pub display_name: String,
    pub kind: SourceKind,
    pub state: SourceState,
    pub source_path: Option<String>,
    pub duckdb_name: String,
    pub options: serde_json::Map<String, serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

/// Lifecycle of one query execution submitted to an engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

/// Observed status of one engine execution. The engine owns the transition
/// from queued to a single terminal state; the desktop persists history from
/// the terminal snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStatus {
    pub execution_id: String,
    pub state: ExecutionState,
    pub duration_ms: u64,
    /// Rows produced so far while running, or the final row count once
    /// terminal. `None` for statements without a row set.
    pub rows_produced: Option<u64>,
    /// Rows changed by DML statements when no result set was produced.
    pub rows_affected: Option<u64>,
    pub error: Option<ErrorEnvelope>,
    /// Published bounded result metadata once the execution succeeded with
    /// a row set.
    #[serde(default)]
    pub result: Option<ResultInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
    pub request_id: Option<String>,
    /// Optional engine-specific detail, e.g. DuckDB error position.
    #[serde(default)]
    pub details: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlDiagnostic {
    pub code: String,
    pub message: String,
    pub severity: DiagnosticSeverity,
    /// UTF-16 document offsets for CodeMirror. Missing when DuckDB did not
    /// provide a mechanically reliable source location.
    pub from: Option<u32>,
    pub to: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlValidation {
    pub revision: u64,
    pub diagnostics: Vec<SqlDiagnostic>,
}

impl ErrorEnvelope {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            request_id: None,
            details: serde_json::Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestEnvelope {
    pub id: String,
    pub method: String,
    /// Method parameters; schemas are defined per method.
    #[serde(default)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseEnvelope {
    pub id: String,
    pub ok: bool,
    pub error: Option<ErrorEnvelope>,
    /// Successful method result.
    pub result: Option<serde_json::Value>,
}

impl ResponseEnvelope {
    pub fn ok(id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            id: id.into(),
            ok: true,
            error: None,
            result: Some(result),
        }
    }

    pub fn err(id: impl Into<String>, error: ErrorEnvelope) -> Self {
        Self {
            id: id.into(),
            ok: false,
            error: Some(error),
            result: None,
        }
    }
}

pub fn new_request_id() -> String {
    Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn duckdb_capabilities() -> Capabilities {
        Capabilities {
            link_parquet: true,
            import_csv: true,
            import_parquet: true,
            ..Default::default()
        }
    }

    #[test]
    fn request_envelope_round_trip_keeps_ids_and_unknown_params() {
        let request = RequestEnvelope {
            id: "req-1".into(),
            method: "catalog.inspect".into(),
            params: serde_json::json!({ "sessionId": "s1", "futureField": 1 })
                .as_object()
                .unwrap()
                .clone(),
        };
        let wire = serde_json::to_string(&request).unwrap();
        let decoded: RequestEnvelope = serde_json::from_str(&wire).unwrap();
        assert_eq!(decoded.id, "req-1");
        assert_eq!(decoded.params["futureField"], 1);
    }

    #[test]
    fn duckdb_locator_extracts_path_only_for_duckdb() {
        let locator = ProjectLocator {
            engine_id: "duckdb".into(),
            payload: serde_json::json!({ "path": "/data/retail.duckdb" })
                .as_object()
                .unwrap()
                .clone(),
        };
        assert_eq!(
            ProjectLocator::duckdb_path(&locator),
            Some("/data/retail.duckdb")
        );

        let postgres = ProjectLocator {
            engine_id: "postgres".into(),
            payload: serde_json::json!({ "host": "localhost" })
                .as_object()
                .unwrap()
                .clone(),
        };
        assert_eq!(ProjectLocator::duckdb_path(&postgres), None);
    }

    #[test]
    fn capabilities_are_negotiable_per_engine() {
        let duckdb = duckdb_capabilities();
        let postgres = Capabilities::default();
        assert!(duckdb.link_parquet);
        assert!(!postgres.link_parquet);
    }

    fn profile_request() -> ProfileRequest {
        ProfileRequest {
            project_id: "project-1".into(),
            target: ProfileTarget {
                database: "memory".into(),
                schema: "main".into(),
                name: "orders".into(),
                kind: "table".into(),
            },
            columns: vec![ProfileColumn {
                name: "order id".into(),
                data_type: "BIGINT".into(),
            }],
            catalog_revision: "abc123".into(),
            mode: ProfileMode::Approximate,
        }
    }

    #[test]
    fn profile_request_is_closed_bounded_and_serializable() {
        let request = profile_request();
        request.validate().unwrap();
        let wire = serde_json::to_value(&request).unwrap();
        assert_eq!(wire["mode"], "approximate");
        assert_eq!(wire["target"]["kind"], "table");
        let evidence = ProfileSqlEvidence {
            columns: vec!["order id".into()],
            metric_kinds: vec![ProfileMetricKind::NullCount, ProfileMetricKind::NullRate],
            sql: "SELECT count(*) FILTER (WHERE \"order id\" IS NULL) FROM \"orders\"".into(),
        };
        assert_eq!(
            serde_json::from_value::<ProfileSqlEvidence>(serde_json::to_value(&evidence).unwrap())
                .unwrap(),
            evidence
        );
        assert_eq!(
            serde_json::from_value::<ProfileRequest>(wire).unwrap(),
            request
        );

        let mut too_wide = profile_request();
        too_wide.columns = (0..=MAX_PROFILE_COLUMNS)
            .map(|index| ProfileColumn {
                name: format!("column-{index}"),
                data_type: "INTEGER".into(),
            })
            .collect();
        assert_eq!(
            too_wide.validate(),
            Err(ProfileValidationError::TooManyColumns)
        );
    }

    #[test]
    fn catalog_revision_is_stable_and_changes_with_column_identity() {
        let objects = vec![CatalogObject {
            database: "memory".into(),
            schema: "main".into(),
            name: "orders".into(),
            kind: "table".into(),
            estimated_row_count: Some(2),
        }];
        let mut columns = vec![CatalogColumn {
            database: "memory".into(),
            schema: "main".into(),
            object: "orders".into(),
            name: "id".into(),
            data_type: "BIGINT".into(),
            position: 0,
            nullable: false,
        }];
        let first = catalog_revision(&objects, &columns);
        assert_eq!(first, catalog_revision(&objects, &columns));
        columns[0].data_type = "VARCHAR".into();
        assert_ne!(first, catalog_revision(&objects, &columns));
    }

    #[test]
    fn engine_resource_presets_are_canonical_and_bounded() {
        assert_eq!(
            EngineResourceSettings::preset(EngineResourcePreset::LowMemory),
            EngineResourceSettings {
                preset: EngineResourcePreset::LowMemory,
                memory_limit_mib: 512,
                threads: 1,
            }
        );
        assert_eq!(
            EngineResourceSettings::preset(EngineResourcePreset::Balanced).memory_limit_mib,
            2_048
        );
        assert_eq!(
            EngineResourceSettings::preset(EngineResourcePreset::Fast).threads,
            4
        );
        assert!(EngineResourceSettings {
            preset: EngineResourcePreset::Custom,
            memory_limit_mib: MIN_ENGINE_MEMORY_MIB,
            threads: MAX_ENGINE_THREADS,
        }
        .validate()
        .is_ok());
        assert_eq!(
            EngineResourceSettings {
                preset: EngineResourcePreset::Balanced,
                memory_limit_mib: 512,
                threads: 2,
            }
            .validate(),
            Err(EngineResourceValidationError::PresetMismatch)
        );
        assert_eq!(
            EngineResourceSettings {
                preset: EngineResourcePreset::Custom,
                memory_limit_mib: MIN_ENGINE_MEMORY_MIB - 1,
                threads: 1,
            }
            .validate(),
            Err(EngineResourceValidationError::MemoryOutOfRange)
        );
    }

    #[test]
    fn error_envelope_carries_code_message_and_request_id() {
        let response = ResponseEnvelope::err(
            "req-9",
            ErrorEnvelope {
                request_id: Some("req-9".into()),
                ..ErrorEnvelope::new("sql.parse", "near line 2: syntax error")
            },
        );
        assert!(!response.ok);
        assert_eq!(response.error.as_ref().unwrap().code, "sql.parse");
        assert_eq!(response.id, "req-9");
    }

    fn export_directory() -> PathBuf {
        let path = std::env::temp_dir().join(format!("tarik-export-options-{}", Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        path
    }

    fn csv_options(directory: &std::path::Path) -> ExportOptions {
        ExportOptions {
            format: ExportFormat::Csv,
            output_directory: directory.to_string_lossy().into_owned(),
            base_name: "orders_2026".into(),
            rows_per_part: 10_000,
            overwrite: ExportOverwritePolicy::FailIfExists,
            csv: Some(CsvExportOptions::default()),
            parquet: None,
        }
    }

    #[test]
    fn export_options_round_trip_with_closed_wire_variants() {
        let directory = export_directory();
        let options = ExportOptions {
            format: ExportFormat::Parquet,
            output_directory: directory.to_string_lossy().into_owned(),
            base_name: "monthly-orders".into(),
            rows_per_part: 50_000,
            overwrite: ExportOverwritePolicy::Replace,
            csv: None,
            parquet: Some(ParquetExportOptions {
                compression: ParquetCompression::Zstd,
            }),
        };

        let wire = serde_json::to_value(&options).unwrap();
        assert_eq!(wire["format"], "parquet");
        assert_eq!(wire["overwrite"], "replace");
        assert_eq!(wire["parquet"]["compression"], "zstd");
        assert_eq!(
            serde_json::from_value::<ExportOptions>(wire).unwrap(),
            options
        );
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn export_validation_normalizes_and_generates_stable_part_names() {
        let directory = export_directory();
        let mut options = csv_options(&directory);
        options.base_name = "  orders_2026  ".into();
        let validated = options.validate().unwrap();

        assert_eq!(validated.base_name, "orders_2026");
        assert_eq!(
            validated.part_file_name(1).unwrap(),
            "orders_2026-part-00001.csv"
        );
        assert_eq!(
            validated.part_file_name(99_999).unwrap(),
            "orders_2026-part-99999.csv"
        );
        assert_eq!(
            validated.part_file_name(100_000).unwrap(),
            "orders_2026-part-100000.csv"
        );
        assert_eq!(
            validated.part_path(2).unwrap(),
            directory
                .canonicalize()
                .unwrap()
                .join("orders_2026-part-00002.csv")
        );
        assert_eq!(
            validated.part_file_name(0).unwrap_err(),
            ExportValidationError::PartNumberZero
        );
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn export_validation_rejects_unsafe_names_and_row_boundaries() {
        let directory = export_directory();
        for name in ["", "../orders", "order items", "orders.csv", "café"] {
            let mut options = csv_options(&directory);
            options.base_name = name.into();
            let error = options.validate().unwrap_err();
            assert_eq!(error.field(), "baseName", "unexpected error for {name:?}");
        }

        let mut options = csv_options(&directory);
        options.rows_per_part = 0;
        assert_eq!(
            options.validate().unwrap_err(),
            ExportValidationError::RowsPerPartZero
        );
        let mut options = csv_options(&directory);
        options.rows_per_part = MAX_EXPORT_ROWS_PER_PART + 1;
        assert_eq!(
            options.validate().unwrap_err(),
            ExportValidationError::RowsPerPartTooLarge
        );
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn export_validation_rejects_invalid_or_cross_format_options() {
        let directory = export_directory();
        for delimiter in ["", "||", "é", "\0", "\"", "\n"] {
            let mut options = csv_options(&directory);
            options.csv.as_mut().unwrap().delimiter = delimiter.into();
            assert_eq!(
                options.validate().unwrap_err(),
                ExportValidationError::CsvDelimiterInvalid
            );
        }

        let mut options = csv_options(&directory);
        options.parquet = Some(ParquetExportOptions::default());
        assert_eq!(
            options.validate().unwrap_err(),
            ExportValidationError::ParquetOptionsUnexpected
        );
        let options = ExportOptions {
            format: ExportFormat::Parquet,
            output_directory: directory.to_string_lossy().into_owned(),
            base_name: "orders".into(),
            rows_per_part: 1,
            overwrite: ExportOverwritePolicy::FailIfExists,
            csv: Some(CsvExportOptions::default()),
            parquet: Some(ParquetExportOptions::default()),
        };
        assert_eq!(
            options.validate().unwrap_err(),
            ExportValidationError::CsvOptionsUnexpected
        );
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn export_validation_rejects_invalid_directories_without_creating_them() {
        let root = export_directory();
        let missing = root.join("missing");
        let options = csv_options(&missing);
        assert_eq!(
            options.validate().unwrap_err(),
            ExportValidationError::OutputDirectoryMissing
        );
        assert!(!missing.exists());

        let file = root.join("file");
        std::fs::write(&file, b"not a directory").unwrap();
        let options = csv_options(&file);
        assert_eq!(
            options.validate().unwrap_err(),
            ExportValidationError::OutputPathNotDirectory
        );
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}

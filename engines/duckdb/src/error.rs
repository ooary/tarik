use std::path::PathBuf;

/// Either a filesystem error or an Arrow/IPC error behind one code.
#[derive(Debug, thiserror::Error)]
pub enum CacheErrorSource {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("arrow error: {0}")]
    Arrow(String),
}

impl From<duckdb::arrow::error::ArrowError> for CacheErrorSource {
    fn from(value: duckdb::arrow::error::ArrowError) -> Self {
        Self::Arrow(value.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("session already exists: {0}")]
    SessionExists(String),
    #[error("session does not exist: {0}")]
    SessionMissing(String),
    #[error("execution already exists: {0}")]
    ExecutionExists(String),
    #[error("execution does not exist: {0}")]
    ExecutionMissing(String),
    #[error("export already exists: {0}")]
    ExportExists(String),
    #[error("export does not exist: {0}")]
    ExportMissing(String),
    #[error("export was cancelled")]
    ExportCancelled,
    #[error("result does not exist: {0}")]
    ResultMissing(String),
    #[error("invalid query request: {0}")]
    InvalidQuery(&'static str),
    #[error("could not spawn job worker: {0}")]
    WorkerSpawn(String),
    #[error("engine job registry state is unavailable")]
    RegistryPoisoned,
    #[error("result cache error at {path}: {source}")]
    CacheIo {
        path: PathBuf,
        source: CacheErrorSource,
    },
    #[error("result display conversion failed: {0}")]
    CacheDisplay(String),
    #[error("invalid export options: {0}")]
    ExportInvalid(String),
    #[error("export destination already exists: {0}")]
    ExportCollision(PathBuf),
    #[error("export file operation failed at {path}: {source}")]
    ExportIo {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("export writer failed at {path}: {message}")]
    ExportWrite { path: PathBuf, message: String },
    #[error("source file does not exist: {0}")]
    Missing(PathBuf),
    #[error("unsupported source type: {0}")]
    Unsupported(PathBuf),
    #[error("source path is not valid UTF-8: {0}")]
    InvalidPath(PathBuf),
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
    #[error("could not read path metadata {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("method not supported: {0}")]
    MethodNotFound(String),
    #[error("missing required request field: {0}")]
    MissingField(String),
    #[error(transparent)]
    DuckDb(#[from] duckdb::Error),
    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),
}

impl EngineError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::SessionExists(_) => "session.exists",
            Self::SessionMissing(_) => "session.missing",
            Self::ExecutionExists(_) => "execution.exists",
            Self::ExecutionMissing(_) => "execution.missing",
            Self::ExportExists(_) => "export.exists",
            Self::ExportMissing(_) => "export.missing",
            Self::ExportCancelled => "export.cancelled",
            Self::ResultMissing(_) => "result.missing",
            Self::InvalidQuery(_) => "query.invalid",
            Self::WorkerSpawn(_) => "engine.worker_spawn",
            Self::RegistryPoisoned => "engine.registry",
            Self::CacheIo { .. } | Self::CacheDisplay(_) => "cache.io",
            Self::ExportInvalid(_) => "export.invalid_options",
            Self::ExportCollision(_) => "export.collision",
            Self::ExportIo { .. } => "export.io",
            Self::ExportWrite { .. } => "export.write",
            Self::Missing(_) => "source.missing",
            Self::Unsupported(_) => "source.unsupported",
            Self::InvalidPath(_) => "source.invalid_path",
            Self::InvalidGlob(_) => "source.invalid_glob",
            Self::InvalidIdentifier => "source.invalid_identifier",
            Self::InvalidOptions(_) => "source.invalid_options",
            Self::InvalidDataType(_) => "source.invalid_data_type",
            Self::IncompatibleSchema { .. } => "source.incompatible_schema",
            Self::Io { .. } => "io.error",
            Self::MethodNotFound(_) => "method.not_found",
            Self::MissingField(_) => "request.missing_field",
            Self::DuckDb(_) => "duckdb.error",
            Self::Serde(_) => "request.parse",
        }
    }
}

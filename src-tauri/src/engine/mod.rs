mod catalog;
mod profile;
pub mod worker;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use profile::{EngineProfile, EngineProfileName};
pub use worker::DuckDbWorker;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogObjectKind {
    Table,
    View,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogObject {
    pub database: String,
    pub schema: String,
    pub name: String,
    pub kind: CatalogObjectKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCatalog {
    pub objects: Vec<CatalogObject>,
    pub columns: Vec<CatalogColumn>,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("could not open DuckDB project at {path}: {source}")]
    Open {
        path: PathBuf,
        source: duckdb::Error,
    },
    #[error("DuckDB job queue is full")]
    QueueFull,
    #[error("DuckDB worker stopped")]
    WorkerStopped,
    #[error("DuckDB worker panicked")]
    WorkerPanicked,
    #[error("DuckDB interrupt handle lock is unavailable")]
    InterruptLock,
    #[error("could not create DuckDB worker thread: {0}")]
    Thread(std::io::Error),
    #[error("invalid engine profile: {0}")]
    InvalidProfile(&'static str),
    #[error(transparent)]
    Source(#[from] crate::sources::SourceError),
    #[error(transparent)]
    DuckDb(#[from] duckdb::Error),
}

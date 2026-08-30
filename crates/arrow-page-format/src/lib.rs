//! Placeholder for bounded Arrow IPC/Parquet result-page interchange.
//! E5.5-T2 fills this crate with page splitting, spill, and export writers.

use tarik_engine_protocol::{PageInfo, ResultInfo};

#[derive(Debug, thiserror::Error)]
pub enum PageError {
    #[error("result page directory does not exist: {0}")]
    MissingDirectory(String),
    #[error("page artifact is missing: {0}")]
    MissingArtifact(String),
    #[error("unsupported page artifact: {0}")]
    UnsupportedArtifact(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageCursor {
    pub result_id: String,
    pub page_dir: String,
    pub offset: u64,
    pub rows: u64,
}

impl PageCursor {
    pub fn from_result(result: &ResultInfo) -> Self {
        Self {
            result_id: result.result_id.clone(),
            page_dir: result.page_dir.clone(),
            offset: 0,
            rows: 0,
        }
    }

    pub fn advance(&mut self, page: &PageInfo) {
        self.offset = page.offset + page.rows;
        self.rows += page.rows;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_cursor_tracks_bounded_advancement() {
        let result = ResultInfo {
            result_id: "result-1".into(),
            columns: Vec::new(),
            row_count: 100,
            row_count_exact: true,
            page_dir: "/cache/result-1".into(),
        };
        let mut cursor = PageCursor::from_result(&result);
        cursor.advance(&PageInfo {
            offset: 0,
            rows: 500,
            artifact: "/cache/result-1/page-0.arrow".into(),
        });
        assert_eq!(cursor.offset, 500);
        assert_eq!(cursor.rows, 500);
    }
}

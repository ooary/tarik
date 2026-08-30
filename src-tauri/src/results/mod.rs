//! Bounded result page access.
//!
//! The engine owns the page artifacts; this module proxies page requests and
//! keeps a small decoded-page LRU so scrolling does not re-decode the same
//! page artifacts. The cache is bounded by entry count, and every entry is
//! dropped on release.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use serde::Serialize;

use crate::engine_manager::EngineManager;

/// Overall decoded-page budget across all results. Three pages per result
/// covers smooth back-and-forth paging within this total.
const MAX_TOTAL_PAGES: usize = 12;

pub trait ResultsEngine: Send + Sync + 'static {
    fn get_page(
        &self,
        result_id: &str,
        offset: u64,
        max_rows: u32,
    ) -> Result<serde_json::Value, String>;
    fn release(&self, result_id: &str) -> Result<(), String>;
}

impl ResultsEngine for EngineManager {
    fn get_page(
        &self,
        result_id: &str,
        offset: u64,
        max_rows: u32,
    ) -> Result<serde_json::Value, String> {
        self.result_page(result_id, offset, max_rows)
    }

    fn release(&self, result_id: &str) -> Result<(), String> {
        self.release_result(result_id)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultPageView {
    pub result_id: String,
    pub offset: u64,
    pub row_total: u64,
    pub row_total_exact: bool,
    pub columns: serde_json::Value,
    pub rows: serde_json::Value,
    pub truncated_cells: serde_json::Value,
    /// True when served from the desktop page cache.
    pub cached: bool,
}

struct CacheEntry {
    page: ResultPageView,
}

pub struct ResultStore {
    engine: Arc<dyn ResultsEngine>,
    cache: Mutex<CacheState>,
    page_rows: u32,
}

#[derive(Default)]
struct CacheState {
    entries: HashMap<(String, u64), CacheEntry>,
    order: VecDeque<(String, u64)>,
}

impl ResultStore {
    pub fn new(engine: Arc<dyn ResultsEngine>) -> Self {
        Self {
            engine,
            cache: Mutex::new(CacheState::default()),
            page_rows: 500,
        }
    }

    /// Fetch one aligned page window for a result.
    pub fn get_page(&self, result_id: &str, offset: u64) -> Result<ResultPageView, String> {
        let aligned = offset - (offset % u64::from(self.page_rows));
        let key = (result_id.to_string(), aligned);
        if let Ok(mut state) = self.cache.lock() {
            if let Some(hit) = state.entry_get(&key) {
                let mut hit = hit.clone();
                hit.cached = true;
                return Ok(hit);
            }
        }

        let page = self.engine.get_page(result_id, aligned, self.page_rows)?;
        let view = ResultPageView {
            result_id: result_id.to_string(),
            offset: aligned,
            row_total: page
                .get("rowTotal")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            row_total_exact: page
                .get("rowTotalExact")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            columns: page
                .get("columns")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            rows: page.get("rows").cloned().unwrap_or(serde_json::Value::Null),
            truncated_cells: page
                .get("truncatedCells")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            cached: false,
        };
        self.cache_put(key, view.clone());
        Ok(view)
    }

    /// Release a result: drop engine artifacts and every cached page.
    pub fn release(&self, result_id: &str) -> Result<(), String> {
        self.engine.release(result_id)?;
        if let Ok(mut state) = self.cache.lock() {
            state.evict_result(result_id);
        }
        Ok(())
    }

    fn cache_put(&self, key: (String, u64), page: ResultPageView) {
        let Ok(mut state) = self.cache.lock() else {
            return;
        };
        state.entry_put(key, page);
        while state.order.len() > MAX_TOTAL_PAGES {
            if let Some(oldest) = state.order.pop_front() {
                state.entries.remove(&oldest);
            }
        }
    }
}

impl CacheState {
    fn entry_get(&mut self, key: &(String, u64)) -> Option<&ResultPageView> {
        if let Some(position) = self.order.iter().position(|entry| entry == key) {
            if let Some(entry) = self.order.remove(position) {
                self.order.push_back(entry);
            }
        }
        self.entries.get(key).map(|entry| &entry.page)
    }

    fn entry_put(&mut self, key: (String, u64), page: ResultPageView) {
        if self.entries.contains_key(&key) {
            self.order.retain(|entry| entry != &key);
        }
        self.entries.insert(key.clone(), CacheEntry { page });
        self.order.push_back(key);
    }

    fn evict_result(&mut self, result_id: &str) {
        self.entries.retain(|(id, _), _| id != result_id);
        self.order.retain(|(id, _)| id != result_id);
    }
}

#[tauri::command]
pub fn get_result_page(
    result_id: String,
    offset: u64,
    store: tauri::State<'_, Arc<ResultStore>>,
) -> Result<ResultPageView, String> {
    store.get_page(&result_id, offset)
}

#[tauri::command]
pub fn release_result(
    result_id: String,
    store: tauri::State<'_, Arc<ResultStore>>,
) -> Result<(), String> {
    store.release(&result_id)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    struct FakeResultsEngine {
        fetches: AtomicU32,
        released: std::sync::Mutex<Vec<String>>,
    }

    impl FakeResultsEngine {
        fn new() -> Self {
            Self {
                fetches: AtomicU32::new(0),
                released: std::sync::Mutex::new(Vec::new()),
            }
        }

        fn fetch_count(&self) -> u32 {
            self.fetches.load(Ordering::SeqCst)
        }
    }

    impl ResultsEngine for FakeResultsEngine {
        fn get_page(
            &self,
            result_id: &str,
            offset: u64,
            _max_rows: u32,
        ) -> Result<serde_json::Value, String> {
            self.fetches.fetch_add(1, Ordering::SeqCst);
            Ok(page_payload_with_id(result_id, offset))
        }

        fn release(&self, result_id: &str) -> Result<(), String> {
            self.released.lock().unwrap().push(result_id.to_string());
            Ok(())
        }
    }

    fn page_payload_with_id(result_id: &str, offset: u64) -> serde_json::Value {
        serde_json::json!({
            "resultId": result_id,
            "offset": offset,
            "rowTotal": 5000,
            "rowTotalExact": true,
            "columns": [],
            "rows": [],
            "truncatedCells": [],
        })
    }

    #[test]
    fn caches_pages_and_deduplicates_aligned_requests() {
        let engine = Arc::new(FakeResultsEngine::new());
        let store = ResultStore::new(engine.clone());

        let first = store.get_page("r1", 0).unwrap();
        assert!(!first.cached);
        let second = store.get_page("r1", 0).unwrap();
        assert!(second.cached);
        assert_eq!(engine.fetch_count(), 1);

        // Unaligned offsets align down to the page boundary.
        let aligned = store.get_page("r1", 123).unwrap();
        assert_eq!(aligned.offset, 0);
        assert!(aligned.cached);
        assert_eq!(engine.fetch_count(), 1);
    }

    #[test]
    fn cache_is_bounded_across_results_and_results() {
        let engine = Arc::new(FakeResultsEngine::new());
        let store = ResultStore::new(engine.clone());

        // Fill well beyond the total budget: results r1..r9 x pages 0..2.
        for result in 1..=9u32 {
            for page in 0..3u64 {
                let id = format!("r{result}");
                store.get_page(&id, page * 500).unwrap();
            }
        }
        assert!(engine.fetch_count() >= 27);
        let cached = store.cache.lock().unwrap().order.len();
        assert!(cached <= 12, "cache grew beyond its budget: {cached}");
    }

    #[test]
    fn release_evicts_cached_pages_and_reaches_the_engine() {
        let engine = Arc::new(FakeResultsEngine::new());
        let store = ResultStore::new(engine.clone());

        store.get_page("r1", 0).unwrap();
        store.release("r1").unwrap();
        assert_eq!(engine.released.lock().unwrap().as_slice(), ["r1"]);

        // After release the cache is empty; a new fetch reaches the engine.
        store.get_page("r1", 0).unwrap();
        assert_eq!(engine.fetch_count(), 2);
    }
}

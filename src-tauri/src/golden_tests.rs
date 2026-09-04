use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::Duration,
};

use tarik_engine_protocol::{
    CsvExportOptions, CsvOptions, ExecutionState, ExportFormat, ExportOptions,
    ExportOverwritePolicy, ExportState, ImportOptions, SourceKind as EngineSourceKind,
    SourceRecord as EngineSourceRecord,
};

use crate::{
    engine_manager::EngineManager,
    export::{ExportCoordinator, ExportView},
    metadata::{
        projects::ProjectsRepository,
        queries::{QueriesRepository, SavedQueryDraft},
        sessions::{QuerySessionSnapshot, QueryTabSnapshot, SessionsRepository},
        sources::{SourceKind, SourceRecord, SourceState, SourcesRepository},
        MetadataDb,
    },
    plan::{self, PlanMode},
    projects::ProjectManager,
    query::{ExecutionView, QueryCoordinator},
    results::ResultStore,
    storage::CleanupService,
};

const JOIN_SQL: &str =
    "SELECT m.market, sum(o.amount) AS revenue FROM main.orders o JOIN main.markets m ON o.market_id = m.market_id GROUP BY m.market ORDER BY m.market";

struct GoldenRoot {
    path: PathBuf,
}

impl GoldenRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("tarik-golden-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("create golden root");
        Self { path }
    }

    fn child(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for GoldenRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Services {
    database: MetadataDb,
    engine: Arc<EngineManager>,
    projects: ProjectManager,
    queries: Arc<QueryCoordinator>,
    exports: Arc<ExportCoordinator>,
    results: Arc<ResultStore>,
}

impl Services {
    fn start(root: &GoldenRoot) -> Self {
        let data = root.child("data");
        let cache = root.child("cache");
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&cache).unwrap();
        let database = MetadataDb::open(data.join("tarik.sqlite")).unwrap();
        let engine = Arc::new(EngineManager::new(
            workspace_engine(),
            cache.join("results"),
        ));
        let projects = ProjectManager::new(database.clone(), data.join("projects"), engine.clone());
        let queries = Arc::new(QueryCoordinator::new(engine.clone(), database.clone()));
        let cleanup = Arc::new(CleanupService::new(cache));
        let exports = Arc::new(
            ExportCoordinator::new(engine.clone(), database.clone()).with_cleanup(cleanup),
        );
        let results = Arc::new(ResultStore::new(engine.clone()));
        Self {
            database,
            engine,
            projects,
            queries,
            exports,
            results,
        }
    }

    fn close(self) {
        let _ = self.results.release_all();
        let _ = self.projects.close();
        self.engine.shutdown();
        self.database.checkpoint().unwrap();
    }
}

fn workspace_engine() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/debug")
        .join(if cfg!(windows) {
            "tarik-engine-duckdb.exe"
        } else {
            "tarik-engine-duckdb"
        })
}

fn metadata_source(source: &EngineSourceRecord) -> SourceRecord {
    SourceRecord {
        id: source.id.clone(),
        project_id: source.project_id.clone(),
        display_name: source.display_name.clone(),
        kind: match source.kind {
            EngineSourceKind::DuckdbTable => SourceKind::DuckdbTable,
            EngineSourceKind::LinkedParquet => SourceKind::LinkedParquet,
            EngineSourceKind::LinkedCsv => SourceKind::LinkedCsv,
        },
        state: match source.state {
            tarik_engine_protocol::SourceState::Ready => SourceState::Ready,
            tarik_engine_protocol::SourceState::Missing => SourceState::Missing,
            tarik_engine_protocol::SourceState::InvalidSchema => SourceState::InvalidSchema,
        },
        source_path: source.source_path.clone(),
        duckdb_name: source.duckdb_name.clone(),
        options: serde_json::Value::Object(source.options.clone()),
        created_at: source.created_at.clone(),
        updated_at: source.updated_at.clone(),
    }
}

fn wait_query(coordinator: &QueryCoordinator, execution_id: &str) -> ExecutionView {
    for _ in 0..1_000 {
        if let Some(view) = coordinator.status(execution_id) {
            if matches!(
                view.state,
                ExecutionState::Succeeded | ExecutionState::Failed | ExecutionState::Cancelled
            ) {
                return view;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("query {execution_id} did not finish");
}

fn wait_export(coordinator: &ExportCoordinator, export_id: &str) -> ExportView {
    for _ in 0..1_000 {
        if let Some(view) = coordinator.status(export_id) {
            if matches!(
                view.state,
                ExportState::Succeeded | ExportState::Failed | ExportState::Cancelled
            ) {
                return view;
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("export {export_id} did not finish");
}

fn wait_running_query(coordinator: &QueryCoordinator, execution_id: &str) {
    for _ in 0..500 {
        if coordinator
            .status(execution_id)
            .is_some_and(|view| view.state == ExecutionState::Running)
        {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("query {execution_id} never started");
}

fn wait_running_export(coordinator: &ExportCoordinator, export_id: &str) {
    for _ in 0..500 {
        if coordinator
            .status(export_id)
            .is_some_and(|view| view.state == ExportState::Running)
        {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("export {export_id} never started");
}

fn write_parquet_fixture(engine: &EngineManager, path: &Path) {
    let escaped = path.to_string_lossy().replace('\'', "''");
    let sql = format!(
        "COPY (SELECT * FROM (VALUES (1, 'Jakarta'), (2, 'Bandung')) AS markets(market_id, market)) TO '{escaped}' (FORMAT PARQUET)"
    );
    let execution_id = format!("fixture-{}", uuid::Uuid::new_v4());
    engine.execute_query(&execution_id, &sql).unwrap();
    for _ in 0..500 {
        if let Some(status) = engine.query_status(&execution_id).unwrap() {
            if status.state == ExecutionState::Succeeded {
                return;
            }
            if matches!(
                status.state,
                ExecutionState::Failed | ExecutionState::Cancelled
            ) {
                panic!("fixture parquet failed: {:?}", status.error);
            }
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("fixture parquet timed out");
}

fn csv_export_options(output: &Path, base_name: &str, rows_per_part: u64) -> ExportOptions {
    ExportOptions {
        format: ExportFormat::Csv,
        output_directory: output.to_string_lossy().into_owned(),
        base_name: base_name.into(),
        rows_per_part,
        overwrite: ExportOverwritePolicy::FailIfExists,
        csv: Some(CsvExportOptions::default()),
        parquet: None,
    }
}

#[test]
fn golden_workflow_survives_restart_missing_link_and_cancellation() {
    let engine_binary = workspace_engine();
    assert!(
        engine_binary.is_file(),
        "real sidecar missing; run ./scripts/build-engine.sh"
    );
    let root = GoldenRoot::new();
    let fixtures = root.child("fixtures");
    let output = root.child("exports");
    fs::create_dir_all(&fixtures).unwrap();
    fs::create_dir_all(&output).unwrap();
    let orders_csv = fixtures.join("orders.csv");
    let markets_parquet = fixtures.join("markets.parquet");
    let markets_replacement = fixtures.join("markets-replacement.parquet");
    fs::write(
        &orders_csv,
        "order_id,market_id,amount\n1,1,12.5\n2,1,7.5\n3,2,30.0\n",
    )
    .unwrap();

    let first = Services::start(&root);
    let project = first.projects.create("Golden retail").unwrap();
    let imported = first
        .projects
        .import_table(
            orders_csv.clone(),
            ImportOptions {
                table_name: "orders".into(),
                csv: Some(CsvOptions::default()),
                column_overrides: Vec::new(),
            },
        )
        .unwrap();
    SourcesRepository::new(first.database.clone())
        .upsert_source(&metadata_source(&imported))
        .unwrap();

    write_parquet_fixture(&first.engine, &markets_parquet);
    fs::copy(&markets_parquet, &markets_replacement).unwrap();
    let linked = first
        .projects
        .link_parquet(markets_parquet.clone(), "markets".into())
        .unwrap();
    SourcesRepository::new(first.database.clone())
        .upsert_source(&metadata_source(&linked))
        .unwrap();

    let catalog = first.projects.catalog().unwrap();
    assert!(catalog.objects.iter().any(|object| object.name == "orders"));
    assert!(catalog
        .objects
        .iter()
        .any(|object| object.name == "markets"));

    let joined = first
        .queries
        .execute(&project.id, "tab-join", JOIN_SQL)
        .unwrap();
    let joined = wait_query(&first.queries, &joined.execution_id);
    assert_eq!(joined.state, ExecutionState::Succeeded);
    assert_eq!(joined.rows_produced, Some(2));
    let page = first
        .results
        .get_page(joined.result_id.as_deref().unwrap(), 0)
        .unwrap();
    assert_eq!(
        page.rows,
        serde_json::json!([["Bandung", 30.0], ["Jakarta", 20.0]])
    );

    for mode in [PlanMode::Explain, PlanMode::Profile] {
        let plan = plan::capture_and_normalize(&first.engine, JOIN_SQL, mode).unwrap();
        assert!(plan.fallback_reason.is_none());
        assert!(plan.nodes.iter().any(|node| node.operator == "join"));
        assert!(plan.nodes.iter().any(|node| {
            node.native_name.contains("GROUP_BY")
                || node
                    .semantic
                    .as_ref()
                    .is_some_and(|semantic| semantic.title.contains("Sum"))
        }));
    }

    let saved = QueriesRepository::new(first.database.clone())
        .create_saved(&SavedQueryDraft {
            project_id: project.id.clone(),
            folder_id: None,
            name: "Revenue by market".into(),
            sql_text: JOIN_SQL.into(),
            tags: vec!["golden".into(), "revenue".into()],
        })
        .unwrap();
    SessionsRepository::new(first.database.clone())
        .save_snapshot(&QuerySessionSnapshot {
            id: format!("project-{}-main", project.id),
            project_id: project.id.clone(),
            tabs: vec![QueryTabSnapshot {
                id: "tab-join".into(),
                title: "Revenue by market".into(),
                sql_text: JOIN_SQL.into(),
                position: 0,
                is_active: true,
            }],
        })
        .unwrap();

    let export = first
        .exports
        .execute(
            &project.id,
            JOIN_SQL,
            csv_export_options(&output, "revenue", 1),
        )
        .unwrap();
    let export = wait_export(&first.exports, &export.export_id);
    assert_eq!(export.state, ExportState::Succeeded);
    assert_eq!(export.rows_written, 2);
    assert_eq!(export.files_written, 2);
    assert!(output.join("revenue-part-00001.csv").is_file());
    assert!(output.join("revenue-part-00002.csv").is_file());

    let long_query = first
        .queries
        .execute(
            &project.id,
            "tab-cancel",
            "SELECT count(*) FROM range(1000000000000) t(i)",
        )
        .unwrap();
    wait_running_query(&first.queries, &long_query.execution_id);
    first.queries.cancel(&long_query.execution_id).unwrap();
    assert_eq!(
        wait_query(&first.queries, &long_query.execution_id).state,
        ExecutionState::Cancelled
    );

    let cancelled_output = root.child("cancelled-export");
    fs::create_dir_all(&cancelled_output).unwrap();
    let long_export = first
        .exports
        .execute(
            &project.id,
            "SELECT i, repeat('x', 1000) AS payload FROM range(1000000000) t(i)",
            csv_export_options(&cancelled_output, "cancelled", 100_000),
        )
        .unwrap();
    wait_running_export(&first.exports, &long_export.export_id);
    first.exports.cancel(&long_export.export_id).unwrap();
    assert_eq!(
        wait_export(&first.exports, &long_export.export_id).state,
        ExportState::Cancelled
    );
    assert!(!fs::read_dir(&cancelled_output)
        .unwrap()
        .flatten()
        .any(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".tarik-export-")
        }));

    let reuse = first
        .queries
        .execute(&project.id, "tab-reuse", "SELECT 6 * 7 AS answer")
        .unwrap();
    assert_eq!(
        wait_query(&first.queries, &reuse.execution_id).state,
        ExecutionState::Succeeded
    );

    fs::remove_file(&markets_parquet).unwrap();
    first.close();

    let second = Services::start(&root);
    let reopened = second.projects.reopen(&project.id).unwrap();
    assert_eq!(reopened.duckdb_path, project.duckdb_path);
    let stored_sources = SourcesRepository::new(second.database.clone())
        .list_sources(&project.id)
        .unwrap();
    let missing = stored_sources
        .iter()
        .find(|source| source.duckdb_name == "markets")
        .unwrap();
    assert_eq!(missing.state, SourceState::Missing);

    let repaired = second
        .projects
        .repair_link(linked, markets_replacement.clone())
        .unwrap();
    SourcesRepository::new(second.database.clone())
        .upsert_source(&metadata_source(&repaired))
        .unwrap();
    assert_eq!(repaired.state, tarik_engine_protocol::SourceState::Ready);

    let restored_session = SessionsRepository::new(second.database.clone())
        .load(&format!("project-{}-main", project.id))
        .unwrap()
        .unwrap();
    assert_eq!(restored_session.tabs[0].sql_text, JOIN_SQL);
    assert_eq!(
        QueriesRepository::new(second.database.clone())
            .list_saved(&project.id, Some("revenue"))
            .unwrap()[0]
            .id,
        saved.id
    );
    let history = QueriesRepository::new(second.database.clone())
        .list_history(&project.id, None, 20)
        .unwrap();
    assert!(history.iter().any(|entry| entry.sql_text == JOIN_SQL));
    assert!(history
        .iter()
        .any(|entry| entry.status == crate::metadata::queries::ExecutionStatus::Cancelled));
    let stored_export = SourcesRepository::new(second.database.clone())
        .get_export(&export.export_id)
        .unwrap()
        .unwrap();
    assert_eq!(
        stored_export.status,
        crate::metadata::sources::ExportStatus::Succeeded
    );
    assert_eq!(stored_export.files_written, 2);

    let after_restart = second
        .queries
        .execute(&project.id, "tab-restored", JOIN_SQL)
        .unwrap();
    assert_eq!(
        wait_query(&second.queries, &after_restart.execution_id).state,
        ExecutionState::Succeeded
    );
    assert_eq!(
        ProjectsRepository::new(second.database.clone())
            .find(&project.id)
            .unwrap()
            .unwrap()
            .name,
        "Golden retail"
    );
    second.close();
}

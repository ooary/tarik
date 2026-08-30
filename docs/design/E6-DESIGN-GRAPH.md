# E6 Query execution and bounded result browsing

## Design read

Local-first desktop SQL workbench for beginner data engineers, with a calm and precise IDE language. Stable workspace geometry, feedback-only motion, and dense but readable results.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 7`
- Foundation: existing Radix primitives and semantic tokens
- Specialized UI: CodeMirror 6 and `@tanstack/react-virtual`

## PROBLEM

Execute immutable SQL snapshots, cancel queued or running work, and browse arbitrarily large results while keeping engine, desktop, WebView memory, and DOM usage bounded.

```text
X -> DesignGraph<A, E, R>
|              |  |  |
|              |  |  `- R: engine job queue, DuckDB, result cache, SQLite, virtualizer
|              |  `---- E: SQL, interrupt, protocol, disk, stale-result errors
|              `------- A: QuerySnapshot, ExecutionState, ResultInfo, ResultPage
|
`- E6 query/results
```

## SHAPES

- IDs: `ProjectId`, `TabId`, `ExecutionId`, `ResultId`
- Records: `QuerySnapshot`, `ExecutionEvent`, `ExecutionTerminal`, `ResultInfo`, `ResultPage`, `ResultMetrics`, `QueryHistoryEntry`
- Variants:
  - `ExecutionState = queued | running | succeeded | failed | cancelled`
  - `ResultState = loading | ready | exhausted | released | failed`
  - `CellValue = null | boolean | safe-number | string`
- Errors: `EmptySql`, `NoSession`, `QueryRejected`, `SqlError`, `Interrupted`, `ResultMissing`, `ResultReleased`, `PageOutOfRange`, `CacheIo`, `ProtocolError`

`QuerySnapshot` is immutable and contains the exact SQL text plus project, tab, and execution IDs. History always stores that snapshot, not a later editor value.

`ResultPage` is JSON-safe and bounded. Integers outside JavaScript's safe range, decimals, date/time values, binary values, and nested values cross the frontend boundary as strings with formatting driven by column metadata. Null and booleans retain native JSON representations.

## GRAPH

```text
run_editor_snapshot (1)
| R: active tab, QueryCommands
| E: EmptySql -> escape(inline message)
| boundary: editor text -> QuerySnapshot
v
submit_execution (1)
| R: QueryCoordinator, MetadataDb, EngineManager
| E: NoSession/ProtocolError -> escape(failed terminal + history)
| behavior: emit queued state
v
enqueue_engine_job (1)
| R: engine JobRegistry, session clone, result cache root
| E: QueryRejected -> escape(failed terminal)
| scope: execution job @execution
v
execute_stream (N)
| R: DuckDB connection clone, InterruptHandle, Arrow IPC writer
| E: SqlError -> escape(clean temp pages + failed)
| E: Interrupted -> escape(clean temp pages + cancelled)
| E: CacheIo -> escape(clean temp pages + failed)
| behavior: emit running state
| boundary: DuckDB Arrow batches -> typed ResultPage artifacts
v
publish_result (1)
| R: atomic result directory rename, JobRegistry
| E: CacheIo -> escape(failed terminal)
| scope: result cursor + page directory @result
v
persist_terminal_once (1)
| R: QueriesRepository, QueryCoordinator terminal guard
| E: MetadataError -> escape(report execution result + diagnostic error)
| behavior: emit succeeded/failed/cancelled
v
request_visible_page (N)
| R: ResultRegistry, bounded desktop page cache
| E: ResultMissing/Released/PageOutOfRange -> escape(result error state)
| boundary: page artifact -> bounded JSON ResultPage
v
virtualize_rows_and_columns (N)
| R: TanStack Virtual, viewport dimensions
| E: render defect -> die(ErrorBoundary)
| behavior: only visible rows/columns + overscan enter DOM

cancel_execution (1)
| R: JobRegistry, InterruptHandle
| E: already terminal -> escape(return existing terminal state)
| E: repeated cancel -> escape(idempotent cancelled/cancelling state)
| queued job -> remove from queue -> cancelled
`- running job -> interrupt -> execute_stream joins cancelled path

release_result (1)
| R: ResultRegistry, filesystem
| E: already released/missing -> escape(idempotent success)
`- mark released -> evict pages -> remove result directory
```

## CARDINALITY

`run_editor_snapshot (1)`; `submit_execution (1)`; `enqueue_engine_job (1)`; `execute_stream (N batches)`; `publish_result (1)`; `persist_terminal_once (1)`; `request_visible_page (N)`; `virtualize_rows_and_columns (N viewport changes)`; `cancel_execution (1 per request, idempotent)`; `release_result (1 per lifecycle, idempotent)`.

## BOUNDARIES

- CodeMirror editor text -> non-empty immutable `QuerySnapshot`
- Tauri invocation payload -> typed project/tab/execution IDs and SQL snapshot
- JSON engine frame -> typed protocol request/response and structured errors
- DuckDB result batches -> typed columns plus page artifacts
- Page artifact -> bounded `ResultPage` values
- Bounded page -> formatted visible cells

No complete result crosses engine stdio or Tauri IPC. No automatic `COUNT(*)` query is issued.

## BEHAVIOR

- Cancellation wraps execution without changing the streaming happy path.
- Typed lifecycle emission wraps job state transitions.
- A bounded LRU wraps page reads.
- Memory and cache metrics wrap result registration/page access.
- SQL and protocol errors are translated at engine, desktop, and UI joins; implementation errors do not leak across layers.
- Polling/backoff may observe engine job states, but job execution is not retried automatically.

## SCOPE

- DuckDB query connection clone: acquire at job start -> drop after terminal state
- DuckDB interrupt handle: acquire at job start -> remove after terminal state
- Temporary page directory: acquire before first batch -> atomic publish on success or delete on error/cancel
- Result directory: acquire at publish -> delete on release, superseding run, project close, app exit, or startup cleanup
- Desktop page cache: acquire on first page -> bounded eviction -> clear on release/project close
- Frontend page cache: acquire on page response -> maximum three pages -> clear on release/project change/unmount
- Poll timer/listener: acquire while running -> clear on every terminal state and unmount

## BOUNDEDNESS CONTRACT

Initial limits are constants with tests, then may become preferences later:

| Layer                    | Limit                                                                         |
| ------------------------ | ----------------------------------------------------------------------------- |
| Engine page              | maximum 500 rows, target maximum 4 MiB per page                               |
| Display cell             | maximum 64 KiB transferred to WebView; larger values show a truncation marker |
| Desktop decoded-page LRU | maximum 3 pages per active result                                             |
| Frontend page LRU        | maximum 3 pages per active result                                             |
| Grid DOM                 | visible rows and columns plus small overscan only                             |
| Active query             | one running job per project session; later submissions queue                  |

The engine streams DuckDB Arrow batches into page artifacts and never collects all batches in a `Vec`. Exact result row count is accumulated while streaming, not computed with a second query.

## CONCURRENCY DECISION

The current sidecar dispatch loop and desktop `EngineProcess` are synchronous. Blocking `query.execute` on either would prevent cancellation. E6 therefore uses asynchronous engine jobs:

1. `query.execute` validates and enqueues, then returns immediately.
2. An engine worker owns a cloned DuckDB connection and writes result pages.
3. `query.status` is a short request used by the desktop coordinator.
4. `query.cancel` remains serviceable while the worker runs and calls DuckDB's thread-safe `InterruptHandle`.
5. `result.get_page` and `result.release` are short requests against the result registry.

This keeps newline-JSON framing deterministic without inventing unsolicited/multiplexed responses in protocol v1.

## TEST LAYERS

Same graph, substituted requirements:

- Protocol serde fixtures and forward-compatible fields
- Temporary result directories and deterministic batch fixtures
- Fake engine jobs/clock for lifecycle and exactly-once history tests
- Real DuckDB sidecar integration for success, SQL failure, queueing, active interrupt, post-cancel reuse, paging, and release
- Mock Tauri commands for frontend loading/error/cancel/released states
- TanStack Virtual tests for bounded render count and keyboard movement
- Large generated DuckDB `range(...)` fixture for memory/DOM review without checking a large file into Git

## IMPLEMENTATION ORDER

1. **E6-T1**: protocol execution shapes; asynchronous engine job registry; desktop coordinator; immutable snapshots; terminal history exactly once; toolbar wiring.
2. **E6-T2**: queued/running cancellation; DuckDB interrupt; idempotent cleanup; session reuse tests.
3. **E6-T3**: Arrow IPC page writer/reader in the engine; result registry; bounded desktop/frontend page APIs; release command.
4. **E6-T4**: install `@tanstack/react-virtual`; replace static fixture with virtualized row and column grid; formatting/copy/keyboard states.
5. **E6-T5**: lifecycle metrics, automatic release paths, startup cleanup, stress test, manual memory review packet.

Each task receives its own Conventional Commit and updates `TASK.md` in that commit. E6 stops at `REVIEW` after T5 for manual large-result and memory sign-off.

## UI STATES

- Empty: "Run a query to see results."
- Queued: queued state plus enabled Cancel action
- Running: elapsed time plus enabled Cancel action
- Succeeded with rows: virtualized grid and result metrics
- Succeeded without a row set: concise completion message
- Failed: structured code/message and line/column when provided
- Cancelled: explicit cancelled state; Run remains available
- Released: "Result released. Run the query again to browse it."
- Page loading/error: localized to the result surface, never replaces editor content

The fake `24,318` count and static sample rows currently in `QueryWorkspace` are removed in E6-T1/T4. Flow/Profile remain disabled or explanatory until E7.

## VERDICT

The intended graph is viable, but the current code does not yet match it in three important places: engine requests are synchronous, `arrow-page-format` currently contains only protocol cursor metadata rather than a concrete page writer/reader, and the result grid is a static fixture without virtualization. E6-T1 through T5 explicitly close those gaps. The happy path and failure joins are separable if jobs and results are registries with scoped cleanup, and cancellation must be implemented before page/grid work is considered complete.

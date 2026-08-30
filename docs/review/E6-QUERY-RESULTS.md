# E6 Query execution and bounded result browsing — review packet

## Outcome

Queries run as asynchronous engine jobs with a typed lifecycle (queued → running →
succeeded/failed/cancelled), exactly one durable history entry per terminal state,
real cancellation through DuckDB interrupts, bounded Arrow IPC result pages, a
virtualized result grid whose DOM and memory stay bounded for any result size, and
lifecycle cleanup that releases superseded results, project-close results, and
stale startup artifacts.

## Commits (in order)

| Commit    | Task  | Summary                                          |
| --------- | ----- | ------------------------------------------------ |
| `a7ae393` | prep  | `docs(query): prepare E6 execution design`       |
| `09fd646` | E6-T1 | `feat(query): add typed execution lifecycle`     |
| `441cb1f` | E6-T2 | `feat(query): support safe cancellation`         |
| `877ea92` | E6-T3 | `feat(results): add bounded result paging`       |
| `5d32db6` | E6-T4 | `feat(results): add virtualized data grid`       |
| `457dd25` | E6-T5 | `fix(results): enforce bounded result lifecycle` |
| `dcb1a76` | E6-T5 | `fix(editor): prefer const in statement counter` |

Design reference: `docs/design/E6-DESIGN-GRAPH.md`.

## Changed paths

- `crates/engine-protocol/src/lib.rs` — `ExecutionState`, `ExecutionStatus` (+ result metadata)
- `engines/duckdb/src/jobs.rs` — async job registry, streaming worker, page writer wiring
- `engines/duckdb/src/pages.rs` — Arrow IPC page writer/reader, JSON-safe cell conversion
- `engines/duckdb/src/sql.rs` — top-level statement splitter
- `engines/duckdb/src/error.rs`, `engines/duckdb/src/main.rs` — lifecycle + result endpoints
- `engines/duckdb/tests/engine_protocol.rs` — 9 integration tests
- `src-tauri/src/query/` — coordinator, `EngineExecutor` trait, Tauri commands, tests
- `src-tauri/src/results/mod.rs` — bounded decoded-page LRU + release commands, tests
- `src-tauri/src/engine_manager.rs` — execute/status/cancel/page/release requests, cache root
- `src/features/results/ResultGrid.tsx` — virtualized grid
- `src/features/results/useQueryExecution.ts` — per-tab lifecycle hook with superseded release
- `src/features/editor/QueryWorkspace.tsx` — Run/Cancel wiring, real result states
- `src/lib/commands.ts`, `src/App.tsx`, `tests/setup.ts`, `package.json`

## Automated checks (all passing)

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` — 62 Rust tests (desktop 35 incl. coordinator + results;
  protocol 4; client 3; page-format 1; engine 12 incl. paging/cancel/stress)
- `npm run lint`, `npm run typecheck`, `npm test` (6 node tests)
- `npm run test:ui` — 48 vitest tests
- `npm run build`

## What changed for the user

1. **Run query** (button or Ctrl+Enter) submits the full editor text as an immutable
   snapshot. Multi-statement text runs sequentially; the last row set is the result.
2. The results panel shows real states: queued → running (rows produced so far) →
   succeeded/failed/cancelled, with structured `code + message` on failure.
3. **Cancel** appears while queued/running. Queued cancels take effect immediately;
   running queries are interrupted by DuckDB and the session stays usable.
4. Successful row queries open a **virtualized grid**: page navigation
   (Previous/Next, PageUp/PageDown), row count window, `Page x of y`, column types
   in the header, NULL markers, truncation ellipsis, arrow-key row navigation, and
   Copy page / double-click row copy.
5. Every terminal execution (success, failure, cancel) lands in `query_history`
   with duration, row count, and error details — visible after restart.

## Manual walkthrough for review

1. `./scripts/build-engine.sh && npm run tauri dev`, open the sample project
   (`docs/sample-data/orders.parquet` is linked, or import a CSV).
2. Paste `SELECT i, 'label-' || (i % 7) AS label FROM range(1, 25000) t(i);` and run:
   - status shows Running with a live row counter, then the grid opens;
   - page through with Next page (50 pages); DOM and app memory stay flat
     (check the process in a task manager while paging);
   - grid header shows column names and Arrow types.
3. Paste `SELECT FROM WHERE` and run: structured `duckdb.error` message appears;
   the history table gains a failed row.
4. Paste `SELECT count(*) FROM range(1000000000000) t(i);` and run, then press
   **Cancel** after a second: state becomes Cancelled, and the next query runs fine
   on the same session.
5. Run `CREATE TABLE demo (a INTEGER); INSERT INTO demo VALUES (1), (2), (3);`:
   completion message shows `3 rows affected`.
6. Close the project and reopen: no stale directories under
   `~/.cache/com.tarik.desktop/results` (Linux path; see app directories command).
7. Restart Tarik and run a query: the new run works; old result artifacts were
   cleaned at startup.

## Known limitations (recorded in TASK.md)

- Engine `rowsAffected` for DML comes from DuckDB count results; non-count DML
  reports no affected-rows value.
- Only the last row-returning statement of a snapshot is browsable; earlier row
  sets are not kept.
- Result artifacts are session-scoped; closing the app discards them (by design).
- Engine crash recovery beyond cancel is E10/E11 scope.

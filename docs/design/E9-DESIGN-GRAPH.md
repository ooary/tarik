# E9 Streaming chunked exports

## Design read

Tarik exports the immutable SQL snapshot from the active editor tab to exact-size CSV or Parquet parts. Export is a long-running engine operation, not a result-grid download: DuckDB Arrow batches stay in the engine sidecar, desktop IPC carries only typed options and bounded progress, and no full result is collected in memory.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 1`
- `VISUAL_DENSITY: 8`
- Foundation: existing semantic tokens, Radix Dialog, Phosphor icons
- Storage: user-selected output directory for completed parts; hidden per-export staging files for incomplete parts; SQLite for export history

## PROBLEM

Export one immutable SQL execution into exact-row CSV or Parquet parts with bounded memory, visible progress, cancellation, and truthful partial-failure cleanup.

```text
X → DesignGraph<A, E, R>
│              │   │  │  │
│              │   │  │  └─ R: active project/session, DuckDB Arrow stream, filesystem, clock, metadata
│              │   │  └──── E: invalid options, SQL failure, collision, write/close/publish failure, cancellation
│              │   └─────── A: ExportOptions, RecordBatch slices, ExportProgress, ExportPartSummary
│              │
│              └─ nodes = functions, edges = data flow
│
└─ E9 streaming chunked exports
```

## SHAPES

- IDs: `ExportId`, `ProjectId`, `SessionId`, `PartNumber`
- Records: `ExportRequest`, `ValidatedExportOptions`, `CsvExportOptions`, `ParquetExportOptions`, `ExportProgress`, `ExportPartSummary`, `ExportStatus`, `ExportHistoryRecord`
- Variants:
  - `ExportFormat = csv | parquet`
  - `OverwritePolicy = fail_if_exists | replace`
  - `ParquetCompression = uncompressed | snappy | gzip | zstd`
  - `ExportState = queued | running | succeeded | failed | cancelled`
  - `Writer = CsvPartWriter | ParquetPartWriter`
- Errors: `InvalidExportOptions`, `ExportExists`, `ExportMissing`, `SqlExecutionFailed`, `OutputCollision`, `PartWriteFailed`, `PartCloseFailed`, `PartPublishFailed`, `ExportCancelled`, `MetadataWriteFailed`

## GRAPH

```text
open_export_dialog (1)
│ R: active project + active editor SQL
│ E: no project/blank SQL ↯escape(disabled action)
└─> choose_output_directory (1)
    │ R: native folder picker
    │ E: user cancel ↯escape(dialog unchanged)
    │ 🔒 selected path → candidate output directory
    └─> validate_export_request (1)
        │ R: filesystem metadata
        │ E: invalid field/path/unsupported option ↯escape(inline field error)
        │ 🔒 form values → ValidatedExportOptions
        └─> enqueue_export (1)
            │ R: active engine session, ExportRegistry, MetadataDb
            │ E: duplicate ExportId ☠die · enqueue failure ↯escape(error state)
            └─> worker_claim (N)
                │ R: per-session export queue + cloned DuckDB connection
                ├─> execute_sql_once (1) @export-job
                │   │ R: DuckDB connection + immutable SQL snapshot
                │   │ E: SQL/Arrow fetch failure ↯escape(failed terminal)
                │   │ E: interrupt ↯escape(cancelled terminal)
                │   │ 🔒 DuckDB Arrow output → schema + RecordBatch stream
                │   └─> split_batch_at_remaining_capacity (N)
                │       │ R: rowsPerPart + current part row count
                │       └─> open_staging_part_if_needed (N) @part
                │           │ R: output directory + generated safe filename
                │           │ E: target collision ↯escape(failed terminal, keep completed parts)
                │           │ E: create/open failure ↯escape(failed terminal)
                │           └─> write_batch_slice (N)
                │               │ R: CSV/Parquet writer
                │               │ E: write failure ↯escape(remove current stage)
                │               └─> close_part_at_exact_boundary (N)
                │                   │ E: flush/close failure ↯escape(remove current stage)
                │                   └─> publish_staged_part (N)
                │                       │ R: filesystem rename + overwrite policy
                │                       │ E: publish failure ↯escape(remove current stage)
                │                       └─> record_completed_part (N)
                │                           └─> update_progress (T)
                │                               R: monotonic clock + status registry
                └─> finish_final_partial_part (1)
                    │ E: close/publish failure ↯escape(remove current stage)
                    └─> mark_terminal (1)
                        │ R: status registry + MetadataDb
                        │ E: metadata write failure ↯escape(report persistence error without invalidating files)
                        └─> completion_summary (1)
                            R: bounded completed part summaries

poll_export_status (T)
│ R: ExportRegistry
└─> ExportProgress

cancel_export (1)
│ R: ExportRegistry + DuckDB InterruptHandle
├─ queued → remove from queue → cancelled
└─ running → request cancel + interrupt → writer closes/removes current stage → cancelled
```

## CARDINALITY

`open_export_dialog (1)` · `choose_output_directory (1)` · `validate_export_request (1)` · `enqueue_export (1)` · `worker_claim (N jobs)` · `execute_sql_once (1 per export)` · `split_batch_at_remaining_capacity (N slices)` · `open/write/close/publish/record part (N parts)` · `update_progress (T latest snapshot)` · `finish_final_partial_part (1)` · `mark_terminal (1)` · `poll_export_status (T)` · `cancel_export (1)` · `completion_summary (1)`.

## BOUNDARIES

- Dialog values are untrusted until one validator produces `ValidatedExportOptions`; engine validates the same wire shape again because IPC is not trusted.
- Output directory must be an existing absolute directory. It is canonicalized once before execution; generated part paths never accept user path separators.
- Base name is trimmed and restricted to portable filename characters (`A-Z`, `a-z`, `0-9`, `_`, `-`), cannot be `.`/`..`, and has a bounded length.
- Rows per part is positive and bounded to the signed SQLite/protocol range.
- CSV delimiter is exactly one non-NUL ASCII byte; header is explicit.
- Parquet compression is a closed enum; format-specific options for the other format are rejected.
- SQL is an immutable non-empty snapshot. It crosses desktop → engine once and is never reconstructed from result pages.
- DuckDB Arrow schemas/batches are trusted only after the engine adapter obtains them; Arrow data never crosses Tauri IPC.
- Part filenames are generated as `<base>-part-<five-or-more-digit sequence>.<csv|parquet>`; sequence overflow is checked.
- Filesystem and metadata errors are translated at their owning layers; raw implementation errors do not reach the UI contract.

## BEHAVIOR

- Export executes SQL exactly once and consumes its Arrow record batches in one pass. It never issues `COUNT(*)`, `LIMIT`, or `OFFSET` queries.
- A batch crossing a part boundary is sliced without copying the full result. Every non-final part contains exactly `rowsPerPart`; the final part contains the remainder.
- Zero result rows produce zero completed parts. CSV headers appear once in every generated CSV part when enabled.
- A hidden staging file is opened with create-new semantics for the current part. It becomes a visible completed part only after writer close succeeds.
- `fail_if_exists` never changes an existing destination. `replace` leaves an existing destination untouched until the replacement staging file is complete, then publishes the completed replacement.
- Completed parts remain valid after cancellation or failure. The incomplete current stage is structurally closed and removed. Status reports completed parts and the failure/cancellation truthfully.
- Progress is monotonic: rows/files/bytes written never decrease, elapsed time uses a monotonic clock, and current part is bounded metadata—not row data.
- Export status polling and cancellation remain serviceable while DuckDB executes because work runs off the protocol loop.
- Export history records the immutable options, terminal state, completed parts, and structured error once. Failure to persist history does not delete valid user files.

## SCOPE

- Engine session: acquire at project open → release at project close; close requests cancellation for queued/running exports.
- Export worker: acquire at `worker_claim` → terminal status and queue release at `mark_terminal`.
- DuckDB statement/Arrow iterator: acquire at `execute_sql_once` → drop on success/error/interrupt.
- Interrupt handle: install at worker start → clear/drop at terminal state.
- Current part writer/file: acquire at `open_staging_part_if_needed` → close at exact boundary/final remainder; on any unwind/error, drop then remove stage.
- Staging path: create at part open → rename on successful publish or remove on failure/cancel.
- Dialog polling timer: acquire while export is non-terminal → clear on terminal/close/unmount.
- Metadata connection/transaction: existing mutex/transaction guard scoped to one terminal history write.

## TEST LAYERS

- Pure validator tests with temporary existing/missing paths cover format, base name, delimiter, rows, compression, and filename sequencing without executing SQL or creating files.
- Writer tests provide synthetic Arrow batches of sizes zero, exact boundary, boundary + 1, many batches, and one batch larger than a part; same graph, temporary filesystem layer.
- CSV readback verifies row order, header per part, delimiter, and exact counts. Parquet readback verifies schema, compression-compatible files, row order, and exact counts.
- Protocol integration tests provide a temporary DuckDB session and output directory, poll bounded status, prove one SQL execution, and verify no `LIMIT/OFFSET` behavior.
- Failure tests replace filesystem/writer requirements with create, write, close, publish, permission, and simulated disk-full failures; completed parts survive and current stage is absent.
- Cancellation tests cover queued and active work, idempotent repeat cancellation, clean session reuse, and bounded terminal retention.
- Desktop coordinator tests provide fake engine + in-memory SQLite to prove exactly-once terminal history and project ownership checks.
- React tests provide mocked typed commands/folder picker/reveal action for validation, progress, cancellation, success, zero rows, and partial failure.

## IMPLEMENTATION ORDER

1. **E9-T1:** adapter-neutral export option/status shapes, pure validation, path/filename rules, and filename sequence tests.
2. **E9-T2:** sidecar `ExportRegistry`, one-pass Arrow batch splitter, CSV/Parquet staged part writers, exact-row/readback tests, and desktop engine bridge.
3. **E9-T3:** status/progress/cancellation, structural stage cleanup, terminal metadata persistence, and injected failure tests.
4. **E9-T4:** compact export dialog, native directory selection, non-blocking progress, cancellation, completion/partial-failure summary, and reveal action.
5. Stop in `REVIEW` with a manual CSV/Parquet, cancellation, collision, restart, and partial-file checklist.

## UI STATES

- Disabled trigger: no active project or active SQL is blank.
- Options: format, output folder, portable base name, rows per part, overwrite policy, and format-specific CSV/Parquet options.
- Inline invalid: field-specific correction before enqueue; no query or file operation has started.
- Queued/running: immutable SQL summary, current part, rows/files/bytes, elapsed time, and one Cancel action.
- Cancelling: action disabled with explicit cleanup status.
- Succeeded with rows: bounded completed-parts summary and Reveal output folder.
- Succeeded with zero rows: explicit “Query returned no rows; no files were created.”
- Failed before first part: structured error and safe retry with prior options.
- Failed after completed parts: warning states that completed files remain valid and lists them; incomplete stage is absent.
- Cancelled: same completed-part disclosure, with no success styling.

## VERDICT

The current query paging pipeline cannot implement E9 safely because page artifacts add an unnecessary disk round-trip and desktop JSON would violate bounded IPC. The export graph therefore belongs in the engine sidecar beside DuckDB’s Arrow iterator, with independent typed status exposed through the existing request protocol. Existing `export_history` metadata is a useful base but needs lifecycle ownership and richer option/error persistence. Implementation is valid only if SQL execution is one-shot, batch slicing creates exact parts, every current writer/stage has structural cleanup, and desktop code never receives exported rows.

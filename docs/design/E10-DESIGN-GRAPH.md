# E10 diagnostics, recovery, cleanup, and shutdown design graph

PROBLEM: Make Tarik failures diagnosable and recovery safe while bounding owned temporary state and closing drafts, jobs, databases, and logs in an explicit order.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: filesystem, clock, incident IDs, Tauri lifecycle, coordinators
│ │ │ └──── E: log I/O, malformed artifacts, unsafe paths, draft flush, job timeout
│ │ └─────── A: structured events, incident reports, cleanup summaries, shutdown report
│ │
│ └─ nodes = functions, edges = data flow
│
└─ the problem: bounded support diagnostics and deterministic local recovery

## SHAPES

- `IncidentId`: UUID generated in the process that first observes a backend panic, command failure, or frontend render failure.
- `OperationId`: UUID for one logged operation; unrelated to SQL text or result content.
- `LogLevel`: `info | warning | error`.
- `LogEvent`: timestamp, level, target, event name, optional operation/project/incident ID, optional duration, stable error code, and bounded safe message.
- `LogPolicy`: 2 MiB active-file limit and seven total `tarik.log*` files.
- `SupportIncident`: incident ID, friendly summary, log directory, and whether logging succeeded.
- `CleanupPolicy`: 24-hour result age, 512 MiB result-root budget, and exact abandoned-export manifests.
- `ExportCleanupManifest`: export ID, absolute output directory, validated base name/format, and creation time. It never contains SQL.
- `CleanupSummary`: exact files/directories/bytes removed, backups restored, warnings, and bounded warning details.
- `ShutdownPhase`: `idle | draft_flush | cancelling_jobs | releasing_results | closing_engine | checkpointing_metadata | flushing_logs | complete | failed`.
- `ShutdownReport`: terminal phase, cancelled query/export counts, released-result count, checkpoint status, and bounded warnings.
- Errors: `LogIo`, `InvalidLogEvent`, `IncidentUnavailable`, `UnsafeCleanupPath`, `MalformedManifest`, `CleanupIo`, `DraftFlushFailed`, `ShutdownTimeout`, `MetadataCheckpointFailed`.

## GRAPH

### A. Structured logging and support incidents

```text
resolve_directories (1)
│  R: Tauri PathResolver
│  E: path resolution/create failure ☠die(setup error before application state)
│  🔒 OS path result → AppDirectories
↓
open_logger(LogPolicy) (1)
│  R: log directory, filesystem, clock
│  E: LogIo ↯escape(stderr-only logger; app remains usable)
│  🔒 existing tarik.log metadata → bounded rotation decision
↓
record(LogEvent) (N)
│  R: logger, clock
│  E: LogIo ↯escape(mark logger degraded + stderr; never fail user operation)
│  🔒 caller fields → typed/bounded/redacted event
↓
rotate_if_needed (T)
│  R: logger lock, filesystem, LogPolicy
│  E: LogIo ↯escape(keep active file or stderr-only; never delete outside log_dir)
↓
flush (1)
   R: logger
   E: LogIo ↯escape(shutdown warning)
```

```text
observe_failure (N)
│  R: panic hook or frontend boundary, incident IDs
│  E: poisoned/default hook ☠die only after best-effort stderr
↓
create_incident (1)
│  R: logger, log directory
│  E: logger unavailable ↯escape(friendly incident with loggingSucceeded=false)
↓
record(error event without SQL/result rows) (1)
│  R: logger
│  E: LogIo ↯escape(friendly support surface still renders)
↓
present_support_surface (1)
   R: React error boundary or typed command response
   E: clipboard/reveal failure ↯escape(copy path or keep instructions visible)
```

Logging is a behavior layer, not a business-data sink. Info events may contain stable operation names, IDs, durations, and aggregate counters. They may not contain SQL text, bound values, result cells, CSV previews, or export payload rows. Error messages are normalized and bounded before persistence.

### B. Owned-cache and abandoned-export cleanup

```text
scan_owned_result_root(policy) (1)
│  R: cache root, filesystem, clock
│  E: missing root ↯escape(empty summary)
│     unreadable entry ↯escape(warning + continue)
│  🔒 directory entries → regular owned result artifacts
↓
remove_expired_results (N)
│  R: canonical result root, filesystem
│  E: symlink/non-child ↯escape(skip + warning)
│     remove failure ↯escape(warning + continue)
↓
enforce_result_size_budget (N)
│  R: metadata sorted oldest first, 512 MiB policy
│  E: metadata race ↯escape(skip + warning)
↓
read_export_manifests (N)
│  R: cache/export-staging root, JSON parser
│  E: malformed/untrusted manifest ↯escape(remove manifest only + warning)
│  🔒 JSON file → ExportCleanupManifest
↓
reconcile_exact_export_artifacts (N)
│  R: manifest, filesystem
│  E: relative/unsafe/mismatched filename ↯escape(skip + warning)
│     missing canonical with backup ↯escape(restore exact backup)
│     canonical present with backup ↯escape(remove exact backup)
│     incomplete exact stage ↯escape(remove exact stage)
↓
remove_manifest_after_reconciliation (N)
   R: manifest path beneath owned manifest root
   E: remove failure ↯escape(warning)
```

Only `<cache>/results` and `<cache>/export-staging` may be recursively removed. User-selected output directories are never recursively cleaned. Reconciliation may touch only regular files whose exact names contain the validated manifest export ID and expected part number. Canonical `<base>-part-NNNNN.<ext>` files are never deleted by cleanup. A backup is restored if publication had not produced its canonical part; otherwise only the redundant hidden backup is removed.

At export submission, `register_export_cleanup` writes a manifest atomically before the engine may create a stage. The sidecar includes the export ID and part number in hidden stage/backup names. Terminal completion removes the manifest only after writer cleanup. A process crash therefore leaves enough bounded metadata for the next startup to reconcile exact files without searching arbitrary user directories.

### C. Graceful shutdown

```text
register_frontend_shutdown_ready (1)
│  R: mounted App + QueryWorkspace flush handle
│  E: frontend unavailable ↯escape(do not intercept native close)
↓
native_close_requested (1)
│  R: Tauri window event, ShutdownCoordinator
│  E: already shutting down ↯escape(ignore duplicate)
↓
emit_shutdown_requested + prevent_close (1)
│  R: Tauri emitter
│  E: emit failure ↯escape(run backend shutdown directly)
↓
flush_latest_draft_and_preferences (1)
│  R: latest tab refs, session repository, preference repository
│  E: DraftFlushFailed ↯escape(keep window open, show retry/quit-without-latest-draft choices)
↓
complete_shutdown (1)
│  R: ShutdownCoordinator
│  E: duplicate request ↯escape(return current/terminal report)
↓
cancel_queries_and_exports (N)
│  R: QueryCoordinator, ExportCoordinator
│  E: engine cancellation failure ↯escape(warning + continue)
↓
wait_for_terminal_history (T)
│  R: coordinators, monotonic clock, 2 second bound
│  E: ShutdownTimeout ↯escape(continue; sidecar session close interrupts remaining jobs)
↓
release_result_artifacts (N)
│  R: ResultStore
│  E: release failure ↯escape(warning + startup cleanup owns remainder)
↓
close_project_session_then_engine (1)
│  R: ProjectManager, EngineManager
│  E: sidecar failure ↯escape(force child shutdown + warning)
↓
checkpoint_metadata (1)
│  R: MetadataDb
│  E: MetadataCheckpointFailed ↯escape(warning; SQLite WAL remains recoverable)
↓
flush_logger (1)
│  R: logger
│  E: LogIo ↯escape(stderr warning)
↓
close_window (1)
   R: Tauri window/app handle
   E: close failure ↯escape(app exit)
```

Policy: Tarik cancels queued/running queries and exports on application shutdown, waits at most two seconds for coordinators to observe/persist terminal states, releases ephemeral results, closes the DuckDB session and sidecar, checkpoints SQLite, and flushes logs. Already-published export parts remain valid. The current hidden part is removed by sidecar RAII or the next startup manifest reconciliation.

The frontend must flush the latest immutable tab snapshot before backend teardown. A failed draft flush keeps the window open and offers Retry or Quit without latest changes; Tarik never silently claims the draft was saved. If the frontend never registered readiness (startup/backend failure), native close is not intercepted and the `Destroyed` fallback performs backend cleanup.

## CARDINALITY

- `resolve_directories`, `open_logger`, `create_incident`, `present_support_surface`, `register_frontend_shutdown_ready`, `native_close_requested`, `emit_shutdown_requested`, `flush_latest_draft_and_preferences`, `complete_shutdown`, `close_project_session_then_engine`, `checkpoint_metadata`, `flush`, `close_window`: `(1)`.
- `record`, `observe_failure`, `remove_expired_results`, `enforce_result_size_budget`, `read_export_manifests`, `reconcile_exact_export_artifacts`, `remove_manifest_after_reconciliation`, `cancel_queries_and_exports`, `release_result_artifacts`: `(N)`.
- `rotate_if_needed`, `wait_for_terminal_history`: `(T)`.

## BOUNDARIES

- `🔒` Tauri-resolved application paths become `AppDirectories`; callers cannot submit log/cache roots.
- `🔒` Existing log metadata is accepted only for exact `tarik.log`/numbered sibling names in the resolved log directory.
- `🔒` Every log event enters through typed fields, bounded lengths, and redaction; arbitrary maps are not persisted.
- `🔒` Frontend incident details are bounded and normalized; full application state and SQL are never accepted.
- `🔒` Cache directory entries become owned artifacts only after non-symlink, direct-child checks.
- `🔒` Export manifest JSON is parsed once into validated UUID/absolute directory/base/format/time shapes.
- `🔒` User output entries are touched only after exact filename parsing against one validated manifest.
- `🔒` Shutdown completion accepts no caller-selected paths, process IDs, or operation IDs.

## BEHAVIOR

- `⛈ structured-logging` wraps setup, project lifecycle, query/export lifecycle, cleanup, incidents, and shutdown without changing their success values.
- `⛈ redaction` removes SQL/result-like fields and bounds message/detail lengths before writing.
- `⛈ rotation` wraps writes at the logger boundary and enforces size/retention.
- `⛈ cleanup-warning-aggregation` continues independent cleanup after recoverable file errors and bounds returned details.
- `⛈ shutdown-idempotency` collapses repeated close events onto one shutdown operation.
- No automatic retry wraps filesystem deletion or database close; repeated destructive actions are unsafe. Recovery is explicit and idempotent instead.

## SCOPE

- Logger file acquire@`open_logger` → flush/drop@`ShutdownCoordinator`/process drop.
- Panic hook previous-handler acquire@`install_panic_hook` → process lifetime; hook writes best effort and chains to the previous handler.
- Cleanup directory iterators/files acquire@individual scan/reconcile node → RAII release@same node.
- Export manifest acquire@`register_export_cleanup` → remove@terminal coordinator or startup reconciliation.
- Query/export pollers acquire@submit → terminal or bounded shutdown wait.
- Result files/cache acquire@query result publication → release@result replacement/project close/shutdown/startup cleanup.
- DuckDB session/sidecar acquire@project open → release@project close/shutdown.
- SQLite connection/WAL acquire@app setup → checkpoint@shutdown → process drop.
- Frontend close listener acquire@App mount → unlisten@App unmount.

## TEST LAYERS

- Logger tests use a temporary log directory, fixed clock/event values, tiny byte limits, and inspect JSONL/retention; no Tauri runtime.
- Redaction tests submit SQL-like keys/messages and result-like data and prove secrets/full statements do not persist.
- Panic-hook formatter tests use a capture writer/hook-safe logger and assert incident ID plus friendly surface contract.
- Frontend boundary tests substitute typed incident/reveal/clipboard commands and simulate render rejection and reporting failure.
- Cleanup tests use temporary owned roots, controlled modification times/sizes, symlinks, malformed manifests, exact stages/backups/canonical files, and an outside sentinel.
- Export lifecycle tests substitute a cleanup-manifest registry and prove registration precedes engine submission and terminal removal follows cleanup.
- Shutdown tests substitute draft saver, query/export coordinators, result store, engine, metadata, logger, and clock; the GRAPH/order remains unchanged.
- Tauri command tests verify no cleanup/shutdown/reveal command accepts a filesystem root from the frontend.
- Integration tests reopen SQLite/DuckDB after forced test shutdown and assert no owned temporary file remains locked.

## VERDICT

The existing code partially implements the target graph: it resolves separate data/cache/log directories, has a basic React render boundary, removes result artifacts at startup, uses RAII for current export stages, and kills the sidecar on window destruction. It does not yet have bounded structured file logs, incident IDs, backend panic capture, exact abandoned-export manifests, age/size cleanup, an explicit clear-cache action, draft-first close interception, coordinator-wide cancellation/wait, SQLite checkpointing, or one idempotent shutdown owner. Current startup result cleanup also deletes every entry without age/size reporting, and current export backups cannot be mapped back to canonical parts after a crash. E10-T1 through T4 must implement the graphs above; code is valid only when those mismatches are removed and failure handling remains at the stated joins.

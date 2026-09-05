# E14 beginner data profiling and quality checks design graph

## Design read

Reading this as a dense local SQL workbench for beginner data engineers: profiles are teaching instruments, not dashboards. Every number must say what it measured, how it was measured, and what SQL proves it.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 8`
- Profile/check surfaces are calm dense tables on existing semantic tokens; no dashboard cards, no decorative charts, no gauges
- Teaching rule: every metric and check outcome carries a provenance label (Exact / Approximate / Sampled) and a plain-language meaning; unavailable metrics are absent with a reason, never shown as zero
- SQL rule: generated SQL is always visible, copyable, and openable in a new tab; copying or opening never executes anything; edits in the opened tab never alter the saved definition

PROBLEM: Let beginners answer "What is in this data?", "Can I trust it?", and "What SQL proves it?" through explicit, local, cancellable profiles and reusable read-only quality checks.

X → DesignGraph<A, E, R>
│ │ │ │
│ │ │ └─ R: React feature state, SQLite, typed Tauri commands, sidecar job registry, DuckDB aggregate SQL, bounded result pages
│ │ └──── E: stale catalog, missing links, unsupported types, oversized definitions, mutation attempts, cancellation, engine loss, history double-writes
│ └─────── A: ProfileRequest, ProfileMetric, CheckDefinition, CheckRevision, CheckRun, FailurePreview
│
└─ nodes = functions, edges = data flow

## SHAPES

- IDs: `ProjectId`, `SourceId`, `CatalogObjectId(database,schema,name,kind)`, `CheckId`, `CheckRevisionId`, `RunId`, `ResultId`
- Identity snapshots:
  - `CatalogRevision` — content-derived identity (object name, kind, column names/types) captured at request time; mismatch on return is staleness, not partial success
  - `ProfileRequest(projectId, sessionId, object, selectedColumns ≤100, catalogRevision, mode)`
  - `ProfileMetric(name, typedValue, provenance, absentReason?, observationNote?)`
  - `ProfileSnapshot(object, catalogRevision, metrics, observedAt, durationMs, mode)`
  - `MetricProvenance = exact | approximate | sampled`
- Quality definitions (SQLite, project-scoped):
  - `CheckType = not_empty | not_null | unique | accepted_values | range | relationship | freshness | custom_sql`
  - `NullPolicy = fail_on_null | pass_on_null` (per-type defaults below)
  - `QualityCheckDefinition(checkId, projectId, name, type, target, typedOptions, nullPolicy, severity = info|warning|critical, enabled, createdAt)`
  - `CheckRevision(revisionId, immutable definition copy, supersededAt?)` — every update creates a new revision; runs reference the revision actually executed
  - `CheckRun(runId, checkId, revisionId, outcome, failureCount, durationMs, observedAt, errorCode?)`
  - `CheckOutcome = pass | fail | error | cancelled`
- Failure previews: `FailurePreviewRequest(runId, revisionId)` → existing `ResultId`, 500-row Arrow pages, desktop 12-page LRU; never a full failure collection
- Errors: `CatalogStale`, `SourceMissing`, `SessionMissing`, `EngineUnavailable`, `ProfileInvalid`, `MetricUnsupported`, `CheckInvalid`, `CheckStale`, `SqlMutating`, `HistoryWriteFailed`, `Cancelled` — all surfaced as stable codes; none fabricate partial success

### Metric applicability matrix

Provenance defaults; `—` means the metric is absent with `absentReason`, never zero.

| Metric                     | bool              | numeric                               | text                | date/timestamp      | binary  | list/struct |
| -------------------------- | ----------------- | ------------------------------------- | ------------------- | ------------------- | ------- | ----------- |
| Row count (table-level)    | exact             | exact                                 | exact               | exact               | exact   | exact       |
| NULL count / rate          | exact             | exact                                 | exact               | exact               | exact   | exact       |
| Distinct count             | exact (≤2 values) | approximate default, exact on request | approximate default | approximate default | —       | —           |
| Min / max value            | —                 | exact                                 | — (use length)      | exact               | —       | —           |
| Numeric summary (avg)      | —                 | exact                                 | —                   | —                   | —       | —           |
| Text length min/max/avg    | —                 | —                                     | exact               | —                   | —       | —           |
| Temporal range (min/max)   | —                 | —                                     | —                   | exact               | —       | —           |
| Common values (top 20)     | exact             | exact                                 | exact               | exact               | —       | —           |
| Representative values (20) | sampled           | sampled                               | sampled             | sampled             | sampled | sampled     |

Bounds: ≤100 columns per request, ≤20 common values per column, ≤20 representative values per column; every displayed value passes the existing safe-value truncation policy.

### Check semantics and NULL policy defaults

| Type            | Passes when                               | Default NULL policy | Rationale                                                             |
| --------------- | ----------------------------------------- | ------------------- | --------------------------------------------------------------------- |
| not_empty       | table/view has ≥1 row                     | (no columns)        | table-level fact                                                      |
| not_null        | zero rows have NULL in column             | NULL always fails   | the check is the NULL policy                                          |
| unique          | no value combination occurs >1            | pass_on_null        | matches SQL UNIQUE treating NULLs as distinct; `fail_on_null` offered |
| accepted_values | every value is in the accepted list       | fail_on_null        | a NULL is a value the user did not accept; `pass_on_null` offered     |
| range           | every value within min/max (numeric/date) | pass_on_null        | a NULL is absent, not out of range; `fail_on_null` offered            |
| relationship    | every non-null child key exists in parent | pass_on_null        | optional keys stay optional; `fail_on_null` offered                   |
| freshness       | latest max(col) is within threshold       | pass_on_null        | an all-NULL column is `CheckInvalid`, not a failure                   |
| custom_sql      | statement returns zero failing rows       | (user SQL)          | validated read-only, exactly one statement                            |

Canonical generated SQL shapes (identifiers pass only through the central quoting boundary; options are typed, never concatenated UI text):

```sql
-- not_empty
SELECT count(*) AS row_total FROM "tbl";
-- not_null (failure count; preview mirrors the WHERE)
SELECT count(*) FROM "tbl" WHERE "col" IS NULL;
-- unique
SELECT count(*) FROM (SELECT "k1","k2" FROM "tbl" GROUP BY ALL HAVING count(*) > 1) AS duplicates;
-- accepted_values (fail_on_null shown)
SELECT count(*) FROM "tbl" WHERE "col" IS NULL OR "col" NOT IN (?, ?);
-- range
SELECT count(*) FROM "tbl" WHERE "col" < ? OR "col" > ?;
-- relationship
SELECT count(*) FROM "child" c LEFT JOIN "parent" p ON c."key" = p."key"
 WHERE c."key" IS NOT NULL AND p."key" IS NULL;
-- freshness (value compared against threshold at recorded observation time)
SELECT max("col") AS latest FROM "tbl";
```

Custom SQL: exactly one read-only statement; rejected before execution with `SqlMutating` if it parses as DDL, DML, `COPY`, `ATTACH`, `INSTALL`, or any mutating/external effect, through the existing sidecar validation layer.

## GRAPH

### Profile execution (async, bounded, cancellable)

```text
Explorer Profile action (N) → open workspace, NO scan (1) → user selects columns + mode + reviews cost (N)
│ R: CatalogSnapshot, SourceRecord                │ R: cost disclosure (exact distinct = full scan+hash, approximate = bounded HyperLogLog)
│ E: unsupported object kind ↯escape(action absent)
│ 🔒 live catalog DOM → captured CatalogObjectId + CatalogRevision
                                                            ↓
                                            start profile (1) → typed Tauri command (1)
                                            │ R: project/session identity
                                            ├─ E: no project/session ↯escape(action disabled)
                                            └─ 🔒 request JSON → ProfileRequest
                                                            ↓
                                            sidecar profile job queued in the SAME per-session FIFO registry (1)
                                            │ R: JobRegistry (no new concurrency machinery; cannot starve query/export cancellation)
                                            ├─ E: session missing ↯escape(SessionMissing)
                                            └─ A: job id; desktop polls status (existing pattern)
                                                            ↓
                                            claim job → clone session connection (1) → verify catalog identity (1)
                                            │ R: SessionManager
                                            └─ E: CatalogStale | SourceMissing ↯escape(terminal error, no partial metrics)
                                                            ↓
                                            execute bounded aggregate statement set (N ≤ 1 + columns/25)
                                            │ R: DuckDB connection, typed metric SQL
                                            │ A: single-row aggregates only; rows never cross IPC as data
                                            ├─ E: DuckDB failure ↯escape(terminal error with stable code)
                                            ├─ E: cancel via InterruptHandle ↯escape(CheckOutcome=cancelled, zero residue)
                                            └─ profile cursor/cache state release on success|error|cancel|close|restart|shutdown (T)
                                                            ↓
                                            assemble ProfileSnapshot ≤ ~256 KiB (1)
                                            │ R: metric applicability matrix, truncation policy
                                            ├─ A: every metric carries provenance + observation time
                                            └─ A: unsupported metrics absent with reason, never zero
                                                            ↓
                                            desktop renders dense metric table (T); values copyable, never logged
```

The profile job is one job whose cancellation cancels the whole profile. Metric SQL is compiled per column type from trusted typed options; common values (`GROUP BY` top-20) are exact for the scanned table; representative values (`LIMIT 20` without ordering guarantees) are labeled Sampled.

### Check definition lifecycle (SQLite, migration 0008)

```text
create/update check (N) → validate typed definition (1) → write definition + new CheckRevision (1)
│ R: QualityRepository, project-scoped tables, FK + explicit cascade
│ E: duplicate name | missing/stale target | type-incompatible thresholds | accepted-value limit | key arity/type | freshness unit ↯escape(inline error, input retained)
│ E: SQLite failure ↯escape(command error, nothing persisted)
│ 🔒 form JSON → typed CheckDefinition (bounded sizes: checks/project, accepted-value entries+bytes, composite-key columns, custom-SQL bytes ≤ saved-query SQL bound)
│
└─ runs continue to identify the revision actually executed; updating never rewrites history
```

Runs persist aggregate facts only: definition/revision, terminal outcome, failure count, duration, observation time, stable error code. No common/sample values, no failing-row payloads, no SQL in history rows (custom SQL lives in the definition it belongs to). Retention is bounded per project with a clear-history command that never deletes definitions, projects, sources, or exports.

### Check and suite execution with exactly-once history

```text
Run one check | Run suite (N) → resolve ordered enabled revisions (1)
│ R: QualityRepository                │ R: immutable definition snapshots
│ E: disabled/stale/missing object ↯escape(per-check error row, suite continues)
│ 🔒 definition → compiled visible SQL (1 per check)
│                                                    ↓
│                                    per check: queue job in the SAME per-session FIFO registry (1)
│                                    │ R: JobRegistry; suite coordinator only sequences submissions
│                                    ├─ E: custom SQL mutating ↯escape(SqlMutating, before execution)
│                                    └─ A: single-check runs observe one DuckDB read snapshot; suites disclose per-check observation boundaries — no implied cross-check atomicity
│                                                    ↓
│                                    execute count statement / custom statement (1)
│                                    │ R: cloned connection, parameterized literals
│                                    ├─ E: DuckDB failure ↯escape(outcome=error + stable code)
│                                    ├─ E: cancel ↯escape(outcome=cancelled)
│                                    └─ A: pass/fail decided by exact failing-row count; freshness compares max(col) against threshold at recorded observation time
│                                                    ↓
│                                    finalize exactly-once CheckRun (1)
│                                    │ R: run id, SQLite transaction
│                                    ├─ E: HistoryWriteFailed ↯escape(surfaced; no duplicate rows on retry)
│                                    └─ session stays usable after fail/error/cancel
│                                                    ↓
│                                    failure preview on explicit request only (N)
│                                    │ R: existing ResultId paging (500-row pages, 12-page LRU)
│                                    ├─ E: stale run/revision ↯escape(preview refused)
│                                    └─ A: the suite never collects all failures; preview released by existing result lifecycle
```

### Stale, missing, restart, and shutdown edges

```text
catalog/source change mid-flight (N)
│ ├─ profile: identity mismatch at claim or between statements ↯escape(CatalogStale, no partial metrics)
│ ├─ check: object/column/type missing at claim ↯escape(outcome=error, CheckInvalid, suite continues)
│ └─ linked Parquet missing at scan ↯escape(SourceMissing; repair stays the existing Locate replacement flow)

sidecrash crash / restart (1)
│ └─ jobs die with the process; queued/running checks finalize as terminal rows at desktop recovery (existing engine recovery path); previews lost by design, history rows not duplicated

project close / application shutdown (N)
│ └─ active profile/check jobs cancel through existing shutdown; result cache cleared; no stranded jobs, previews, or duplicate history (T)
```

## CARDINALITY

Profile start (N per user action) · catalog identity capture (1 per request) · job submission (1 per profile/check) · aggregate statements (N, ≤ 1 + ceil(columns/25)) · metrics returned (N ≤ columns × matrix row) · definition writes (N, 1 revision each) · suite submissions (N ordered, 1 job each) · history rows (1 terminal row per started check, exactly once) · failure preview requests (N on demand) · common/representative values (≤20+20 per column) · status polls (T).

## BOUNDARIES

- Live catalog/editor DOM 🔒 captured `CatalogObjectId` + `CatalogRevision`; later changes are staleness, never silent re-targeting.
- Profile/check request JSON 🔒 typed protocol requests validated at Tauri command decoding and again in the sidecar.
- Metric/check options 🔒 typed Rust structs; generated SQL is produced only from trusted typed values plus the central identifier quoting boundary — never from concatenated UI text.
- Custom SQL 🔒 one-statement read-only validation before any execution or suite inclusion; explicit user confirmation required before running custom SQL.
- Definition/history storage 🔒 bounded, project-scoped SQLite rows with FK isolation; malformed/oversized variants rejected before writes; aggregate facts only.
- Result pages 🔒 existing Arrow IPC page boundary (500 rows/page, 12-page desktop LRU, 64 KiB value truncation, NULL markers) — profiling adds no new data channel.
- Logs 🔒 stable IDs/codes/durations only; no generated or custom SQL, no metric values, no accepted values, no failing rows.
- Freshness comparison 🔒 threshold math evaluated in the coordinator against the SQL's returned `max(col)` at a recorded observation time; both SQL and comparison are shown.

## BEHAVIOR

- ⛈ accessibility wraps every workspace: table semantics, focus management, screen-reader labels, keyboard-complete flows, light/dark/system legibility, minimum viewport.
- ⛈ provenance teaching wraps every metric and outcome: Exact/Approximate/Sampled label, plain-language meaning, why approximate can differ, NULL vs empty text, distinct vs unique.
- ⛈ cost disclosure wraps profile start and exact-distinct opt-in; nothing scans implicitly and opening a workspace never executes.
- ⛈ submitting guards and at-most-once semantics wrap definition writes, runs, and history finalization.
- ⛈ cancellation reuses the existing InterruptHandle path; cancelled profiles/checks leave the session usable and zero artifacts.
- ⛈ structured logging (JSONL, rotated, bounded) records operation, stable codes, durations — never SQL or data.

## SCOPE

- Catalog identity acquire@request capture → release@return | stale detection.
- Profile job acquire@start → terminal(success|error|cancelled) → release@all cursor/cache state; re-checked at project close, sidecar restart, shutdown.
- Check definition revisions acquire@first save → supersede@update → retain forever (bounded count per project, deletable).
- CheckRun rows acquire@job start (pending intent) → finalize exactly-once@terminal → release@retention/clear-history (definitions unaffected).
- Failure preview ResultId acquire@explicit preview request → release@existing result lifecycle (supersede, close, clear cache, restart).
- Engine/session scopes are the existing ones; profiling adds no process, connection pool, or worker of its own.

## TEST LAYERS

R = {

- Catalog fixtures: empty tables, all-NULL columns, mixed NULL/type edge cases, quoted/reserved identifiers, boolean/decimal/float-NaN/text-Unicode/long values, date/timestamp, binary, list/struct.
- Linked sources: Parquet present, moved mid-run, repaired via Locate replacement.
- Protocol: typed request/response serde, bounds enforcement (100 columns, 20+20 values, payload size), stale rejection.
- Sidecar: real DuckDB — golden profile metrics on a fixed fixture with expected exact/approximate/sampled labels; check SQL golden cases per type and NULL policy; custom mutation sentinels (`INSERT`, `COPY`, `ATTACH`, `INSTALL` rejected).
- Execution: queued/running/pass/fail/error/cancel transitions, suite order, per-check observation disclosure, exactly-once history (including retry and crash recovery), preview paging/release/residue.
- Metadata: fresh migration 0008, schema-7 upgrade, newer-schema refusal, CRUD/revisions, project cascade, retention, malformed JSON, size limits, transaction rollback, Unicode names.
- Frontend: no-auto-run, empty/loading/progress/partial-unavailable/error/cancel/stale states, provenance labels, profile-to-check prefill without implicit save/run, generated SQL display/copy/open-without-run, keyboard/focus, themes, minimum viewport.

}; same graphs, no test-only execution paths.

## VERDICT

No profiling or quality-check implementation exists yet; this graph is the target. The E14-T1–T7 implementations must be reconstructed against these nodes and re-diffed here: every metric needs provenance and absent-reason semantics, every check needs visible deterministic SQL with an explicit NULL policy, runs need exactly-once terminal history, previews must ride the existing bounded page lifecycle, and no path may mutate user data, hide execution, present approximation as exactness, or grow desktop memory with table size. E13-T6 Windows acceptance remains open in parallel and is not implied by this design.

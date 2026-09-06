# E14 beginner data profiling and quality checks design graph

## Design read

Reading this as a dense local SQL workbench for beginner data engineers. Profiles are teaching instruments, not dashboards. Every number must say what it measured, how it was measured, and what SQL produced it.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 8`
- Use the existing Tarik tokens, controls, typography, focus treatment, and one accent.
- Use column navigation, metric groups, and progressive disclosure. Do not use decorative charts, gauges, dashboard cards, or a flat metric dump.
- Generated profile/check SQL is visible, copyable, and openable without execution. Editing opened SQL never alters the profile snapshot or saved check.
- E14 UI implementation remains unapproved until the user reviews the real Tauri app in light, dark, and minimum-viewport states.

## PROBLEM

Let beginners move through one explicit local data-trust loop: profile data, understand an observation, inspect its SQL evidence, create a reviewed check, run it, inspect bounded failures, repair, and rerun.

X → DesignGraph<A, E, R>
│ │ │ │
│ │ │ └─ R: React workbench state, typed Tauri commands, SQLite, DuckDB sidecar, bounded profile/check registries, result paging
│ │ └──── E: stale object, missing source, unsupported metric, invalid check, unsafe SQL, cancellation, engine loss, history conflict
│ └─────── A: ProfileIntent, ProfileRequest, ProfileSnapshot, ProfileSqlEvidence, CheckRevision, CheckRun, FailurePreview
│
└─ nodes = functions, edges = data flow

## SHAPES

- IDs: `ProjectId`, `CatalogObjectId(database,schema,name,kind)`, `ProfileId`, `CheckId`, `CheckRevisionId`, `RunId`, `ResultId`.
- `CatalogRevision`: content-derived catalog identity captured when Profile opens and submitted unchanged when it runs.
- `ProfileIntent(projectId, object, sourceSnapshot?, openedCatalogRevision)`: opening it performs no scan.
- `ProfileRequest(projectId, target, selectedColumns 1..100, catalogRevision, distinctMode)`.
- `DistinctMode = approximate | exact`: this choice changes distinct-count SQL only.
- `ProfileMetric(column?, kind, typedValue?, provenance, unavailableReason?, truncated)`.
- `ProfileSqlEvidence(columns, metricKinds, sql)`: immutable exact SQL text executed for those metrics. Evidence values contain SQL only, never returned data.
- `ProfileSnapshot(target, catalogRevision, metrics, statements, observedAt, distinctMode)`.
- `MetricProvenance = exact | approximate | sampled`.
- `ProfileState = queued | running | succeeded | failed | cancelled`.
- `CheckType = not_empty | not_null | unique | accepted_values | range | relationship | freshness | custom_sql`.
- `NullPolicy = fail_on_null | pass_on_null` where the check type permits a choice.
- `QualityCheckDefinition`, immutable `CheckRevision`, and aggregate-only `CheckRun` retain project and revision identity.
- `CheckOutcome = pass | fail | error | cancelled`.
- `FailurePreviewRequest(runId, revisionId)` produces an ephemeral existing `ResultId`. It never collects all failures.
- Named errors: `CatalogStale`, `SourceMissing`, `SessionMissing`, `ProfileBusy`, `ProfileInvalid`, `MetricUnsupported`, `CheckInvalid`, `SqlMutating`, `HistoryWriteFailed`, `Cancelled`.

### Metric applicability and provenance

| Metric                            | bool        | numeric                         | text                            | date/timestamp                  | binary/complex |
| --------------------------------- | ----------- | ------------------------------- | ------------------------------- | ------------------------------- | -------------- |
| Row count                         | Exact       | Exact                           | Exact                           | Exact                           | Exact          |
| NULL count/rate                   | Exact       | Exact                           | Exact                           | Exact                           | Exact          |
| Distinct count                    | Exact       | Approximate by default or Exact | Approximate by default or Exact | Approximate by default or Exact | Unavailable    |
| Minimum/maximum                   | Unavailable | Exact                           | Use text length                 | Exact                           | Unavailable    |
| Average                           | Unavailable | Exact                           | Unavailable                     | Unavailable                     | Unavailable    |
| Text lengths                      | Unavailable | Unavailable                     | Exact                           | Unavailable                     | Unavailable    |
| Common values, at most 20         | Exact       | Exact                           | Exact                           | Exact                           | Unavailable    |
| Representative values, at most 20 | Sampled     | Sampled                         | Sampled                         | Sampled                         | Sampled        |

An unsupported metric carries one reason. The UI omits unsupported metric groups and offers the reason through the selected-column details; it never renders a misleading zero.

### Check semantics

| Type            | Passes when                                   | Default NULL behavior         |
| --------------- | --------------------------------------------- | ----------------------------- |
| not_empty       | table/view has at least one row               | no column policy              |
| not_null        | zero rows contain NULL in the column          | NULL always fails             |
| unique          | no selected value tuple occurs more than once | NULL passes by default        |
| accepted_values | every tested value is in the accepted list    | NULL fails by default         |
| range           | every tested value stays inside typed bounds  | NULL passes by default        |
| relationship    | each tested child key exists in the parent    | NULL passes by default        |
| freshness       | latest value is inside the threshold          | all-NULL is invalid, not pass |
| custom_sql      | the read-only statement returns zero rows     | defined by user SQL           |

## GRAPH

### Open and run Profile

```text
Explorer object action (N) → resolve full object identity (1) → capture ProfileIntent (1) → open Profile, NO scan (1)
│ R: active project, CatalogSnapshot, main-schema SourceRecord mapping
├─ E: unsupported kind ↯escape(action absent)
├─ E: missing linked source ↯escape(action disabled for pointer and keyboard)
└─ 🔒 live catalog/source state → immutable intent

Profile setup (N) → choose 1..100 columns + distinct mode (N) → review local scan cost (1) → Run (1)
│ R: default first min(12, column count), searchable selector, no implicit execution
├─ E: stale catalog/source ↯escape(refresh setup, still no scan)
└─ 🔒 form state → ProfileRequest

ProfileRequest (1) → typed Tauri boundary (1) → dedicated ProfileRegistry enqueue (1)
│ R: active session, one active profile/session, terminal retention 32
├─ E: query/export/profile active ↯escape(ProfileBusy)
├─ E: invalid bounds/identity ↯escape(ProfileInvalid)
└─ A: ProfileId returned immediately; desktop polls non-overlapping

Profile worker claim (1) → verify full target/column identity (1) → execute bounded statements (N) → verify catalog again (1)
│ R: one cloned DuckDB connection, InterruptHandle, five-minute deadline
├─ E: CatalogStale | SourceMissing ↯escape(failed, no partial snapshot)
├─ E: cancel/timeout ↯escape(cancelled or stable deadline error)
└─ A: scalar summaries plus bounded value lists

Executed statements (N) → collect ProfileSqlEvidence (N) → assemble bounded ProfileSnapshot (1)
│ R: central identifier quoting, typed compiler, 256 KiB response budget, 64 KiB value truncation
├─ A: one row-count statement
├─ A: at most ceil(selected columns / 25) scalar aggregate statements
├─ A: at most one common-value and one representative-value statement per selected column
├─ A: total statement bound = 1 + ceil(N/25) + 2N for N selected columns
├─ A: every present metric has Exact, Approximate, or Sampled provenance
└─ A: every statement is immutable evidence; values are omitted before SQL evidence if payload reduction is needed

ProfileSnapshot (1) → column navigator (N) → selected-column metric groups (N) → selected evidence inspector (T)
│ R: compact workbench composition, container-responsive layout
├─ A: common values render as value/count rows, representative values as bounded examples
├─ A: Copy/Open SQL never executes and never mutates the snapshot
├─ A: Create check produces an unsaved draft only
└─ E: polling transport failure ⟳retry; later success clears transient error
```

A dedicated profile registry is intentional. It owns profile-specific bounded status and never writes Arrow result artifacts. Dispatch enforces mutual exclusion with query and export jobs, so Profile cannot create competing DuckDB scans. The registry admits one active profile per session and retains at most 32 terminal records. A deadline helper may interrupt the one running profile; it does not execute analytical work.

### Define and run checks

```text
Profile observation | New check (N) → typed unsaved draft (1) → validate live catalog and options (1)
│ R: catalog-backed controls, explicit NULL behavior
├─ E: stale/missing/type/limit error ↯escape(inline error, draft retained)
└─ 🔒 form JSON → bounded QualityCheckDraft

QualityCheckDraft (1) → Rust compile preview (1) → bind validate (1) → visible count/failure SQL (1)
│ R: central quoting, sidecar read-only validation for custom SQL
├─ E: mutation/external effect ↯escape(SqlMutating)
└─ A: Copy/Open SQL never executes

Save (1) → SQLite definition + immutable revision transaction (1)
│ R: project isolation, retention limits, foreign keys
├─ E: duplicate/stale/storage failure ↯escape(nothing persisted)
└─ A: saving never runs the check

Run one | Run enabled suite (N) → resolve immutable revisions in order (1) → submit count jobs (N)
│ R: existing query JobRegistry FIFO, QualityCoordinator
├─ E: invalid revision ↯escape(per-check error; suite continues)
├─ E: custom SQL requires explicit confirmation
└─ A: each check has its own disclosed observation boundary

Count terminal status (T) → finalize CheckRun exactly once (1) → render observed versus expected (1)
│ R: SQLite transaction, stable run id
├─ E: HistoryWriteFailed ↯escape(surfaced without duplicate row)
├─ A: pass/fail is a valid assertion result; error means not evaluated
└─ A: session remains usable after pass/fail/error/cancel

Failed run (1) → explicit failure preview request (1) → existing bounded ResultId pages (N)
│ R: immutable revision SQL, 500-row pages, 12-page desktop LRU
├─ E: missing revision/non-failed run ↯escape(preview refused)
└─ A: preview released on close/supersede/restart/shutdown
```

### Recovery and lifecycle

```text
catalog/source changes (N)
├─ open Profile: disable Run and offer Refresh setup; refresh never scans
├─ running Profile: terminal CatalogStale/SourceMissing, no partial metrics
└─ check: terminal error with Edit/Profile/Locate/Open SQL actions; none auto-run

project close | sidecar restart | shutdown (N)
├─ cancel active profile/check/preview work
├─ release result pages and cloned connections
├─ preserve definitions, revisions, and aggregate terminal history
└─ lose ephemeral Profile values and failure rows by design
```

## CARDINALITY

Explorer actions (N) · Profile open (1 per intent) · setup edits (N) · Profile submission (1 per run) · active profile (at most 1 per session) · status polls (T) · row-count statements (1) · scalar batches (N, at most ceil(columns/25)) · value-list statements (N, at most 2 per column) · evidence records (N, same statement bound) · definition revisions (1 per save) · suite jobs (N ordered) · terminal history row (1 per started check) · preview pages (N bounded).

## BOUNDARIES

- Catalog and source state 🔒 full `CatalogObjectId` plus catalog revision. Legacy source metadata maps only to objects Tarik creates unqualified in `main`; same-name objects in other schemas receive no source metadata.
- Profile request JSON 🔒 typed protocol validation in desktop and sidecar, including 1..100 columns.
- Generated identifiers 🔒 central DuckDB quoting. UI text is never concatenated into metric SQL.
- Profile SQL evidence 🔒 sidecar-generated exact executed SQL. It is data-free, response-bounded, and never logged.
- Custom check SQL 🔒 exactly one read-only result-producing statement; DDL, DML, COPY, ATTACH, INSTALL, and external/mutating effects are rejected.
- SQLite 🔒 bounded project-scoped definitions, immutable revisions, and aggregate facts only.
- Failure rows 🔒 existing Arrow page format and desktop safe-value decoding; never profile IPC or SQLite.
- Logs 🔒 stable IDs, codes, states, and durations only. No SQL, accepted values, metric values, examples, or failing rows.

## BEHAVIOR

- ⛈ explicit execution wraps Profile and checks. Opening, copying, refreshing setup, or opening SQL never scans data.
- ⛈ cancellation wraps active analytical work through DuckDB `InterruptHandle`.
- ⛈ provenance teaching wraps every present metric and check observation.
- ⛈ accessibility wraps workspaces: real button/list/table semantics, keyboard-complete actions, focus restoration, text plus color states, and no tooltip-only meaning.
- ⛈ responsive composition uses the Profile container width. At narrow widths, Explorer may collapse and Evidence becomes a tab/drawer; teaching content remains reachable.
- ⛈ structured logging is bounded and redacted.

## SCOPE

- ProfileIntent acquire@open → release@close/check handoff/project change.
- Profile connection acquire@submission → release@success|failure|cancel|close|restart|shutdown.
- Deadline helper acquire@worker claim → signal/release@terminal.
- ProfileSnapshot values/evidence acquire@success → release@workspace close/project change; never persisted.
- CheckRevision acquire@save → retain@history references → delete only through bounded metadata policy.
- CheckRun acquire@start → finalize exactly-once@terminal → release@retention/clear history.
- Failure ResultId acquire@explicit preview → release@close|supersede|cache clear|restart|shutdown.

## TEST LAYERS

R = {

- Protocol fixtures for request validation, evidence serialization, provenance, and 256 KiB snapshot limits.
- DuckDB fixtures for empty/all-NULL, numeric, boolean, Unicode text, temporal, binary/complex, quoted identifiers, and 100 columns.
- Instrumented profile executor proving scalar batch count and total statement bound.
- Sidecar lifecycle tests for busy exclusion, queued/running/terminal state, cancel, deadline, catalog staleness, missing links, and zero result artifacts.
- App tests proving pointer and Alt+P use the same eligibility predicate and legacy source metadata never crosses schema identity.
- Profile tests for no-auto-run, 12-column default, searchable selection, transient poll recovery, non-duplicated terminal errors, column grouping, value/count lists, SQL Copy/Open without execution, check handoff, focus, and narrow container behavior.
- Check tests for all variants, live catalog validation, immutable SQL/revisions, explicit custom confirmation, run outcomes, preview paging/release, and recovery actions.
- Manual real-Tauri review in light, dark, and 680×520 before any E14 UI correction is committed or pushed.

}; production and tests use the same graph with R substituted.

## VERDICT

The E14-T1 through T5 implementation was reconstructed on September 6, 2026. Persistence, check compilation, cancellation, bounded previews, and no-auto-run boundaries substantially match. The original graph did not match the dedicated Profile registry or the per-column statement cardinality, and T4/T5 automated tests did not establish visual approval. Remediation must make the statement bound and immutable SQL evidence true, fix object/source and keyboard eligibility bugs, replace the flat Profile metric table with the column/metrics/evidence composition, and obtain explicit manual approval before a UI commit or push. E14-T6 remains blocked on those remediation gates. E13-T6 Windows acceptance also remains open and is still required before E14 final acceptance.

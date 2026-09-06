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
- `QualityRunDetail(run, checkName, revisionNumber, currentRevisionNumber, revision, countSql, failureSql, isLatestRevision)`: restart-safe evidence reconstructed from the persisted aggregate run and its immutable typed revision. SQL compilation is deterministic, so the detail contains the exact revision-bound count and failure statements without persisting another SQL copy.
- `SuitePresentation(runIds 0..200, startedAt, statusCounts)`: desktop-owned progress for one explicit suite submission. Each run remains independently durable and cancellable.
- `FailurePreviewRequest(projectId, runId)` produces an ephemeral existing `ResultId` by recompiling the run's immutable revision. It is labeled as a current-data preview, never as historical failing rows, and never collects all failures.
- `RecoveryAction = edit_check | profile_target | repair_link | open_sql | retry_after_restart`: deterministic navigation only. It never mutates data or automatically executes SQL.
- Named errors: `CatalogStale`, `SourceMissing`, `SessionMissing`, `ProfileBusy`, `ProfileInvalid`, `MetricUnsupported`, `CheckInvalid`, `SqlMutating`, `RunMissing`, `RevisionMissing`, `PreviewBusy`, `HistoryWriteFailed`, `Cancelled`.

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

Persisted run id (1) → load aggregate run + immutable revision (1) → compile run detail (1) → render evidence (1)
│ R: SQLite metadata, deterministic Rust compiler, current definition revision number
├─ E: RunMissing | RevisionMissing ↯escape(history changed; refresh bounded list)
├─ A: historical run remains bound to its own revision after definition edits and restart
└─ A: SQL evidence is reconstructed but not executed

Historical rerun (1) → resolve the selected run's revision (1) → explicit custom confirmation when needed (1) → submit that revision (1)
│ R: active project/session, immutable revision, QualityCoordinator
├─ E: custom confirmation declined ↯escape(no execution)
└─ A: new aggregate run references the selected historical revision, not the latest definition

Failed persisted run (1) → explicit current-data failure preview request (1) → compile immutable revision SQL (1) → existing bounded ResultId pages (N)
│ R: 500-row pages, 12-page desktop LRU, one UI-owned preview at a time
├─ E: missing revision/non-failed run/preview limit ↯escape(preview refused)
├─ A: label = Current-data preview using revision N
└─ A: dedicated release on close/supersede; no historical row-fidelity claim

Run one | suite submission (1) → switch Definitions to Runs (1) → non-overlapping status polls (T) → terminal detail/history refresh (1)
│ R: text-plus-color states, 1 Hz elapsed display, bounded 200-check suite
├─ E: one run error ↯escape(explain error; other suite runs remain visible)
├─ E: status transport failure ⟳retry; preserve last truthful state
└─ A: queued/running/passed/failed/error/cancelled counts and per-check outcomes
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
├─ prune the coordinator to at most 256 terminal execution records and 8 tracked previews
├─ preserve definitions, revisions, deterministic SQL evidence, and aggregate terminal history
└─ lose ephemeral Profile values and failure rows by design

terminal run | missing object | missing column | missing linked file | engine loss (N)
├─ valid assertion failure: explain observed failures versus expected zero and offer Preview/Profile/Open SQL
├─ catalog/type mismatch: offer Edit check or Profile target
├─ linked-source error: offer Repair link through the existing source flow
├─ engine interruption/loss: offer explicit rerun after recovery
└─ every action navigates or prepares evidence; none executes or repairs automatically
```

## CARDINALITY

Explorer actions (N) · Profile open (1 per intent) · setup edits (N) · Profile submission (1 per run) · active profile (at most 1 per session) · status polls (T) · row-count statements (1) · scalar batches (N, at most ceil(columns/25)) · value-list statements (N, at most 2 per column) · evidence records (N, same statement bound) · definition revisions (1 per save) · suite submission (1, at most 200 ordered run ids) · suite jobs (N ordered) · status polls (T, non-overlapping) · terminal history row (1 per started check) · history page (N, 25 per desktop request and 100 maximum backend) · coordinator terminal records (N, at most 256) · tracked previews (N, at most 8 backend and one UI-owned) · preview pages (N bounded at 500 rows per page).

## BOUNDARIES

- Catalog and source state 🔒 full `CatalogObjectId` plus catalog revision. Legacy source metadata maps only to objects Tarik creates unqualified in `main`; same-name objects in other schemas receive no source metadata.
- Profile request JSON 🔒 typed protocol validation in desktop and sidecar, including 1..100 columns.
- Generated identifiers 🔒 central DuckDB quoting. UI text is never concatenated into metric SQL.
- Profile SQL evidence 🔒 sidecar-generated exact executed SQL. It is data-free, response-bounded, and never logged.
- Custom check SQL 🔒 exactly one read-only result-producing statement; DDL, DML, COPY, ATTACH, INSTALL, and external/mutating effects are rejected.
- SQLite 🔒 bounded project-scoped definitions, immutable revisions, and aggregate facts only.
- Run detail request 🔒 active project plus persisted run ownership; the backend resolves revision identity and compiles SQL rather than accepting SQL from the UI.
- Historical rerun request 🔒 persisted run identity to immutable revision; the UI cannot substitute a latest revision or statement.
- Failure rows 🔒 persisted failed-run identity to deterministic revision SQL, then existing Arrow page format and desktop safe-value decoding; never profile IPC or SQLite.
- Logs 🔒 stable IDs, codes, states, and durations only. No SQL, accepted values, metric values, examples, or failing rows.

## BEHAVIOR

- ⛈ explicit execution wraps Profile and checks. Opening, copying, refreshing setup, or opening SQL never scans data.
- ⛈ cancellation wraps active analytical work through DuckDB `InterruptHandle`.
- ⛈ provenance teaching wraps every present metric and check observation.
- ⛈ accessibility wraps workspaces: real button/list/table semantics, keyboard-complete actions, focus restoration, text plus color states, and no tooltip-only meaning.
- ⛈ responsive composition uses each workspace container width, never viewport media queries. Checks authoring shows all three panes when space permits and switches to explicit Checks, Definition, and SQL tabs when narrow. Runs uses bounded run-list and detail surfaces with the same narrow-width rule.
- ⛈ run polling is non-overlapping. A transient transport error preserves the last truthful run state and retries; terminal states trigger one bounded metadata refresh.
- ⛈ current-data preview language wraps every historical preview. No UI path calls ephemeral rows historical evidence.
- ⛈ structured logging is bounded and redacted.

## SCOPE

- ProfileIntent acquire@open → release@close/check handoff/project change.
- Profile connection acquire@submission → release@success|failure|cancel|close|restart|shutdown.
- Deadline helper acquire@worker claim → signal/release@terminal.
- ProfileSnapshot values/evidence acquire@success → release@workspace close/project change; never persisted.
- CheckRevision acquire@save → retain@history references → delete only through bounded metadata policy.
- CheckRun acquire@start → finalize exactly-once@terminal → retain aggregate@bounded history → release@retention/clear history.
- Coordinator execution record acquire@submission → retain while active plus bounded terminal reopening → prune oldest terminal@over 256.
- SuitePresentation acquire@explicit suite run → release@new suite/workspace close/project change; persisted component runs remain in history.
- Failure ResultId acquire@explicit current-data preview → release@close|supersede|dedicated release|cache clear|restart|shutdown.

## TEST LAYERS

R = {

- Protocol fixtures for request validation, evidence serialization, provenance, and 256 KiB snapshot limits.
- DuckDB fixtures for empty/all-NULL, numeric, boolean, Unicode text, temporal, binary/complex, quoted identifiers, and 100 columns.
- Instrumented profile executor proving scalar batch count and total statement bound.
- Sidecar lifecycle tests for busy exclusion, queued/running/terminal state, cancel, deadline, catalog staleness, missing links, and zero result artifacts.
- App tests proving pointer and Alt+P use the same eligibility predicate and legacy source metadata never crosses schema identity.
- Profile tests for no-auto-run, 12-column default, searchable selection, transient poll recovery, non-duplicated terminal errors, column grouping, value/count lists, SQL Copy/Open without execution, check handoff, focus, and narrow container behavior.
- Check tests for all variants, live catalog validation, immutable SQL/revisions, explicit custom confirmation, run outcomes, durable run detail, historical revision rerun, restart-safe current-data preview, preview paging/release, coordinator bounds, and recovery actions.
- A deterministic local data-trust fixture containing NULL, duplicate, range, accepted-value, stale-date, and unmatched-parent failures plus explicit repair SQL and expected outcomes.
- Golden workflow automation proving fail → bounded preview → release → repair → pass while preserving the original aggregate run.
- Manual real-Tauri review in light, dark, system, and 680x520 before the T6/T7 UI candidate is committed or pushed. Windows packaged review remains a separate open gate under E13-T6.

}; production and tests use the same graph with R substituted.

## VERDICT

The Profile remediation matches the graph and received explicit real-Tauri approval on September 6, 2026. The Checks authoring baseline has temporary approval, with its responsive layout correction intentionally combined with T6. The pre-T6 implementation does not yet match the run graph: reopening depends on an unbounded in-memory coordinator record, historical previews fail after restart, preview release has no dedicated quality command, and no Runs presentation exists. T6 must close those mismatches, and T7 must add the deterministic workflow evidence and review packet. Automated evidence may establish implementation completeness but cannot self-approve the real-Tauri visual/manual gate. E13-T6 Windows acceptance remains open and prevents a final cross-platform E14 acceptance claim.

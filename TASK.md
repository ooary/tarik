# Tarik Desktop App — Delivery Tracker

> Local-first DuckDB desktop workbench for beginner data engineers.
>
> This file is the implementation contract and shared tracker for maintainers and subagents. Keep it current in the same commit as the work it tracks.

## Product goal

Build a low-memory desktop application that lets a user:

- Create and reopen local analytical projects.
- Link Parquet datasets or import CSV/Parquet into DuckDB tables.
- Query multiple tables and linked views, including joins.
- Work with multiple autosaved SQL tabs and saved queries.
- Inspect a beginner-friendly visual DuckDB query flow.
- Browse large results without loading the complete result into the WebView.
- Export CSV or Parquet in exact, configurable row chunks.
- Review historical query executions after restarting the app.

## Confirmed architecture decisions

| Concern                 | Decision                                                     |
| ----------------------- | ------------------------------------------------------------ |
| Desktop shell           | Tauri 2                                                      |
| Backend                 | Rust                                                         |
| Analytical engine       | Embedded DuckDB via the maintained Rust binding              |
| Frontend                | React + TypeScript + Vite                                    |
| SQL editor              | CodeMirror 6                                                 |
| Result rendering        | Virtualized grid; never materialize the full result in React |
| Query flow              | XYFlow with deterministic automatic layout                   |
| Operational metadata    | SQLite                                                       |
| Analytical tables       | DuckDB                                                       |
| Linked datasets         | Original CSV/Parquet files; SQLite stores source metadata    |
| Historical queries      | SQLite; diagnostic/runtime events go to rolling log files    |
| Large temporary results | App cache directory, not SQLite                              |
| External integrations   | Out of scope for the current product                         |
| Credentials/keychain    | Out of scope; no credential-store module yet                 |
| Python                  | Not part of the core runtime                                 |

## Storage boundaries

```text
Tarik app data/
├── tarik.sqlite                  # Settings, sessions, saved queries, history
├── projects/
│   └── <project-id>/
│       └── project.duckdb        # Imported analytical tables and DuckDB views
├── cache/
│   └── results/<result-id>/      # Disposable bounded result-page artifacts
└── logs/
    └── tarik.YYYY-MM-DD.log      # Rotating diagnostics; no full datasets

User-selected locations/
├── *.csv                         # May be linked or imported
├── *.parquet                     # May be linked or imported
└── exports/                      # User-owned chunked exports
```

Rules:

1. SQLite stores metadata, SQL text, and execution history, not analytical row sets.
2. DuckDB stores imported analytical tables and project views.
3. A linked file remains in its original location; moving it must produce a recoverable “source missing” state.
4. Logs contain diagnostics and identifiers, but must not contain complete SQL result rows.
5. Full query results are ephemeral unless the user explicitly exports them.
6. Query history is durable in SQLite even when log files rotate.

## MVP success criteria

The MVP is complete when a user can:

1. Create a project and reopen it after restarting Tarik.
2. Link one or more Parquet files as named views.
3. Import a CSV or Parquet file as a DuckDB table.
4. Join any combination of imported tables and linked views.
5. Use multiple SQL tabs whose drafts, order, and active tab survive restart.
6. Run and cancel queries and see structured SQL errors.
7. Browse a multi-million-row result through bounded pages with stable memory usage.
8. Save, rename, organize, open, and delete queries.
9. Review successful, failed, and cancelled historical query executions.
10. Inspect an Explain flow and an execution Profile flow with beginner descriptions.
11. Export query output to CSV or Parquet with an exact maximum row count per file.
12. Find useful rotating logs after a crash or failure.

## Explicit non-goals for MVP

- BigQuery, PostgreSQL, S3, or other remote connectors.
- Authentication, secrets, or OS credential-store integration.
- Python runtime, notebooks, or user-defined Python plugins.
- AI-generated SQL.
- Collaborative/cloud synchronization.
- Spreadsheet editing, chart builder, or general BI dashboards.
- Editing source rows directly in the result grid.
- Persisting complete query results between application restarts.

---

# Working agreement for agents

## Task lifecycle

- A task ID is the smallest independently reviewable delivery unit.
- Before starting, confirm every task in `Depends on` is complete.
- Claim a task by changing `[ ]` to `[~]` and appending `— owner: <agent-name>`.
- Do not claim two tasks that edit the same owned paths concurrently.
- Finish implementation, tests, and documentation before changing `[~]` to `[x]`.
- Update this tracker in the same commit as the completed task.
- If blocked, change `[~]` back to `[ ]` and add a short `BLOCKED:` note.
- Do not silently expand scope. Add a new task under the appropriate EPIC.

## Manual EPIC review gates

Work proceeds one EPIC at a time unless the user explicitly approves parallel EPICs.

1. The lead agent assigns only dependency-ready tasks to subagents and owns integration of shared files.
2. Each subagent returns its task commit, checks run, known limitations, and manual test steps.
3. The lead agent reviews the diff, runs the EPIC-wide checks, fixes integration issues in separate atomic commits, and prepares a review packet.
4. The review packet includes commit list, changed paths, screenshots for visible UI, automated check results, and a short manual walkthrough.
5. The EPIC enters `REVIEW` and implementation pauses at the gate.
6. The user manually reviews and responds with approval or requested changes.
7. Requested changes are implemented as separate `fix(...)` commits and returned for review.
8. Dependent EPIC work starts only after explicit user approval is recorded in this file.

Backend-only EPICs still require a manual review packet with reproducible commands and observable output. A passing automated test suite does not replace user sign-off.

## Product UI design protocol

Tarik is a dense desktop product, not a landing page. For every user-visible task, load `design-taste-frontend` before implementation and apply its relevant anti-slop, accessibility, consistency, copy, interaction-state, performance, and pre-flight rules. Do not force its marketing-page patterns onto the workbench, editor, data grid, node graph, or import wizard.

The approved starting direction is:

```text
Design Read: local-first desktop SQL workbench for beginner data engineers,
             with a calm, precise IDE language and progressive disclosure.
DESIGN_VARIANCE: 3   # stable workspace geometry; asymmetry only when functional
MOTION_INTENSITY: 2  # hover, focus, resize, progress, and state feedback only
VISUAL_DENSITY: 7    # data-dense, but with clear hierarchy and readable defaults
Foundation: Radix accessible primitives + custom semantic tokens
Specialized UI: CodeMirror, TanStack Virtual, and XYFlow
```

Visible UI guardrails:

- No AI-purple gradients, neon glow, glassmorphism, decorative status dots, fake metrics, or ornamental dashboard cards.
- Use one restrained accent and one documented corner-radius system in both themes.
- Use cards only when they communicate elevation or interaction; prefer spacing and quiet dividers for structure.
- Use an approved icon family. Do not hand-roll SVG paths or use emoji as product icons.
- Use plain functional copy. Avoid startup slogans, filler verbs, and invented precision.
- Every feature includes loading, empty, error, success, disabled, focus, and cancellation states where applicable.
- Animation must communicate feedback or state, respect reduced motion, and avoid continuous decorative loops.
- Result tables follow data-grid patterns, not marketing-page table styling.
- Node color communicates meaningful state or performance, not arbitrary operator categories.
- Test light and dark themes, keyboard-only use, zoom, narrow windows, overflow, and realistic long labels/data.
- Before requesting review, run the relevant `design-taste-frontend` pre-flight checks and include the result in the review packet.

## Git discipline — mandatory

Every completed feature, fix, chore, documentation change, test addition, or refactor must have its own atomic Git commit.

Use Conventional Commits:

```text
feat(projects): create and reopen local projects
fix(results): release cursor when result tab closes
chore(repo): bootstrap Tauri workspace
docs(tasks): define MVP delivery plan
test(export): verify exact row chunk boundaries
refactor(engine): isolate DuckDB connection worker
```

Rules:

1. One task ID should normally map to one commit.
2. Never combine unrelated task IDs in a commit.
3. Commit only after relevant tests and checks pass.
4. Do not use `git add .`; stage intentional paths.
5. Never rewrite or force-push shared history unless the user explicitly requests it.
6. Do not commit generated build output, caches, exported datasets, local databases, or logs.
7. Include the task ID in the commit body, for example `Task: E3-T2`.
8. A temporary checkpoint commit is allowed only when clearly prefixed `chore(wip):`; squash it before declaring the task complete.

## Definition of done for every task

- [ ] Acceptance criteria are satisfied.
- [ ] Domain and boundary errors are structured, not opaque strings where avoidable.
- [ ] Rust unit/integration tests cover backend behavior.
- [ ] Frontend tests cover user-visible state changes where relevant.
- [ ] Loading, empty, error, success, and cancellation states are handled where relevant.
- [ ] Resources are structurally released on success, failure, cancellation, and window close.
- [ ] No unbounded result collection or frontend rendering was introduced.
- [ ] Accessibility labels, keyboard behavior, and focus are checked for UI work.
- [ ] `TASK.md` and affected documentation are updated.
- [ ] Formatting, linting, type checks, and relevant tests pass.
- [ ] The change is committed atomically using the Git policy above.

## Required checks before commit

Commands may be adjusted during E0 when package scripts are established, but equivalent checks remain mandatory.

```bash
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run format:check
npm run lint
npm run typecheck
npm test
npm run build
```

## Parallel-work ownership

Subagents may work in parallel only when their paths and dependencies do not overlap.

| Area                | Primary owned paths                                    |
| ------------------- | ------------------------------------------------------ |
| Shell/design system | `src/app/`, `src/components/ui/`, global styles        |
| Metadata            | `src-tauri/src/metadata/`, SQLite migrations           |
| DuckDB engine       | `src-tauri/src/engine/`                                |
| Sources/import      | `src-tauri/src/sources/`, `src/features/sources/`      |
| Editor/session UI   | `src/features/editor/`, `src/features/session/`        |
| Results             | `src-tauri/src/results/`, `src/features/results/`      |
| Query flow          | `src-tauri/src/plan/`, `src/features/query-flow/`      |
| Saved/history       | `src/features/saved-queries/`, `src/features/history/` |
| Export              | `src-tauri/src/export/`, `src/features/export/`        |
| Observability       | `src-tauri/src/observability/`, recovery UI            |

Shared files such as `package.json`, `Cargo.toml`, Tauri command registration, global types, and `TASK.md` require coordination. The task owner integrates changes to shared files.

---

# Domain model baseline

Use stable IDs rather than names or filesystem paths as identity.

```text
ProjectId        Project metadata and project.duckdb location
SourceId         Imported table or linked dataset identity
SessionId        Restorable window/workbench session
TabId            SQL editor tab identity
SavedQueryId     Durable named SQL asset
ExecutionId      One query attempt, including failure/cancellation
ResultId         Ephemeral result cursor/cache identity
ExportId         One export attempt
```

Important state variants:

```text
SourceKind       DuckDbTable | LinkedParquet | LinkedCsv
SourceState      Ready | Missing | InvalidSchema
ExecutionState   Queued | Running | Succeeded | Failed | Cancelled
ResultState      Loading | Ready | Exhausted | Released | Failed
ExportState      Queued | Running | Succeeded | Failed | Cancelled
```

---

# Design graph

```text
PROBLEM: build a low-memory local workbench for importing, querying, explaining,
         saving, restoring, and chunk-exporting analytical data

X → DesignGraph<A, E, R>
│              │   │  │  │
│              │   │  │  └─ R: DuckDB, SQLite, filesystem, workers
│              │   │  └──── E: SQL, parse, disk, cancellation, state errors
│              │   └─────── A: Source, Query, Batch, Plan, History, ExportPart
│              │
│              └─ nodes = functions, edges = data flow
│
└─ Tarik Desktop App

SHAPES: ProjectId, SourceId, SessionId, TabId, SavedQueryId, ExecutionId,
        ResultId, ExportId, Source, QueryDraft, QuerySnapshot, RecordBatch,
        ResultPage, QueryPlan, PlanNode, QueryHistoryEntry, ExportOptions,
        ExportPart, QueryError, ImportError, StorageError, ExportError

GRAPH:
  select_project (1)
  │ R: FileSystem, MetadataDb
  │ E: StorageError ↯escape(show recoverable project error)
  │ 🔒 selected path → ProjectPath
  ▼
  open_project (1)
  │ R: DuckDbWorker, MetadataDb
  │ E: InvalidDatabase ↯escape(return to project picker)
  ▼
  restore_session (1)
    R: MetadataDb
    E: CorruptSession ↯escape(open clean session and log warning)

  choose_source (1)
  │ R: FileDialog
  │ 🔒 selected path/options → SourceDefinition
  ▼
  inspect_source (1)
  │ R: DuckDbWorker, FileSystem
  │ E: ParseError ↯escape(show schema correction UI)
  ▼
  link_or_import (1)
    R: DuckDbWorker, MetadataDb, FileSystem
    E: MissingFile ↯escape(relocate or remove source)

  edit_query (N)
  │ R: MetadataDb
  │ 🔒 editor text → QuerySnapshot
  ▼
  execute_query (1)
  │ R: QueryWorker, DuckDbConnection, MetadataDb
  │ E: QueryError ↯escape(structured editor error and history entry)
  │ E: Interrupted ↯escape(cancelled state and history entry)
  ├──────────────► stream_batches (N)
  │                 R: DuckDbCursor, ResultCache
  │                 E: DiskError ↯escape(release cursor, preserve valid pages)
  │                 ▼
  │               render_visible_page (N)
  │                 R: ResultId, bounded page cache
  │
  └──────────────► capture_plan (1)
                    R: DuckDbConnection, PlanAdapter
                    E: UnsupportedPlan ↯escape(show raw textual plan)
                    ▼
                  render_flow (1)
                    R: GraphLayout

  QuerySnapshot
  ▼
  export_query (1)
  │ R: ExportWorker, DuckDbConnection, FileSystem
  ▼
  write_chunks (N)
    R: CSV/Parquet writer
    E: DiskFull ↯escape(close and retain completed parts)
    E: PermissionDenied ↯escape(select another directory)
    E: Interrupted ↯escape(close current part and report completed parts)

CARDINALITY: select_project (1) · open_project (1) · restore_session (1) ·
             choose_source (1) · inspect_source (1) · link_or_import (1) ·
             edit_query (N) · execute_query (1) · stream_batches (N) ·
             render_visible_page (N) · capture_plan (1) · render_flow (1) ·
             export_query (1) · write_chunks (N)

BOUNDARIES: filesystem selection → ProjectPath/SourceDefinition · CSV/Parquet
            metadata → typed SourceSchema · editor text → QuerySnapshot ·
            DuckDB explain/profile output → QueryPlan · form input → ExportOptions

BEHAVIOR: ⛈structured logging wraps commands · ⛈progress wraps import/export ·
          ⛈cancellation wraps execution/export · ⛈bounded LRU wraps result pages ·
          ⛈autosave debounce wraps session persistence

SCOPE: MetadataDb acquire@app → release@app · DuckDbConnection acquire@project →
       release@project-close · query cursor acquire@execution →
       release@result-close/error/cancel · output file acquire@chunk →
       release@chunk-complete/error/cancel · result cache acquire@result →
       release@tab-close/startup-cleanup

TEST LAYERS: temporary SQLite · temporary DuckDB · temporary filesystem ·
             deterministic plan fixtures · fake clock · same graph, no hidden globals

VERDICT: the design is viable if query/result/export paths remain streamed and
         bounded, SQLite is limited to metadata/history, and every connection,
         cursor, cache, and output file has structural cleanup.
```

---

# Delivery plan

Status legend: `[ ]` ready, `[~]` in progress, `[x]` complete, EPIC `REVIEW` waiting for user sign-off.

## EPIC execution and review order

```text
E0  Foundation              -> manual build/tooling review
E1  Shell and design system -> mandatory visual review
E2  SQLite metadata         -> manual persistence review
E3  DuckDB lifecycle        -> manual project lifecycle review
E4  Sources/import          -> mandatory workflow review
E5  Editor/sessions         -> mandatory visual and restart review
E6  Query/results           -> mandatory large-result and memory review
E7  Query flow              -> mandatory visual and beginner-usability review
E8  Saved/history           -> manual persistence and usability review
E9  Chunked export          -> manual file-boundary review
E10 Diagnostics/recovery    -> manual failure and cleanup review
E11 Release quality         -> final acceptance review
```

Only tasks inside the current EPIC may run concurrently, and only when dependencies and path ownership allow it. A later EPIC starts after the current EPIC's explicit user sign-off unless the user authorizes an exception.

### Next planned review gate: E0

The lead agent will first complete the remaining E0 tasks:

1. `E0-T2` bootstrap the Tauri, React, TypeScript, and Vite workspace.
2. After T2, assign `E0-T3` quality automation and `E0-T4` app directories/ignore rules in parallel because their primary paths are separable.
3. Integrate and run all E0 checks.
4. Stop and provide the E0 review packet. Do not start E1 until the user approves E0.

The next gate, E1, is the first visual checkpoint. Before E1 code, provide the Design Read, token direction, and low-fidelity workbench composition. After implementation, provide screenshots in light/dark modes and the adapted anti-slop pre-flight result for manual approval.

---

## EPIC E0 — Repository and engineering foundation

**Status:** `APPROVED` - user sign-off received; E1 authorized.

**Outcome:** Reproducible Tauri workspace with enforced quality and Git hygiene.

- [x] **E0-T1 Create EPIC tracker and initialize Git history**
  - Depends on: none
  - Owns: `TASK.md`, `.git/`
  - Deliverables:
    - Record architecture, scope, storage boundaries, agent workflow, and backlog.
    - Initialize repository and create the first atomic documentation commit.
  - Acceptance:
    - A new agent can select a task without needing chat history.
    - Git working tree is clean after commit.
  - Tests: review Markdown structure and run `git status`.
  - Commit: `docs(tasks): add epic delivery tracker`

- [x] **E0-T2 Bootstrap Tauri 2 + React + TypeScript + Vite workspace** — owner: lead-agent
  - Depends on: E0-T1
  - Owns: initial workspace, `package.json`, `src-tauri/Cargo.toml`, Tauri config
  - Deliverables:
    - Minimal desktop window starts successfully.
    - Rust command round-trip is demonstrated with a typed frontend wrapper.
    - Pin Node and Rust toolchain expectations in repository files/docs.
  - Acceptance:
    - Development and production builds succeed on the current platform.
    - No Electron or Python runtime dependency exists.
  - Tests: smoke test Tauri command; frontend render test.
  - Commit: `chore(repo): bootstrap tauri react workspace`
  - Implementation commit: recorded in Git history

- [x] **E0-T3 Add formatting, linting, test, and build automation** — owner: quality-subagent
  - Depends on: E0-T2
  - Owns: formatter/linter configs, package scripts, CI workflow
  - Deliverables:
    - Rust fmt, clippy, and tests.
    - TypeScript formatting, linting, type checking, and tests.
    - CI runs the same checks documented above.
  - Acceptance: a deliberately malformed fixture proves checks fail, then fixture is removed.
  - Tests: execute every required check locally.
  - Commit: `chore(ci): enforce project quality gates`
  - Implementation commit: `b11f8e0`

- [x] **E0-T4 Add ignore rules and application directory resolver** — owner: storage-subagent
  - Depends on: E0-T2
  - Owns: `.gitignore`, Rust app-path module
  - Deliverables:
    - Ignore build output, local DBs, datasets, logs, cache, and exports.
    - Resolve platform-correct data/cache/log directories.
  - Acceptance: startup creates required directories and returns structured path errors.
  - Tests: temp-directory path tests.
  - Commit: `chore(storage): define local app directories`
  - Implementation commit: `e90e0e3`

- [x] **E0-T5 Add Telegram-readable status contract and read-only status command** — owner: lead-agent
  - Depends on: E0-T1
  - Owns: `.tarik-agent/`
  - Deliverables:
    - Define versioned JSON fields for project, EPIC, task, state, summary, commit, review requirement, error, and timestamp.
    - Provide an executable read-only command at `/home/ooary/Projects/Tarik/.tarik-agent/read-status`.
    - Return valid JSON for missing status, normal status, and unsafe symlink status.
    - Document polling and state-transition semantics for the Telegram agent.
  - Acceptance:
    - `read-status` never writes, executes project code, or follows a symlinked status file.
    - Telegram agent can parse stdout as one JSON document using SSH.
    - Runtime status updates do not dirty Git history.
    - States include `working`, `completed`, `blocked`, `waiting_for_user_review`, and `unknown`.
  - Tests: shell smoke tests for missing, valid, and symlink status files.
  - Commit: `chore(agent): add read-only status contract`
  - Implementation commit: `8ff6f27`

---

## EPIC E1 — Desktop shell and interaction foundation

**Status:** `APPROVED` - user authorized E2; E1-T3 persistence will complete after E2-T2.

**Outcome:** Accessible workbench shell ready for feature modules.

**Mandatory design gate:** Use the Product UI design protocol above. This EPIC cannot be marked approved without user review of the workbench composition, light/dark screenshots, keyboard focus, narrow-window behavior, and anti-slop pre-flight result.

- [x] **E1-T1 Establish visual tokens and accessible UI primitives** — owner: lead-agent
  - Depends on: E0-T2
  - Owns: global styles, `src/components/ui/`
  - Deliverables:
    - Record the final Design Read and dial values before implementation.
    - Light/dark semantic tokens, restrained single accent, typography, spacing, and radius rules.
    - Button, input, dialog, tabs, tooltip, menu, empty state, skeleton, and inline error primitives.
    - Adapted `design-taste-frontend` pre-flight checklist for Tarik product surfaces.
  - Acceptance: keyboard focus and WCAG AA contrast are visible in both themes.
  - Tests: component interaction and accessibility tests.
  - Commit: `feat(ui): establish workbench design system`
  - Implementation commit: `a18becc`

- [x] **E1-T2 Build resizable workbench shell** — owner: lead-agent
  - Depends on: E1-T1
  - Owns: `src/app/`, shell layout components
  - Deliverables:
    - Top bar, source explorer, editor area, result/flow panel, and status bar.
    - Resizable/collapsible side and bottom panels.
    - Explicit minimum sizes and narrow-window behavior.
  - Acceptance: layout remains usable at minimum supported window size.
  - Tests: layout and keyboard interaction tests.
  - Commit: `feat(shell): add resizable desktop workbench`
  - Implementation commit: `73f6773`

- [x] **E1-T3 Persist UI preferences through typed settings interface** — owner: lead-agent
  - Depends on: E1-T2, E2-T2
  - Owns: UI preference store and typed commands
  - Deliverables: theme, panel widths, bottom-panel height, and last active panel persistence.
  - E1 foundation: typed repository contract, defaults, and normalization exist in `src/app/preferences.ts`; durable SQLite implementation waits for E2-T2.
  - Acceptance: settings restore after application restart and invalid values fall back safely.
  - Tests: settings round-trip and invalid-value tests.
  - Commit: `feat(settings): persist workbench preferences`
  - Implementation commit: included with E2-T2 integration

---

## EPIC E2 — SQLite metadata and durable application state

**Status:** `IN PROGRESS` - user authorized implementation.

**Outcome:** Versioned, transactional metadata store for all operational state.

- [x] **E2-T1 Create SQLite connection and migration framework** — owner: lead-agent
  - Depends on: E0-T4
  - Owns: `src-tauri/src/metadata/`, migrations
  - Deliverables:
    - One application metadata database at `tarik.sqlite`.
    - Versioned forward migrations executed transactionally.
    - Safe connection ownership and graceful corruption/open errors.
  - Acceptance: a fresh DB reaches latest schema; repeated startup is idempotent.
  - Tests: fresh, upgrade, rollback-on-failure, and incompatible-version tests.
  - Commit: `feat(metadata): add sqlite migrations`
  - Implementation commit: `d5efc07`

- [x] **E2-T2 Implement settings and recent-project repositories** — owner: lead-agent
  - Depends on: E2-T1
  - Owns: settings/projects metadata repositories
  - Deliverables:
    - Typed settings read/write.
    - Recent project create/update/list/remove.
    - No arbitrary JSON access from UI code.
  - Acceptance: repository operations are transactional and project IDs are stable.
  - Tests: repository CRUD and ordering tests.
  - Commit: `feat(metadata): persist settings and recent projects`
  - Implementation commit: `5675cf3`

- [x] **E2-T3 Add session, tab, and draft schema** — owner: lead-agent
  - Depends on: E2-T1
  - Owns: session migrations/repositories
  - Deliverables:
    - Durable sessions, tabs, tab order, active tab, title, SQL draft, timestamps.
    - Atomic session snapshot update.
  - Acceptance: partially written session snapshots cannot appear after failure.
  - Tests: ordering, active-tab uniqueness, and transaction rollback tests.
  - Commit: `feat(metadata): add durable query sessions`
  - Implementation commit: `6bf0de6`

- [x] **E2-T4 Add saved-query and query-history schema** — owner: lead-agent
  - Depends on: E2-T1
  - Owns: saved query/history migrations and repositories
  - Deliverables:
    - Saved query folders/tags and query records.
    - Execution history with project, SQL snapshot, status, timing, row counts, and structured error summary.
    - History retention setting and pruning operation.
  - Acceptance: successful, failed, and cancelled attempts are representable.
  - Tests: CRUD, filters, retention pruning, and cascade behavior.
  - Commit: `feat(metadata): add saved queries and execution history`
  - Implementation commit: pending commit

- [~] **E2-T5 Add source and export-history schema** — owner: lead-agent
  - Depends on: E2-T1
  - Owns: source/export migrations and repositories
  - Deliverables:
    - Linked/imported source definitions and source state.
    - Export attempt metadata and completed part summaries.
  - Acceptance: paths are metadata only; no file content is stored in SQLite.
  - Tests: source state transitions and export history tests.
  - Commit: `feat(metadata): persist sources and export history`

---

## EPIC E3 — DuckDB project and engine lifecycle

**Outcome:** Safe project-scoped DuckDB engine with bounded worker concurrency.

- [ ] **E3-T1 Implement project create/open/close lifecycle**
  - Depends on: E0-T4, E2-T2
  - Owns: `src-tauri/src/projects/`, engine composition root
  - Deliverables:
    - Create project directory and `project.duckdb`.
    - Open exactly one active project initially.
    - Close connections before switching/deleting projects.
  - Acceptance: reopen preserves DuckDB tables and operational project metadata.
  - Tests: create, reopen, invalid path, already-open, and close-on-error tests.
  - Commit: `feat(projects): manage local duckdb projects`

- [ ] **E3-T2 Add dedicated DuckDB worker and job protocol**
  - Depends on: E3-T1
  - Owns: `src-tauri/src/engine/worker.rs`, job types
  - Deliverables:
    - DuckDB work runs off Tauri async handlers.
    - Bounded queue and configured worker count.
    - Typed request/response IDs and structured failure translation.
  - Acceptance: long query does not block the UI command loop.
  - Tests: serialization of connection access, queue capacity, worker shutdown.
  - Commit: `feat(engine): add bounded duckdb worker`

- [ ] **E3-T3 Add engine resource and performance settings**
  - Depends on: E3-T2, E2-T2
  - Owns: engine configuration module
  - Deliverables:
    - Low memory, Balanced, and Fast profiles.
    - Configurable memory limit, thread count, and temporary directory.
    - Validate settings before applying them.
  - Acceptance: settings apply per opened project without raw SQL interpolation hazards.
  - Tests: profile mapping and validation tests.
  - Commit: `feat(engine): add resource profiles`

- [ ] **E3-T4 Add catalog inspection service**
  - Depends on: E3-T2
  - Owns: engine catalog module and typed commands
  - Deliverables: schemas, tables, views, columns, types, and source-kind metadata.
  - Acceptance: UI can refresh catalog after DDL/import without reopening project.
  - Tests: multiple schemas/tables/views and unusual identifier tests.
  - Commit: `feat(engine): expose project catalog`

---

## EPIC E4 — Local sources and import workflow

**Outcome:** Beginner-safe Parquet linking and CSV/Parquet imports.

- [ ] **E4-T1 Build source inspection boundary**
  - Depends on: E3-T2
  - Owns: `src-tauri/src/sources/inspect.rs`
  - Deliverables:
    - Validate selected path and supported extension.
    - Read CSV/Parquet schema and bounded preview through DuckDB.
    - Return structured parse warnings without loading full files.
  - Acceptance: malformed/unsupported/missing files produce recoverable errors.
  - Tests: fixture matrix for valid and invalid CSV/Parquet.
  - Commit: `feat(sources): inspect local datasets`

- [ ] **E4-T2 Link a Parquet file or glob as a named DuckDB view**
  - Depends on: E4-T1, E2-T5, E3-T4
  - Owns: source linking backend and source explorer integration
  - Deliverables:
    - Safe identifier/path handling.
    - Single file and multi-file glob support.
    - Durable source metadata and catalog refresh.
  - Acceptance: linked view can join imported tables; no data is copied.
  - Tests: spaces/quotes in paths, glob, join, duplicate name, missing file.
  - Commit: `feat(sources): link parquet datasets`

- [ ] **E4-T3 Build CSV import wizard**
  - Depends on: E4-T1, E1-T2
  - Owns: CSV import UI and options types
  - Deliverables:
    - Preview delimiter, header, nulls, encoding assumptions, and inferred types.
    - Per-column type override and destination table naming.
    - Clear progress, cancellation, and parse-error states.
  - Acceptance: user confirms trusted options before data copy begins.
  - Tests: wizard navigation, overrides, validation, cancellation.
  - Commit: `feat(import): add csv import wizard`

- [ ] **E4-T4 Import CSV and Parquet as DuckDB tables**
  - Depends on: E4-T1, E2-T5, E3-T2
  - Owns: source import backend
  - Deliverables:
    - Transactional import with progress events where available.
    - Cleanup incomplete table on error/cancel.
    - Persist source metadata and refresh catalog.
  - Acceptance: imported source remains queryable after original file moves.
  - Tests: import/reopen, cancel cleanup, duplicate table, disk/write failure.
  - Commit: `feat(import): import datasets into duckdb`

- [ ] **E4-T5 Detect and repair missing linked sources**
  - Depends on: E4-T2
  - Owns: source health backend/UI
  - Deliverables: missing state, locate replacement, verify compatible schema, remove source.
  - Acceptance: broken source does not prevent unrelated project use.
  - Tests: move, relink, incompatible replacement, remove.
  - Commit: `feat(sources): repair missing linked files`

---

## EPIC E5 — SQL editor and restorable sessions

**Outcome:** Fast multi-tab SQL workspace that never loses drafts during normal use.

- [ ] **E5-T1 Integrate CodeMirror SQL editor**
  - Depends on: E1-T2, E3-T4
  - Owns: `src/features/editor/`
  - Deliverables:
    - SQL syntax highlighting, line numbers, selection, find, and keyboard run shortcut.
    - Catalog-aware completion for schemas, tables/views, and columns.
  - Acceptance: editor remains responsive with realistically large SQL scripts.
  - Tests: keyboard commands and completion data adapter.
  - Commit: `feat(editor): add codemirror sql workspace`

- [ ] **E5-T2 Implement multi-tab session model**
  - Depends on: E5-T1, E2-T3
  - Owns: editor/session frontend and typed commands
  - Deliverables:
    - Create, rename, reorder, activate, duplicate, and close tabs.
    - Dirty-state indicator and close confirmation when persistence failed.
  - Acceptance: tab identity is stable and active-tab invariant holds.
  - Tests: tab reducer/store transitions and user interactions.
  - Commit: `feat(session): add multi-tab query workspace`

- [ ] **E5-T3 Autosave and restore drafts**
  - Depends on: E5-T2
  - Owns: autosave/restore behavior
  - Deliverables:
    - Debounced draft snapshots with flush on close/window shutdown.
    - Restore tab order, active tab, title, and SQL after restart.
    - Clean fallback if stored session is invalid.
  - Acceptance: rapid edits cannot write older SQL over newer SQL.
  - Tests: debounce ordering, shutdown flush, corrupt-session fallback.
  - Commit: `feat(session): autosave and restore query drafts`

- [ ] **E5-T4 Add SQL-to-source affordances**
  - Depends on: E5-T1, E3-T4
  - Owns: editor/source explorer interactions
  - Deliverables: insert quoted identifier, open table preview query, copy qualified name.
  - Acceptance: generated SQL handles unusual identifiers safely.
  - Tests: identifier quoting and UI actions.
  - Commit: `feat(editor): connect catalog actions to sql tabs`

---

## EPIC E6 — Query execution and bounded result browsing

**Outcome:** Cancellable execution with large-result browsing that has a defined memory ceiling.

- [ ] **E6-T1 Define typed query execution protocol**
  - Depends on: E3-T2, E2-T4
  - Owns: `src-tauri/src/query/`, shared frontend command types
  - Deliverables:
    - Execute immutable SQL snapshot with project/tab/execution IDs.
    - Queued/running/succeeded/failed/cancelled events.
    - Structured DuckDB error location/message where available.
  - Acceptance: every terminal execution state creates one durable history entry.
  - Tests: state machine and history integration tests.
  - Commit: `feat(query): add typed execution lifecycle`

- [ ] **E6-T2 Implement query cancellation and cleanup**
  - Depends on: E6-T1
  - Owns: query cancellation backend/UI
  - Deliverables:
    - Cancel queued or active query.
    - Interrupt connection safely.
    - Release cursor/cache on cancellation.
  - Acceptance: cancelled worker remains usable for a later query.
  - Tests: cancel queued, active, already-finished, and repeated cancellation.
  - Commit: `feat(query): support safe cancellation`

- [ ] **E6-T3 Stream query output into bounded result pages**
  - Depends on: E6-T1, E0-T4
  - Owns: `src-tauri/src/results/`
  - Deliverables:
    - Bounded batches/page cache with configurable maximum.
    - Result metadata returned separately from page data.
    - Spill eligible pages to cache directory when required.
    - Explicit result release command.
  - Acceptance: backend never collects the full result solely for UI display.
  - Tests: multi-batch query, eviction, spill/readback, close cleanup, query error.
  - Commit: `feat(results): add bounded result paging`

- [ ] **E6-T4 Build virtualized result grid**
  - Depends on: E6-T3, E1-T2
  - Owns: `src/features/results/`
  - Deliverables:
    - Row and column virtualization.
    - Typed formatting for null, boolean, numeric, date/time, binary, and nested values.
    - Copy cell/row/selection and visible loading/error states.
  - Acceptance: DOM size remains bounded while browsing a large fixture.
  - Tests: virtualization adapter, type formatting, keyboard navigation.
  - Commit: `feat(results): add virtualized data grid`

- [ ] **E6-T5 Add result lifecycle and memory instrumentation**
  - Depends on: E6-T3, E6-T4
  - Owns: result status UI/backend metrics
  - Deliverables:
    - Display loaded page range, total/unknown count, cache use, and released state.
    - Release superseded results and all results on project close.
  - Acceptance: repeated run/close cycle does not leak open cursors or cache directories.
  - Tests: lifecycle stress integration test.
  - Commit: `fix(results): enforce bounded result lifecycle`

---

## EPIC E7 — Beginner-friendly query flow

**Outcome:** Explain and Profile plans become understandable node graphs.

- [ ] **E7-T1 Capture stable DuckDB Explain/Profile fixtures**
  - Depends on: E6-T1
  - Owns: plan fixtures and compatibility notes
  - Deliverables:
    - Fixtures for scan, pushed filter, projection, join, aggregate, sort, limit, union, CTE, and window.
    - Record supported DuckDB version and fallback expectations.
  - Acceptance: fixtures contain no machine-specific absolute paths.
  - Tests: fixture loading/validation.
  - Commit: `test(plan): add duckdb plan fixtures`

- [ ] **E7-T2 Parse DuckDB plans into a normalized graph**
  - Depends on: E7-T1
  - Owns: `src-tauri/src/plan/`
  - Deliverables:
    - Stable `QueryPlan`, `PlanNode`, and edge types.
    - Preserve unknown operators rather than dropping them.
    - Fallback to raw textual plan if structured parsing fails.
  - Acceptance: all fixture operators produce connected, deterministic graphs.
  - Tests: golden parser tests and malformed-plan fallback.
  - Commit: `feat(plan): normalize duckdb query plans`

- [ ] **E7-T3 Build XYFlow query graph**
  - Depends on: E7-T2, E1-T2
  - Owns: `src/features/query-flow/`
  - Deliverables:
    - Directed source-to-result layout, fit view, zoom, pan, minimap only if useful.
    - Nodes show operation, source, estimate/actual rows, and timing where available.
    - Loading, empty, unsupported, and error states.
  - Acceptance: two-input joins visibly converge into one join node.
  - Tests: graph mapping, selection, keyboard navigation, snapshot fixtures.
  - Commit: `feat(flow): visualize query execution plans`

- [ ] **E7-T4 Add beginner explanations and node inspector**
  - Depends on: E7-T3
  - Owns: flow explanation dictionary and inspector UI
  - Deliverables:
    - Plain-language explanations for common operators.
    - Join type/keys, group keys, filters, projections, sort keys, and source details.
    - Explain estimated values vs Profile actual values are clearly labeled.
  - Acceptance: unknown operators show truthful generic details, not invented explanations.
  - Tests: explanation mapping and inspector states.
  - Commit: `feat(flow): explain query nodes for beginners`

- [ ] **E7-T5 Link flow nodes to relevant SQL when reliably available**
  - Depends on: E7-T4
  - Owns: editor/flow selection bridge
  - Deliverables: selecting a node highlights related SQL; unsupported mappings do nothing harmful.
  - Acceptance: feature is presented as best-effort and never highlights a knowingly wrong range.
  - Tests: mapping fixtures and unsupported cases.
  - Commit: `feat(flow): connect plan nodes to sql ranges`

---

## EPIC E8 — Saved queries and historical executions

**Outcome:** Durable query library and useful local audit trail.

- [ ] **E8-T1 Implement saved-query service and UI**
  - Depends on: E2-T4, E5-T2
  - Owns: `src/features/saved-queries/`, saved query commands
  - Deliverables:
    - Save current tab, save as, open, rename, move folder, tag, search, and delete.
    - Prevent accidental overwrite with explicit update semantics.
  - Acceptance: saved query survives restart and opens into a new/existing tab predictably.
  - Tests: CRUD, search, overwrite confirmation, folder behavior.
  - Commit: `feat(queries): add saved query library`

- [ ] **E8-T2 Build query-history service and UI**
  - Depends on: E6-T1, E2-T4
  - Owns: `src/features/history/`, history commands
  - Deliverables:
    - Filter by project, status, text, and time.
    - Show SQL snapshot, timestamps, duration, row count, and error summary.
    - Reopen historical SQL without automatically executing it.
  - Acceptance: success, failure, and cancellation all appear once and in correct order.
  - Tests: filtering, pagination, reopen, and terminal-state deduplication.
  - Commit: `feat(history): add historical query browser`

- [ ] **E8-T3 Add configurable retention and clearing controls**
  - Depends on: E8-T2
  - Owns: history settings/UI and pruning command
  - Deliverables: retention by age/count, manual clear with confirmation, transactional pruning.
  - Acceptance: clearing history never deletes saved queries or SQL drafts.
  - Tests: retention boundary and isolation tests.
  - Commit: `feat(history): manage local history retention`

---

## EPIC E9 — Streaming chunked exports

**Outcome:** Exact row-count CSV/Parquet chunks with progress, cancellation, and safe partial failure.

- [ ] **E9-T1 Define and validate export options**
  - Depends on: E6-T1
  - Owns: export domain types and validation
  - Deliverables:
    - Format, output directory, base name, rows per part, overwrite policy.
    - CSV delimiter/header options and Parquet compression option.
    - Safe naming and positive row-limit validation.
  - Acceptance: invalid options fail before query execution or file creation.
  - Tests: validation and filename sequence tests.
  - Commit: `feat(export): define chunk export options`

- [ ] **E9-T2 Implement one-pass exact row chunk writer**
  - Depends on: E9-T1, E3-T2
  - Owns: `src-tauri/src/export/`
  - Deliverables:
    - Execute once, stream batches, split crossing batches, rotate files at exact row count.
    - Never use repeated `LIMIT/OFFSET` queries.
    - CSV header in every part when enabled.
    - Deterministic `part-00001` naming.
  - Acceptance: every non-final part has exactly the requested number of rows.
  - Tests: zero rows, exact boundary, boundary+1, many batches, batch larger than chunk, CSV/Parquet readback.
  - Commit: `feat(export): stream exact row chunks`

- [ ] **E9-T3 Add export progress, cancellation, and partial-failure policy**
  - Depends on: E9-T2, E2-T5
  - Owns: export worker events, history integration
  - Deliverables:
    - Rows/files/bytes written, elapsed time, current part.
    - Cancellation closes files and reports completed parts.
    - Failed/incomplete current part is removed or marked clearly; completed parts stay valid.
  - Acceptance: worker and project remain usable after cancel/disk failure.
  - Tests: cancellation, permission failure, simulated disk full, cleanup.
  - Commit: `feat(export): add resilient export lifecycle`

- [ ] **E9-T4 Build export dialog and completion summary**
  - Depends on: E9-T1, E9-T3, E1-T1
  - Owns: `src/features/export/`
  - Deliverables: accessible options form, progress view, cancel, completed parts, reveal output location.
  - Acceptance: long operations show continuous non-blocking feedback.
  - Tests: validation, progress events, cancel, success, partial failure.
  - Commit: `feat(export): add chunk export workflow`

---

## EPIC E10 — Diagnostics, recovery, and cleanup

**Outcome:** Diagnosable failures and bounded on-disk application state.

- [ ] **E10-T1 Add structured rolling file logging**
  - Depends on: E0-T4
  - Owns: `src-tauri/src/observability/`
  - Deliverables:
    - Daily or size-based rolling logs with retention.
    - Levels, timestamps, operation IDs, project IDs, and duration.
    - Redact result data; avoid logging full SQL at info level by default.
  - Acceptance: user can reveal logs from settings; rotation bounds disk use.
  - Tests: rotation/retention and redaction tests.
  - Commit: `feat(logging): add rolling diagnostic logs`

- [ ] **E10-T2 Add panic/error boundary and support diagnostics**
  - Depends on: E10-T1, E1-T1
  - Owns: Rust panic hook, frontend error boundary, diagnostics UI
  - Deliverables:
    - Friendly crash/error surface with log location and copyable incident ID.
    - No raw panic details presented as the only user message.
  - Acceptance: simulated backend and frontend failures lead to recoverable diagnostics.
  - Tests: error boundary and panic-hook unit tests where practical.
  - Commit: `feat(diagnostics): add app failure recovery surfaces`

- [ ] **E10-T3 Clean stale caches and incomplete exports safely**
  - Depends on: E6-T3, E9-T3
  - Owns: cache cleanup service
  - Deliverables:
    - Startup cleanup for abandoned result cache entries.
    - Age/size limits and explicit clear-cache action.
    - Never delete user-completed exports.
  - Acceptance: cleanup is constrained to Tarik-owned cache/staging directories.
  - Tests: path safety, stale/fresh distinction, partial export handling.
  - Commit: `chore(storage): clean stale temporary artifacts`

- [ ] **E10-T4 Add graceful application shutdown coordinator**
  - Depends on: E3-T2, E5-T3, E6-T2, E9-T3
  - Owns: app shutdown composition
  - Deliverables:
    - Flush current drafts.
    - Cancel/finish active jobs according to explicit policy.
    - Close files, cursors, DuckDB, SQLite, and logger in order.
  - Acceptance: forced test shutdown leaves databases reopenable and no owned temp file locked.
  - Tests: shutdown during edit, query, and export.
  - Commit: `feat(app): coordinate graceful shutdown`

---

## EPIC E11 — Quality, performance, packaging, and release

**Outcome:** Repeatable, measured MVP release with documented limits.

- [ ] **E11-T1 Add end-to-end golden workflows**
  - Depends on: E4, E5, E6, E7, E8, E9 core tasks
  - Owns: E2E tests and fixtures
  - Deliverables:
    - Create project → import/link → join query → inspect flow → save → export → restart → history/session restore.
    - Missing linked file and cancelled query/export paths.
  - Acceptance: workflows pass from a clean application data directory.
  - Tests: automated E2E suite on supported CI environment.
  - Commit: `test(e2e): cover core tarik workflows`

- [ ] **E11-T2 Establish performance and memory budgets**
  - Depends on: E6-T5, E9-T3
  - Owns: benchmark harness and performance docs
  - Deliverables:
    - Measure idle, large-result browsing, repeated-query, import, and export memory.
    - Define bounded page/cache/worker defaults from measurements.
    - Record dataset shape and machine details with every benchmark.
  - Acceptance: no unbounded growth across repeated result open/close and export cancellation loops.
  - Tests: repeatable benchmark/stress scripts.
  - Commit: `perf(app): establish memory regression budgets`

- [ ] **E11-T3 Security and filesystem boundary review**
  - Depends on: E4, E9, E10 core tasks
  - Owns: Tauri capabilities, boundary tests, review notes
  - Deliverables:
    - Least-privilege Tauri commands/capabilities.
    - Identifier/path escaping review.
    - Confirm cleanup cannot traverse outside owned directories.
  - Acceptance: UI cannot invoke arbitrary filesystem or SQL-adjacent internal operations outside declared commands.
  - Tests: malicious path/identifier and command-input tests.
  - Commit: `fix(security): harden local trust boundaries`

- [ ] **E11-T4 Add packaging, versioning, and release artifacts**
  - Depends on: E11-T1, E11-T2, E11-T3
  - Owns: packaging/release config and docs
  - Deliverables:
    - Application name/icon/version, platform bundles, licenses/notices.
    - Upgrade compatibility note for SQLite migrations and DuckDB projects.
    - Checksums for distributed artifacts where supported.
  - Acceptance: clean-machine install/start/uninstall smoke test on each declared platform.
  - Tests: release build and packaged smoke test.
  - Commit: `chore(release): package tarik desktop mvp`

- [ ] **E11-T5 Write user documentation and ship checklist**
  - Depends on: E11-T4
  - Owns: README/user docs/release checklist
  - Deliverables:
    - Explain CSV vs Parquet, link vs import, tables/views, joins, Explain vs Profile, chunk export, history, logs, and data locations.
    - Document known limitations and backup behavior.
  - Acceptance: a new user can complete the golden workflow without developer help.
  - Tests: manual documentation walkthrough.
  - Commit: `docs(app): add mvp user guide`

---

# Cross-cutting test matrix

| Layer                  | Required coverage                                                          |
| ---------------------- | -------------------------------------------------------------------------- |
| Pure Rust domain       | IDs, state machines, option validation, filename/chunk calculation         |
| SQLite repositories    | Fresh migrations, upgrades, transactions, retention, CRUD                  |
| DuckDB integration     | Project reopen, catalog, joins, import/link, cancellation, plans           |
| Filesystem integration | Missing paths, unusual names, permissions, cleanup safety                  |
| Frontend unit          | Stores/reducers, formatting, graph mapping, forms                          |
| Frontend component     | Keyboard/focus, loading/empty/error/success/cancel states                  |
| End-to-end             | Golden workflow, restart restore, missing link, large result, chunk export |
| Performance            | Bounded DOM, result cache, repeated execution lifecycle, streaming export  |

## Required export boundary cases

For chunk size `1,000,000`, verify outputs for:

- `0` rows → defined empty-export behavior.
- `1` row → one part with one row.
- `999,999` rows → one part.
- `1,000,000` rows → one exact part.
- `1,000,001` rows → parts of `1,000,000` and `1`.
- `2,000,000` rows → two exact parts.
- Input batch crossing the boundary midway.
- Input batch larger than the configured chunk.
- Cancellation during first and later parts.
- Write failure after one or more completed parts.

---

# Risks and guardrails

| Risk                                         | Guardrail                                                                      |
| -------------------------------------------- | ------------------------------------------------------------------------------ |
| Millions of rows copied into frontend memory | Bounded backend pages plus row/column virtualization                           |
| DuckDB query blocks Tauri runtime            | Dedicated bounded worker(s)                                                    |
| Repeated export scans                        | One query execution and streaming batch splitter; never LIMIT/OFFSET loops     |
| Broken link after source moves               | Durable missing state and relink workflow                                      |
| SQL/path injection through generated DDL     | Central identifier quoting and parameter/path handling with adversarial tests  |
| DuckDB plan format changes                   | Versioned fixtures, normalized adapter, truthful raw-text fallback             |
| Session loses latest edit                    | Monotonic/debounced snapshots and shutdown flush                               |
| App metadata grows forever                   | Configurable query-history and log retention                                   |
| Logs expose user data                        | Structured identifiers and errors; no result rows; full SQL omitted by default |
| Cache fills disk                             | Size/age limits and startup cleanup constrained to owned directory             |
| Subagents create integration conflicts       | Path ownership, dependency gates, atomic commits                               |

---

# Open decisions — resolve before affected task starts

- [ ] **D1:** Confirm supported MVP operating systems and packaging priority.
- [ ] **D2:** Select and pin the DuckDB Rust binding/version after a minimal Arrow/profile compatibility spike.
- [ ] **D3:** Choose the Arrow batch transport strategy across Tauri IPC after measuring JSON vs binary transfer overhead.
- [ ] **D4:** Define default result page size, cache size, and worker count from E11-T2 measurements rather than guesses.
- [ ] **D5:** Define whether “empty export” creates no files or one schema-only file; document consistently.
- [ ] **D6:** Decide whether project deletion removes only Tarik-managed project files or merely forgets an externally located `.duckdb` file.

Do not resolve an open decision implicitly inside unrelated code. Record the decision here and in the implementing commit.

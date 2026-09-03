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

| Concern                 | Decision                                                                |
| ----------------------- | ----------------------------------------------------------------------- |
| Desktop shell           | Tauri 2                                                                 |
| Backend                 | Rust control plane plus independently built Rust engine sidecars        |
| Analytical engine       | Engine-agnostic sidecar protocol; DuckDB is the first adapter           |
| Engine runtime          | Long-running process; DuckDB uses a pinned prebuilt dynamic library     |
| Frontend                | React + TypeScript + Vite                                               |
| SQL editor              | CodeMirror 6                                                            |
| Result rendering        | Virtualized grid; never materialize the full result in React            |
| Result transport        | Metadata plus bounded pages; Arrow/IPC stays inside engine/cache layers |
| Query flow              | XYFlow with deterministic automatic layout                              |
| Operational metadata    | SQLite                                                                  |
| Analytical tables       | DuckDB                                                                  |
| Linked datasets         | Original CSV/Parquet files; SQLite stores source metadata               |
| Historical queries      | SQLite; diagnostic/runtime events go to rolling log files               |
| Large temporary results | App cache directory, not SQLite                                         |
| External integrations   | Out of scope for the current product                                    |
| Credentials/keychain    | Out of scope; no credential-store module yet                            |
| Python                  | Not part of the core runtime                                            |
| Engine extensibility    | Common capabilities plus namespaced engine-specific operations          |

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

## Advisory references

- `from_claude-rust-tauri-dev-setup.md`: user-shared Rust/Tauri development and build-loop notes. Tracked locally but ignored by Prettier and Git; not application source.
- `advise-from-claude/duckdb-rust-tauri.md`: comparison notes on avoiding `bundled` DuckDB compilation. Reviewed and incorporated into E5.5.
- `advise-from-claude/arrow-file-io-implementation.md`: Arrow-based file I/O notes. Its bounded-page and export ideas inform E5.5-T2 and later E6/E9 work; Tarik keeps DuckDB as its SQL engine.

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

| Area                   | Primary owned paths                                                             |
| ---------------------- | ------------------------------------------------------------------------------- |
| Shell/design system    | `src/app/`, `src/components/ui/`, global styles                                 |
| Metadata               | `src-tauri/src/metadata/`, SQLite migrations                                    |
| Engine protocol/client | `crates/engine-protocol/`, `crates/engine-client/`, Tauri engine-manager module |
| DuckDB engine adapter  | `engines/duckdb/`                                                               |
| Sources/import         | `src-tauri/src/sources/`, `src/features/sources/`                               |
| Editor/session UI      | `src/features/editor/`, `src/features/session/`                                 |
| Results                | `src-tauri/src/results/`, `src/features/results/`                               |
| Query flow             | `src-tauri/src/plan/`, `src/features/query-flow/`                               |
| Saved/history          | `src/features/saved-queries/`, `src/features/history/`                          |
| Export                 | `src-tauri/src/export/`, `src/features/export/`                                 |
| Observability          | `src-tauri/src/observability/`, recovery UI                                     |

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
  │ R: EngineManager, MetadataDb
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
  │ R: EngineClient, FileSystem
  │ E: ParseError ↯escape(show schema correction UI)
  ▼
  link_or_import (1)
    R: EngineClient, MetadataDb, FileSystem
    E: MissingFile ↯escape(relocate or remove source)

  edit_query (N)
  │ R: MetadataDb
  │ 🔒 editor text → QuerySnapshot
  ▼
  execute_query (1)
  │ R: EngineClient, MetadataDb
  │ E: QueryError ↯escape(structured editor error and history entry)
  │ E: Interrupted ↯escape(cancelled state and history entry)
  ├──────────────► stream_batches (N)
  │                 R: EngineResultCursor, ResultCache
  │                 E: DiskError ↯escape(release cursor, preserve valid pages)
  │                 ▼
  │               render_visible_page (N)
  │                 R: ResultId, bounded page cache
  │
  └──────────────► capture_plan (1)
                    R: EngineClient, PlanAdapter
                    E: UnsupportedPlan ↯escape(show raw textual plan)
                    ▼
                  render_flow (1)
                    R: GraphLayout

  QuerySnapshot
  ▼
  export_query (1)
  │ R: ExportWorker, EngineClient, FileSystem
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

SCOPE: MetadataDb acquire@app → release@app · engine process acquire@engine-manager →
       release@app-exit · engine session acquire@project-open → release@project-close ·
       result cursor acquire@execution → release@result-close/error/cancel ·
       output file acquire@chunk → release@chunk-complete/error/cancel ·
       result cache acquire@result → release@tab-close/startup-cleanup

TEST LAYERS: temporary SQLite · in-process engine adapter for protocol tests ·
             fake engine process for client tests · temporary filesystem ·
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
E5.5 Engine protocol/adapters -> mandatory build-time, process, and compatibility review
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

**Status:** `APPROVED` - all E1 tasks complete, including SQLite-backed preference persistence.

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
  - Implementation commit: `f408433`

---

## EPIC E2 — SQLite metadata and durable application state

**Status:** `APPROVED` - user authorized E3.

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
  - Implementation commit: `e9470f9`

- [x] **E2-T5 Add source and export-history schema** — owner: lead-agent
  - Depends on: E2-T1
  - Owns: source/export migrations and repositories
  - Deliverables:
    - Linked/imported source definitions and source state.
    - Export attempt metadata and completed part summaries.
  - Acceptance: paths are metadata only; no file content is stored in SQLite.
  - Tests: source state transitions and export history tests.
  - Commit: `feat(metadata): persist sources and export history`
  - Implementation commit: `1cc94e4`

---

## EPIC E3 — DuckDB project and engine lifecycle

**Status:** `APPROVED` - user authorized E4.

**Outcome:** Safe project-scoped DuckDB engine with bounded worker concurrency.

- [x] **E3-T1 Implement project create/open/close lifecycle** — owner: lead-agent
  - Depends on: E0-T4, E2-T2
  - Owns: `src-tauri/src/projects/`, engine composition root
  - Deliverables:
    - Create project directory and `project.duckdb`.
    - Open exactly one active project initially.
    - Close connections before switching/deleting projects.
  - Acceptance: reopen preserves DuckDB tables and operational project metadata.
  - Tests: create, reopen, invalid path, already-open, and close-on-error tests.
  - Commit: `feat(projects): manage local duckdb projects`
  - Implementation commit: `24b0415`

- [x] **E3-T2 Add dedicated DuckDB worker and job protocol** — owner: lead-agent
  - Depends on: E3-T1
  - Owns: `src-tauri/src/engine/worker.rs`, job types
  - Deliverables:
    - DuckDB work runs off Tauri async handlers.
    - Bounded queue and configured worker count.
    - Typed request/response IDs and structured failure translation.
  - Acceptance: long query does not block the UI command loop.
  - Tests: serialization of connection access, queue capacity, worker shutdown.
  - Commit: `feat(engine): add bounded duckdb worker`
  - Implementation commit: `42f3d00`, `b02bd8c`

- [x] **E3-T3 Add engine resource and performance settings** — owner: lead-agent
  - Depends on: E3-T2, E2-T2
  - Owns: engine configuration module
  - Deliverables:
    - Low memory, Balanced, and Fast profiles.
    - Configurable memory limit, thread count, and temporary directory.
    - Validate settings before applying them.
  - Acceptance: settings apply per opened project without raw SQL interpolation hazards.
  - Tests: profile mapping and validation tests.
  - Commit: `feat(engine): add resource profiles`
  - Implementation commit: `42f3d00`

- [x] **E3-T4 Add catalog inspection service** — owner: lead-agent
  - Depends on: E3-T2
  - Owns: engine catalog module and typed commands
  - Deliverables: schemas, tables, views, columns, types, and source-kind metadata.
  - Acceptance: UI can refresh catalog after DDL/import without reopening project.
  - Tests: multiple schemas/tables/views and unusual identifier tests.
  - Commit: `feat(engine): expose project catalog`
  - Implementation commit: `42f3d00`, `24b0415`

- [x] **E3-T5 Complete manual-QA project discovery and native open workflow** — owner: lead-agent
  - Depends on: E3-T1, E3-T4
  - Owns: project manager, recent project commands, native dialog integration, explorer project list
  - Deliverables:
    - Show recent managed and external DuckDB projects after the active project closes.
    - Reopen a recent project from Explorer with one action.
    - Name managed database files from a Windows-safe slug of the project name, with collision-safe project directories.
    - Use a native file-selection dialog filtered to DuckDB file extensions.
    - Keep live imported/opened DuckDB table names and column totals visible.
  - Acceptance:
    - Closing a project does not hide its durable recent-project entry.
    - `Retail Analysis` creates `retail-analysis.duckdb`, while reserved/invalid Windows filename characters are sanitized.
    - Open presents the operating system file picker instead of asking for a pasted path.
    - Opening a populated DuckDB file displays each real table name and column count.
  - Tests: filename sanitization/reserved names, recent reopen, native dialog boundary, populated catalog UI.
  - Commit: `feat(projects): complete native project discovery workflow`
  - Implementation commit: `61de982`

- [x] **E3-T6 Add safe project rename, forget, and delete workflows** — owner: lead-agent
  - Depends on: E3-T1, E3-T5
  - Owns: project ownership metadata, project repository, filesystem lifecycle, project actions UI
  - Deliverables:
    - Record whether a project is Tarik-managed or an externally opened DuckDB file.
    - Rename a managed project display name and its DuckDB filename using the Windows-safe filename rules; update SQLite path metadata transactionally and roll back the filesystem rename if metadata update fails.
    - Rename an external project's Tarik display name without renaming or moving the user-owned DuckDB file.
    - Offer `Delete project` only for Tarik-managed projects, with explicit destructive confirmation and worker shutdown before deleting the managed project directory and related SQLite metadata.
    - Offer `Forget project` for external projects, removing only Tarik metadata while preserving the external DuckDB file.
    - Remove deleted/forgotten projects from Recent projects and select a predictable empty state afterward.
  - Acceptance:
    - An active project connection is closed before any managed file rename or deletion.
    - Managed rename preserves DuckDB contents and reopens from the new filename.
    - Filename collisions, permission failures, locked files, and metadata failures are recoverable and never leave a silently broken recent-project entry.
    - Tarik never deletes, renames, or moves an externally opened DuckDB file through the normal project-management actions.
    - Destructive confirmation states the exact managed project name and path.
  - Tests: managed rename/reopen, display-only external rename, managed delete cascade, external forget preservation, active-worker shutdown, collision, locked/permission failure, rollback.
  - Commit: `feat(projects): add safe rename and delete workflows`
  - Implementation commit: `8379053`

---

## EPIC E4 — Local sources and import workflow

**Status:** `APPROVED` - user authorized E5.

**Outcome:** Beginner-safe Parquet linking and CSV/Parquet imports.

- [x] **E4-T1 Build source inspection boundary** — owner: lead-agent
  - Depends on: E3-T2
  - Owns: `src-tauri/src/sources/inspect.rs`
  - Deliverables:
    - Validate selected path and supported extension.
    - Read CSV/Parquet schema and bounded preview through DuckDB.
    - Return structured parse warnings without loading full files.
  - Acceptance: malformed/unsupported/missing files produce recoverable errors.
  - Tests: fixture matrix for valid and invalid CSV/Parquet.
  - Commit: `feat(sources): inspect local datasets`
  - Implementation commit: `3d7edf9`

- [x] **E4-T2 Link a Parquet file or glob as a named DuckDB view** — owner: lead-agent
  - Depends on: E4-T1, E2-T5, E3-T4
  - Owns: source linking backend and source explorer integration
  - Deliverables:
    - Safe identifier/path handling.
    - Single file and multi-file glob support.
    - Durable source metadata and catalog refresh.
  - Acceptance: linked view can join imported tables; no data is copied.
  - Tests: spaces/quotes in paths, glob, join, duplicate name, missing file.
  - Commit: `feat(sources): link parquet datasets`
  - Implementation commit: `3d7edf9`

- [x] **E4-T3 Build CSV import wizard** — owner: lead-agent
  - Depends on: E4-T1, E1-T2
  - Owns: CSV import UI and options types
  - Deliverables:
    - Preview delimiter, header, nulls, encoding assumptions, and inferred types.
    - Per-column type override and destination table naming.
    - Clear progress, cancellation, and parse-error states.
  - Acceptance: user confirms trusted options before data copy begins.
  - Tests: wizard navigation, overrides, validation, cancellation.
  - Commit: `feat(import): add csv import wizard`
  - Implementation commit: `2eb21b5`

- [x] **E4-T4 Import CSV and Parquet as DuckDB tables** — owner: lead-agent
  - Depends on: E4-T1, E2-T5, E3-T2
  - Owns: source import backend
  - Deliverables:
    - Transactional import with progress events where available.
    - Cleanup incomplete table on error/cancel.
    - Persist source metadata and refresh catalog.
  - Acceptance: imported source remains queryable after original file moves.
  - Tests: import/reopen, cancel cleanup, duplicate table, disk/write failure.
  - Commit: `feat(import): import datasets into duckdb`
  - Implementation commit: `3d7edf9`, `2eb21b5`

- [x] **E4-T5 Detect and repair missing linked sources** — owner: lead-agent
  - Depends on: E4-T2
  - Owns: source health backend/UI
  - Deliverables: missing state, locate replacement, verify compatible schema, remove source.
  - Acceptance: broken source does not prevent unrelated project use.
  - Tests: move, relink, incompatible replacement, remove.
  - Commit: `feat(sources): repair missing linked files`
  - Implementation commit: `3d7edf9`, `2eb21b5`

- [x] **E4-T6 Improve import type overrides and source cardinality summary** — owner: lead-agent
  - Depends on: E4-T1, E4-T3, E4-T4
  - Deliverables:
    - Replace free-text column override fields with a controlled DuckDB type dropdown whose first option keeps the inferred type.
    - Show destination table/view name, total columns, row cardinality, and source file size above the preview.
    - Use exact row counts for Parquet and small CSV files; use a clearly marked estimate for large CSV files to avoid an expensive full pre-import scan.
    - Record the exact imported row count in source metadata after successful import.
    - Format summary counts compactly (`1K`, `1.2M`) while preserving the exact value in accessible text/title where known.
  - Acceptance:
    - Users cannot submit arbitrary type SQL from the wizard.
    - Approximate counts display `~`; exact counts do not.
    - Large CSV inspection remains bounded and does not scan the full file only to populate the summary.
    - Summary refreshes when CSV parsing options change.
  - Tests: dropdown choices/submission, exact Parquet count, exact small CSV count, estimated large CSV count, compact formatting boundaries, exact post-import metadata count.
  - Commit: `fix(import): add controlled types and row summary`
  - Implementation commit: `c355ce2`

- [x] **E4-T7 Show cheap cached/estimated row totals in Explorer** — owner: lead-agent
  - Depends on: E3-T4, E4-T4, E4-T6
  - Deliverables:
    - Display table name, column total, and compact row total without issuing automatic `COUNT(*)` queries.
    - Prefer cached exact row counts recorded after Tarik import or Parquet inspection.
    - Fall back to DuckDB `duckdb_tables().estimated_size` for existing physical tables and prefix estimates with `~`.
    - Keep arbitrary views as `view` without executing them for cardinality.
    - Expose full exact/estimated values in accessible title text.
  - Acceptance: Explorer refresh uses catalog/source metadata only; exact cached values override estimates.
  - Tests: cached exact import count, catalog estimate, unknown view count, compact formatting.
  - Commit: `feat(catalog): show cached row totals`
  - Implementation commit: `d05696d`

---

## EPIC E5 — SQL editor and restorable sessions

**Status:** `APPROVED` - user sign-off received; engine protocol work authorized.

**Outcome:** Fast multi-tab SQL workspace that never loses drafts during normal use.

- [x] **E5-T1 Integrate CodeMirror SQL editor** — owner: lead-agent
  - Depends on: E1-T2, E3-T4
  - Owns: `src/features/editor/`
  - Deliverables:
    - SQL syntax highlighting, line numbers, selection, find, and keyboard run shortcut.
    - Catalog-aware completion for schemas, tables/views, and columns.
  - Acceptance: editor remains responsive with realistically large SQL scripts.
  - Tests: keyboard commands and completion data adapter.
  - Commit: `feat(editor): add codemirror sql workspace`
  - Implementation commit: `fea173b`

- [x] **E5-T2 Implement multi-tab session model** — owner: lead-agent
  - Depends on: E5-T1, E2-T3
  - Owns: editor/session frontend and typed commands
  - Deliverables:
    - Create, rename, reorder, activate, duplicate, and close tabs.
    - Dirty-state indicator and close confirmation when persistence failed.
  - Acceptance: tab identity is stable and active-tab invariant holds.
  - Tests: tab reducer/store transitions and user interactions.
  - Commit: `feat(session): add multi-tab query workspace`
  - Implementation commit: `98eca42`

- [x] **E5-T3 Autosave and restore drafts** — owner: lead-agent
  - Depends on: E5-T2
  - Owns: autosave/restore behavior
  - Deliverables:
    - Debounced draft snapshots with flush on close/window shutdown.
    - Restore tab order, active tab, title, and SQL after restart.
    - Clean fallback if stored session is invalid.
  - Acceptance: rapid edits cannot write older SQL over newer SQL.
  - Tests: debounce ordering, shutdown flush, corrupt-session fallback.
  - Commit: `feat(session): autosave and restore query drafts`
  - Implementation commit: `98eca42`

- [x] **E5-T4 Add SQL-to-source affordances** — owner: lead-agent
  - Depends on: E5-T1, E3-T4
  - Owns: editor/source explorer interactions
  - Deliverables: insert quoted identifier, open table preview query, copy qualified name.
  - Acceptance: generated SQL handles unusual identifiers safely.
  - Tests: identifier quoting and UI actions.
  - Commit: `feat(editor): connect catalog actions to sql tabs`
  - Implementation commit: `98eca42`

---

## EPIC E5.5 — Engine protocol and DuckDB sidecar adapter

**Status:** `APPROVED` (2026-08-30) - T1-T6 implemented; sidecar verified end-to-end by manual review (project open/create, CSV import, Parquet link, clean shutdown). Follow-ups recorded in Remaining E5.5 follow-ups.

**Outcome:** A capability-driven engine protocol with a long-running DuckDB sidecar adapter, so normal Tauri builds exclude database drivers and DuckDB compiles without bundled C++.

**Rationale:** Keep DuckDB as Tarik's SQL engine and Arrow for bounded result interchange, but move them out of the Tauri dependency graph. E6 and later EPICs then build against the engine protocol instead of an embedded connection. This also prepares PostgreSQL/MySQL/BigQuery adapters later without UI rewrites.

**Non-goals here:** remote connectors, credentials, query execution, result-grid UI, query-plan visualization, exports.

- [x] **E5.5-T1 Define versioned engine protocol and capability negotiation** — owner: lead-agent
  - Depends on: E5 (approved)
  - Owns: `crates/engine-protocol/`
  - Deliverables:
    - Handshake with `engineId`, `engineName`, `engineVersion`, `protocolVersion`, `capabilities`.
    - Generic operations: `session.open/close`, `catalog.inspect`, `query.execute/cancel`, `result.get_page/release`, `plan.explain`, `engine.shutdown`.
    - Namespaced engine-specific operations such as `duckdb.source.link_parquet`.
    - Structured error envelopes with stable error codes.
    - Forward compatibility for unknown fields and capabilities.
  - Acceptance: protocol v1 is versioned, capability-driven, and independent of any database crate.
  - Tests: handshake, capability negotiation, unknown-method error, structured error envelope, forward-compatible payloads.
  - Commit: `feat(engine): define engine protocol`
  - Implementation commit: `f98015c`

- [x] **E5.5-T2 Define result-page and export interchange format** — owner: lead-agent
  - Depends on: E5.5-T1
  - Owns: `crates/arrow-page-format/`, engine result writer, bounded page reader
  - Deliverables:
    - Result metadata plus bounded pages; full datasets never cross Tauri IPC.
    - Arrow IPC or Parquet artifacts written by the engine and read in bounded pages.
    - Explicit `result.release` lifecycle and cache-directory ownership.
    - CSV/Parquet export writers over the same batch stream.
  - Acceptance: E6 result browsing and E9 export consume one bounded interchange format.
  - Tests: multi-batch pagination, batch split, page spill, release cleanup, export boundary cases.
  - Commit: `feat(engine): define bounded result interchange`
  - Implementation commit: `f98015c`

- [x] **E5.5-T3 Add engine manager and protocol client in Tauri** — owner: lead-agent
  - Depends on: E5.5-T1
  - Owns: `crates/engine-client/`, Tauri engine-manager module
  - Deliverables:
    - Start the configured engine binary, handshake, health check, and shutdown.
    - Typed request/response client with request IDs and cancellation.
    - Capability-driven command surface in the frontend.
    - Project `engine_id` + `locator_json` in SQLite via migration.
    - At most one active engine session per project.
  - Acceptance: normal Tauri builds and tests do not depend on DuckDB or Arrow crates.
  - Tests: fake-engine process for client, process lifecycle, request IDs, shutdown, project locator migration.
  - Commit: `feat(engine): add tauri engine client`
  - Implementation commit: `f6357f4`

- [x] **E5.5-T4 Port DuckDB lifecycle and sources to a sidecar adapter** — owner: lead-agent
  - Depends on: E5.5-T2, E5.5-T3
  - Owns: `engines/duckdb/`
  - Deliverables:
    - Move E3 project lifecycle, E4 inspection, link, import, repair, and catalog logic into the DuckDB adapter.
    - Preserve identical behavior and error messages for existing E3/E4 tests.
    - DuckDB adapter uses a pinned prebuilt dynamic library rather than `bundled` C++ compilation.
    - Engine discovery by manifest (id, executable, protocol version, display name).
  - Acceptance: existing E3/E4 workflows pass end-to-end through the sidecar; Tauri no longer compiles DuckDB.
  - Tests: adapter unit tests, sidecar integration, E3/E4 workflow parity, missing engine, engine crash, restart.
  - Commit: `feat(engine): port duckdb lifecycle to sidecar`
  - Implementation commit: `f6357f4`

- [x] **E5.5-T5 Measure build, runtime, and packaging impact** — owner: lead-agent
  - Depends on: E5.5-T3, E5.5-T4
  - Owns: build benchmark, portable packaging preparation
  - Deliverables:
    - Compare clean/warm Tauri build, engine build, binary size, startup, and memory before/after.
    - Define engine binary placement and runtime library discovery for Linux and Windows portable archives.
    - Record DuckDB dynamic-library provenance and checksum.
  - Acceptance: clean Tauri build excludes DuckDB/Arrow compile; the engine builds separately and packages with a pinned prebuilt library.
  - Tests: reproducible benchmark script, packaged-engine smoke test, checksum verification.
  - Commit: `perf(engine): benchmark sidecar architecture`
  - Implementation commit: `pending doc commit`

- [x] **E5.5-T6 Add engine protocol documentation and review gate** — owner: lead-agent
  - Depends on: E5.5-T1, E5.5-T4, E5.5-T5
  - Owns: docs and manual review packet
  - Deliverables: protocol reference, capability matrix, adapter authoring guide, build/run instructions, manual QA checklist.
  - Acceptance: a new engine adapter can be added without editing Tarik's core or result UI.
  - Tests: manual review plus golden workflow through the sidecar.
  - Commit: `docs(engine): document engine protocol and adapters`
  - Implementation commit: `pending doc commit`

---

## EPIC E6 — Query execution and bounded result browsing

**Status:** `PROVISIONALLY APPROVED` (2026-08-31) - user authorized E7 to proceed after validating query results, table queries, column rendering/resizing, catalog auto-refresh, and table/link removal. E6 remains reopenable for review fixes until final release acceptance.

**Outcome:** Cancellable execution with large-result browsing that has a defined memory ceiling.

**Preparation:** See `docs/design/E6-DESIGN-GRAPH.md` for the data-flow graph, concurrency decision, boundedness contract, lifecycle scope, UI states, test layers, and implementation order. The E5.5 protocol/page metadata exists, while E6-T3 owns the concrete streaming page writer/reader and cache behavior.

- [x] **E6-T1 Define typed query execution lifecycle** — owner: lead-agent
  - Depends on: E5.5-T1, E5.5-T3, E2-T4
  - Owns: `src-tauri/src/query/`, shared frontend command types
  - Deliverables:
    - Execute immutable SQL snapshot with project/tab/execution IDs through the engine client.
    - Add an asynchronous engine job registry so `query.execute` returns without blocking protocol framing or cancellation.
    - Queued/running/succeeded/failed/cancelled events.
    - Structured engine error location/message where available.
  - Acceptance: every terminal execution state creates one durable history entry.
  - Tests: state machine, engine-client integration, history integration.
  - Commit: `feat(query): add typed execution lifecycle`
  - Notes:
    - Engine: `engines/duckdb/src/jobs.rs` job registry (one worker thread per execution, per-session queue); `query.execute` returns `queued` immediately; `query.status` reports state/duration/rows. Multi-statement snapshots split on top-level semicolons (`src/sql.rs`); DuckDB one-row DML `Count` results are surfaced as `rowsAffected` rather than a browsable result; every statement streams via `stream_arrow`, so no result is materialized up front. DuckDB reports fetch failures (including interrupt) as panics inside its iterator, so job execution runs under `catch_unwind` and a requested cancel becomes a clean cancelled terminal.
    - Desktop: `src-tauri/src/query/` coordinator observes engine job states with short polls (150 ms) and writes exactly one `query_history` row per terminal state (`history_written` guard). The `EngineExecutor` trait isolates the coordinator for scripted tests.
    - Frontend: `useQueryExecution` hook keeps lightweight per-tab state, polls every 250 ms, and the results panel shows empty/queued/running/failed/cancelled/completed states plus a Cancel action. The static fake fixture (`24,318` rows) is removed. Each successful terminal execution refreshes project catalog/source metadata exactly once, so schema-changing SQL (CREATE/DROP/ALTER/view changes) appears in Explorer without manual refresh; failed/cancelled queries do not refresh.
    - Engine status currently reports `rowsAffected` from DuckDB DML count results; `rows_produced` while running reflects batches already fetched. Engine-side cancel tests land with E6-T2.
    - Post-review fix `066e175`: `mark_terminal` now checks `history_written` before insertion, rejected submissions do not start a status poller, and late non-terminal polls cannot roll terminal state backwards. A regression test proves duplicate terminal observations persist exactly one history row.
    - Post-review fix `8f3807e`: `EngineManager::execute_query` now uses `session_request` so the active `sessionId` reaches the sidecar; the Tauri command derives and validates the backend active project instead of trusting frontend identity; the coordinator rejects unknown projects before submission/history persistence; SQLite history inserts are idempotent (`ON CONFLICT(id) DO NOTHING`). Integration/unit tests cover active-session injection, missing-project rejection, and duplicate storage writes.

- [x] **E6-T2 Implement query cancellation and cleanup** — owner: lead-agent
  - Depends on: E6-T1, E5.5-T3
  - Owns: query cancellation backend/UI
  - Deliverables:
    - Cancel queued or active query through the engine client.
    - Engine adapter interrupts its session safely.
    - Release cursor/cache on cancellation.
  - Acceptance: cancelled engine session remains usable for a later query.
  - Tests: cancel queued, active, already-finished, and repeated cancellation.
  - Commit: `feat(query): support safe cancellation`
  - Notes:
    - `query.cancel` removes queued jobs immediately; running jobs are interrupted through DuckDB's thread-safe `InterruptHandle`. An interrupt surfaces either as a panic in the Arrow fetch path or as a DuckDB `INTERRUPT` error; the worker maps both to a clean cancelled terminal when a cancel was requested, and any unexpected panic becomes `query.interrupted`.
    - `session.close` first cancels the session's queued/running jobs so closing cannot strand work. Result-page cleanup on cancellation lands with E6-T3, which owns page artifacts.
    - Integration tests cover queued-cancel removal, active interrupt with the session remaining usable afterwards, already-finished cancel, and repeated cancellation.

- [x] **E6-T3 Wire bounded result paging through the engine client** — owner: lead-agent
  - Depends on: E6-T1, E5.5-T2, E0-T4
  - Owns: `src-tauri/src/results/`, bounded page reader
  - Deliverables:
    - Implement concrete streaming Arrow IPC page writing/reading behind the E5.5 interchange types; never collect all record batches.
    - Consume Arrow IPC/Parquet pages produced by the engine.
    - Bounded page cache with configurable maximum and spill to cache directory.
    - Result metadata returned separately from page data.
    - Explicit result release command.
  - Acceptance: backend never collects the full result solely for UI display.
  - Tests: multi-batch query, eviction, spill/readback, close cleanup, query error.
  - Commit: `feat(results): add bounded result paging`
  - Notes:
    - Engine (`engines/duckdb/src/pages.rs`): the job worker streams record batches into Arrow IPC page files of at most 500 rows and roughly 4 MiB (rows per page shrink for wide rows). Pages are written to `<cacheDir>/<executionId>.tmp/` and atomically published by rename; failure/cancel discards temporary directories. `result.get_page` reads the page files covering the requested window, stitches pages, and converts cells to JSON-safe values (JS-safe numbers, booleans, strings; >64 KiB strings truncate with `truncatedCells`; decimals/date-times/binary/nested become display strings; integers beyond 2^53 cross as strings). `result.release` deletes the directory and registry entry. Arrow stays inside the engine process; the desktop receives JSON only.
    - Desktop: `src-tauri/src/results/` proxies page reads behind `ResultStore`, a bounded LRU (12 decoded pages across results) with aligned offsets and release-driven eviction. The execution view now carries `resultId`/`rowTotal` from engine result metadata.
  - Acceptance: backend never collects the full result solely for UI display.
  - Tests: multi-batch query, eviction, spill/readback, close cleanup, query error.
  - Commit: `feat(results): add bounded result paging`

- [x] **E6-T4 Build virtualized result grid** — owner: lead-agent
  - Depends on: E6-T3, E1-T2
  - Owns: `src/features/results/`
  - Deliverables:
    - Row and column virtualization using `@tanstack/react-virtual` (dependency added only when this task starts).
    - Typed formatting for null, boolean, numeric, date/time, binary, and nested values.
    - Copy cell/row/selection and visible loading/error states.
  - Acceptance: DOM size remains bounded while browsing a large fixture.
  - Tests: virtualization adapter, type formatting, keyboard navigation.
  - Commit: `feat(results): add virtualized data grid`
  - Notes:
    - `src/features/results/ResultGrid.tsx`: row and column virtualization over engine pages; only visible rows/columns render, so DOM stays bounded for a 24k-row result and any column count. Page navigation (Previous/Next + PageUp/PageDown), arrow-key row navigation with scroll-into-view, and Ctrl/Cmd+C page copy are wired. Null cells render a dim NULL marker; truncated cells (>64 KiB) show an ellipsis; numbers render with tabular numerals.
    - `src/features/editor/` results panel now renders the grid for successful row-returning executions; DML completions keep the concise completion message.
    - jsdom cannot measure layout, so the bounded-DOM test asserts the virtualizer's initial overscan window (<30 rows) for a 500-row page of a 24,318-row result; manual review will confirm large-fixture behavior. Release actions land with E6-T5.
    - Post-review fix `876ec03`: `ResultGrid` passed the entire page to `parseColumns` instead of `page.columns`, so the toolbar count appeared but the grid had only the 68px row-number column. The grid now parses the real column array, fixes header/viewport geometry, and renders a bounded 16-row first-paint fallback while ResizeObserver initializes. Regression asserts headers and real cell values render.
    - Post-review enhancement: result columns are individually resizable from the header edge (80-640 px), keyboard-accessible with Left/Right on the separator, double-click resets to 150 px, and TanStack column measurements/offsets update without disabling virtualization. Widths reset for each new result schema; global drag listeners/cursor are structurally cleaned up.
    - Post-review Explorer enhancement: catalog context menus expose `Delete table` for local/imported tables, `Remove link` for linked Parquet views, and `Delete view` for other views. Destructive actions require confirmation; source files are explicitly preserved. The backend validates the fully qualified database/schema/name and current object kind against a fresh catalog snapshot, quotes every identifier in the engine, drops the DuckDB relation first, then removes matching source metadata and refreshes Explorer.

- [x] **E6-T5 Add result lifecycle and memory instrumentation** — owner: lead-agent
  - Depends on: E6-T3, E6-T4
  - Owns: result status UI/backend metrics
  - Deliverables:
    - Display loaded page range, total/unknown count, cache use, and released state.
    - Release superseded results and all results on project close.
  - Acceptance: repeated run/close cycle does not leak open cursors or cache directories.
  - Tests: lifecycle stress integration test.
  - Commit: `fix(results): enforce bounded result lifecycle`
  - Notes:
    - The desktop passes the app cache root to `query.execute`, so page artifacts always live under `<cacheDir>/results/<executionId>/`. Startup cleanup removes stale artifact directories from a previous session; a new run in the same tab releases the superseded result before submitting; project close releases every known result via `release_all_results`.
    - The grid toolbar shows the loaded row window, page x of y, and total count (exact from streaming, never a COUNT(*) query); released results surface as a structured `result.missing` error state.
    - Lifecycle stress integration test: three run/release cycles with 1,200-row results plus a final run, asserting page reads at offset 1,000, missing-after-release, session usability, and exactly one artifact directory remaining.

---

## EPIC E7 — Beginner-friendly query flow

**Status:** `PROVISIONALLY APPROVED` (2026-09-02) - user authorized E8 progression and deferred final E7 review. E7 remains reopenable for graph/inspector/Estimate/Actual Flow fixes until final release acceptance.

**Outcome:** Explain and Profile plans become understandable node graphs.

**Preparation:** See `docs/design/E7-DESIGN-GRAPH.md` for native DuckDB JSON formats, normalized graph boundaries, Explain-vs-Profile semantics, fallback rules, scope, test layers, and implementation order.

- [x] **E7-T1 Capture stable DuckDB Explain/Profile fixtures** — owner: lead-agent
  - Depends on: E6-T1, E5.5-T4
  - Owns: plan fixtures and compatibility notes
  - Deliverables:
    - Fixtures for scan, pushed filter, projection, join, aggregate, sort, limit, union, CTE, and window.
    - Record supported DuckDB version and fallback expectations.
  - Acceptance: fixtures contain no machine-specific absolute paths.
  - Tests: fixture loading/validation.
  - Commit: `test(plan): add duckdb plan fixtures`
  - Notes:
    - Captured from pinned DuckDB 1.5.5: `EXPLAIN (FORMAT JSON)` physical trees (`name`, `children`, `extra_info`) and `EXPLAIN (ANALYZE, FORMAT JSON)` profile trees (`operator_name/type`, actual cardinality, timing, rows scanned, children).
    - Fixtures cover scan, pushed filter/projection, hash join, aggregate, sort/limit (`TOP_N`), union, CTE, and window; database qualifiers are sanitized to `fixture`, volatile profile timing/query/memory values are zeroed, and no machine paths remain.
    - `manifest.json` pins version/format and records raw JSON/text preservation as the fallback. Mechanical tests validate JSON, one connected Explain root, profile connectivity/metrics, required operator families, manifest completeness, and path sanitization.

- [x] **E7-T2 Parse DuckDB plans into a normalized graph** — owner: lead-agent
  - Depends on: E7-T1, E5.5-T4
  - Owns: `src-tauri/src/plan/`
  - Deliverables:
    - Stable `QueryPlan`, `PlanNode`, and edge types.
    - Preserve unknown operators rather than dropping them.
    - Fallback to raw textual plan if structured parsing fails.
  - Acceptance: all fixture operators produce connected, deterministic graphs.
  - Tests: golden parser tests and malformed-plan fallback.
  - Commit: `feat(plan): normalize duckdb query plans`
  - Notes:
    - `src-tauri/src/plan/` captures native JSON through the existing async engine lifecycle (`EXPLAIN (FORMAT JSON)` or explicit `EXPLAIN (ANALYZE, FORMAT JSON)`), reads the one-row payload, and structurally releases its temporary result pages on decode success/failure.
    - Explain/Profile have explicit parsers behind stable `QueryPlan`, `PlanNode`, `PlanEdge`, and `PlanMode`. IDs/edges are deterministic preorder; edges flow from child/input to parent/result. Explain preserves estimated cardinality; Profile preserves actual cardinality, milliseconds, rows scanned, and native details.
    - Common native operators normalize to scan/join/aggregate/filter/projection/sort/limit/union/window/result. Unknown names are kept as `unknown` with native name/details. Invalid/malformed JSON returns an empty normalized graph plus `rawPlan` and `fallbackReason`, never drops the source.
    - Tests cover every fixture, deterministic connectivity, two-input joins, profile metrics, unknown operator preservation, malformed fallback, a real sidecar Explain/release roundtrip, and the typed frontend command.

- [x] **E7-T3 Build XYFlow query graph** — owner: lead-agent
  - Depends on: E7-T2, E1-T2
  - Owns: `src/features/query-flow/`
  - Deliverables:
    - Directed source-to-result layout, fit view, zoom, pan, minimap only if useful.
    - Nodes show operation, source, estimate/actual rows, and timing where available.
    - Loading, empty, unsupported, and error states.
  - Acceptance: two-input joins visibly converge into one join node.
  - Tests: graph mapping, selection, keyboard navigation, snapshot fixtures.
  - Commit: `feat(flow): visualize query execution plans`
  - Notes:
    - Added official `@xyflow/react`. Deterministic layout puts source/input depth on the left and result/root on the right; nodes at each depth retain normalized preorder. Two scan inputs receive distinct rows and converge into one join. Pan/zoom/fit and restrained controls are enabled; minimap is intentionally omitted at current graph sizes.
    - Review fix: persistent connector strokes now use the defined `--border-control` token (the original undefined `--border-strong` token made edges invisible). Arrowheads show source-to-result direction. A one-shot depth-staggered accent pulse and brief node-arrival border cue communicate traversal; reduced-motion keeps static connectors and suppresses motion.
    - Estimate and Actual Flow are editor-toolbar actions beside Query Library and open the same viewport-sized three-pane analysis workspace: immutable planned/profiled SQL on the left, graph in the center, and operation explanation/metrics on the right. The bottom output area contains Results only. Node selection highlights conservative SQL ranges in the analysis snapshot; current editor changes mark analysis stale until explicit rebuild/rerun.
    - A single `Present` graph action deterministically selects the upper-left source node, opens its inspector, and focuses it at 1-1.35× zoom with a smooth 650ms fit transition. Multi-source joins use topmost then stable node ID as tie-breakers. Reduced-motion uses an instant transition. Present works in both Estimate and Actual Flow workspaces.
    - Beginner-facing tabs are `Estimate` (DuckDB Explain) and `Actual Flow` (DuckDB Profile), while persisted/internal mode keys remain compatible. Node cards show beginner operation label, native operator, source, estimated or actual output rows, and explicit `Operator time` in Actual Flow. Estimate says row counts are DuckDB planning guesses and labels cards `DuckDB estimate · ~N output rows`; Actual Flow says row counts are measured during execution and labels cards `Actual output · N rows`.
    - Final deferred-review interaction: Run query executes current SQL exactly once and stays on Results. Toolbar order is Run query | Query Library | Estimate | Actual Flow. Estimate explicitly captures current immutable SQL without execution; Actual Flow remains explicit because it executes SQL. Both open the shared analysis workspace, retain independent immutable snapshots, and show stale state after editor changes.
    - Loading skeleton, empty actions, structured error, raw structured-fallback, and ready graph states are covered. Tests verify deterministic join convergence/layout, Explain wiring, fallback UI, and accessible Flow/Profile empty states.

- [x] **E7-T4 Add beginner explanations and node inspector** — owner: lead-agent
  - Depends on: E7-T3
  - Owns: flow explanation dictionary and inspector UI
  - Deliverables:
    - Plain-language explanations for common operators.
    - Join type/keys, group keys, filters, projections, sort keys, and source details.
    - Explain estimated values vs Profile actual values are clearly labeled.
  - Acceptance: unknown operators show truthful generic details, not invented explanations.
  - Tests: explanation mapping and inspector states.
  - Commit: `feat(flow): explain query nodes for beginners`
  - Notes:
    - Explanation dictionary covers scan, filter, projection, join, aggregate, sort, limit, union, window, and result in plain functional language with truthful input/output descriptions.
    - Selecting a node opens the inspector with explicit `Estimated operation` vs `Actual operation`, source, estimated output, actual output, operator timing, rows scanned, and ordered native details (join type/conditions, filters, groups/aggregates, projections, order/top, table/type). Flow includes a prominent `Planning estimate, not result count` note explaining that DuckDB's guess neither limits nor describes actual results; projection additionally explains why it normally repeats its input row estimate.
    - Profile nodes with both cardinalities show `Est. ~N · Actual N` and a symmetric error-factor badge (`max(est/actual, actual/est)`): green below `10×`, yellow from `10×` through `100×`, red above `100×`; zero mismatches show `∞×`. Text labels distinguish over- vs under-estimates, and the inspector warns this measures estimate accuracy—not query speed—while pointing to operator time and rows scanned for performance.
    - Unknown operators retain their native title/details and explicitly say no verified beginner explanation exists; no guessed semantics. Profile normalization collapses DuckDB `QUERY`/`EXPLAIN_ANALYZE` administrative wrappers into exactly one Query result boundary. A filter explicitly reported inside a scan is expanded into Read data → Filter rows, with rows scanned and combined operator time retained on Read while post-filter estimated/actual cardinality moves to Filter. The inspector discloses that this is a beginner presentation of fused native work.
    - `Return columns` replaces the ambiguous `Choose columns` label. Actual Flow shows a mutation warning unless SQL is exactly one clearly read-only SELECT/VALUES/SHOW/DESCRIBE statement. Tests cover every common operator, unknown truthfulness, deterministic expansion/rewiring, one result boundary, warning semantics, detail ordering, empty inspector, node selection, estimated labeling, operator time, and metric display.

- [x] **E7-T5 Link flow nodes to relevant SQL when reliably available** — owner: lead-agent
  - Depends on: E7-T4
  - Owns: editor/flow selection bridge
  - Deliverables: selecting a node highlights related SQL; unsupported mappings do nothing harmful.
  - Acceptance: feature is presented as best-effort and never highlights a knowingly wrong range.
  - Tests: mapping fixtures and unsupported cases.
  - Commit: `feat(flow): connect plan nodes to sql ranges`
  - Notes:
    - Conservative tokenizer skips SQL strings, line comments, nested block comments, and preserves exact source ranges. It maps only unique, mechanically reliable constructs: relation token after `FROM`/`JOIN`, `WHERE`, `JOIN`, `GROUP BY`, `ORDER BY`, `LIMIT`, `UNION`, and `OVER`.
    - Ambiguous duplicate constructs, unsupported projection/result operators, missing source matches, and stale/edited SQL return no range and clear any prior highlight. Highlights are bound to the active tab and immutable SQL snapshot that produced the plan.
    - CodeMirror now synchronizes external tab SQL value changes (a pre-existing controlled-editor defect exposed by this bridge), applies a restrained plan-range decoration, and selects the mapped text. Tests cover all supported constructs, quoted qualified sources, strings/comments, duplicate ambiguity, unsupported operators, end-to-end node click highlighting, and clearing on an unmappable node.

---

## EPIC E8 — Saved queries and historical executions

**Status:** `APPROVED` (2026-09-03) - user accepted saved-query persistence, project-scoped history, reopen-without-execution behavior, and retention/clear isolation. E9 is ready but not started.

**Outcome:** Durable query library and useful local audit trail.

**Preparation:** See `docs/design/E8-DESIGN-GRAPH.md` for explicit create/update semantics, folder behavior, bounded history pages, reopen-without-execution boundary, retention isolation, UI states, and test layers.

- [x] **E8-T1 Implement saved-query service and UI** — owner: lead-agent
  - Depends on: E2-T4, E5-T2
  - Owns: `src/features/saved-queries/`, saved query commands
  - Deliverables:
    - Save current tab, save as, open, rename, move folder, tag, search, and delete.
    - Prevent accidental overwrite with explicit update semantics.
  - Acceptance: saved query survives restart and opens into a new/existing tab predictably.
  - Tests: CRUD, search, overwrite confirmation, folder behavior.
  - Commit: `feat(queries): add saved query library`
  - Notes:
    - Replaced user-facing generic upsert with explicit project-scoped create/update/delete commands. Create rejects case-insensitive duplicate names; update requires an existing ID and project; names/SQL/tags normalize at the repository boundary. Search covers name, SQL, and tags.
    - Added project-scoped folder create/list/rename/delete. Folder deletion preserves contained saved queries through SQLite `ON DELETE SET NULL` (Unfiled); invalid/cross-project folder references are rejected.
    - Query Library dialog sits in the editor toolbar with bounded two-pane Saved queries UI, search, save-current-as-new, explicit edit/update, tags, folder movement, SQL preview, and destructive confirmations. Replacing stored SQL requires confirmation. Opening a saved query creates a predictably named dirty editor tab and has no execution call path.
    - Tests cover normalized CRUD, duplicate prevention, tag search, folder rename/delete-to-Unfiled, project isolation, typed command shapes, dialog interactions, overwrite confirmation, and workspace reopen-without-execution.

- [x] **E8-T2 Build query-history service and UI** — owner: lead-agent
  - Depends on: E6-T1, E2-T4
  - Owns: `src/features/history/`, history commands
  - Deliverables:
    - Filter by project, status, text, and time.
    - Show SQL snapshot, timestamps, duration, row count, and error summary.
    - Reopen historical SQL without automatically executing it.
  - Acceptance: success, failure, and cancellation all appear once and in correct order.
  - Tests: filtering, pagination, reopen, and terminal-state deduplication.
  - Commit: `feat(history): add historical query browser`
  - Notes:
    - Added project-scoped `QueryHistoryFilter/Page` with status, SQL/error text, inclusive ISO timestamp range, offset, and clamped 1-100 limit. Repository fetches `limit + 1` for bounded `nextOffset`, orders by terminal timestamp then ID descending, and keeps the legacy bounded list adapter for E6 coordinator/tests.
    - Query Library History view uses 25-row pages with status/search/from/to filters, Previous/Next, terminal status text badges, SQL snapshot, executed timestamp, duration, returned rows, and structured error detail. Empty/loading/error states are bounded within the dialog.
    - Reopen creates a named dirty editor tab and closes the library; no execution call is reachable. Existing E6 tests continue proving succeeded/failed/cancelled terminal writes are exactly once. New tests cover backend filter/pagination boundaries, typed command shape, UI filtering/paging/error detail, and workspace reopen-without-execution.

- [x] **E8-T3 Add configurable retention and clearing controls** — owner: lead-agent
  - Depends on: E8-T2
  - Owns: history settings/UI and pruning command
  - Deliverables: retention by age/count, manual clear with confirmation, transactional pruning.
  - Acceptance: clearing history never deletes saved queries or SQL drafts.
  - Tests: retention boundary and isolation tests.
  - Commit: `feat(history): manage local history retention`
  - Notes:
    - Added explicit `HistoryRetentionPolicy` (optional max count and max age days) plus `HistoryPruneSummary`. One SQLite `IMMEDIATE` transaction applies age cutoff first, then deterministic newest-N retention, counts remaining rows, and commits atomically. Empty policy is rejected; zero is a valid explicit boundary.
    - Clear history is a separate project-scoped immediate transaction. Both repository tests and command boundaries prove they touch only `query_history`: saved queries survive, another project's history survives, and editor session drafts have no reachable deletion path.
    - History UI exposes Retention and Clear history controls with count/age fields, explicit scope/irreversibility confirmation, inline validation/error, and deleted/remaining summaries. Transactions force a page refresh after completion. Tests cover cancel/accept, policy shape, clear isolation messaging, fixed-clock age/count boundaries, and typed commands.

---

## EPIC E9 — Streaming chunked exports

**Status:** `IN PROGRESS` - E9 preparation/design complete; E9-T1 is active.

**Outcome:** Exact row-count CSV/Parquet chunks with progress, cancellation, and safe partial failure.

**Preparation:** See `docs/design/E9-DESIGN-GRAPH.md` for adapter-neutral option/status shapes, one-pass Arrow batch flow, exact part boundaries, staged-file publication, cancellation/partial-failure policy, ownership boundaries, and test layers.

- [x] **E9-T1 Define and validate export options**
  - Depends on: E6-T1, E5.5-T2
  - Owns: export domain types and validation
  - Deliverables:
    - Format, output directory, base name, rows per part, overwrite policy.
    - CSV delimiter/header options and Parquet compression option.
    - Safe naming and positive row-limit validation.
    - Explicit CSV and Parquet export support.
  - Acceptance: invalid options fail before query execution or file creation.
  - Tests: validation and filename sequence tests.
  - Commit: `feat(export): define chunk export options`
  - Notes:
    - Added one adapter-neutral serialized contract in `tarik-engine-protocol`: CSV/Parquet format, fail-if-exists/replace policy, CSV delimiter/header, and closed Parquet compression variants. `ValidatedExportOptions` is constructible only through the consuming validator.
    - Validation trims and canonicalizes an existing absolute output directory without creating it; restricts portable base names to 128 bytes of ASCII letters/digits/hyphens/underscores; bounds rows per part to `1..=i64::MAX`; rejects cross-format options; and requires one safe ASCII CSV delimiter byte.
    - Deterministic filenames use `<base>-part-00001.<csv|parquet>` with checked one-based numbering and stable expansion beyond five digits. Tests cover wire variants, normalization, invalid directories without creation, unsafe names, row bounds, delimiters, cross-format options, and filename sequences.

- [x] **E9-T2 Implement one-pass exact row chunk writer**
  - Depends on: E9-T1, E5.5-T2
  - Owns: `src-tauri/src/export/`, engine batch stream
  - Deliverables:
    - Execute once, stream batches from the engine, split crossing batches, rotate files at exact row count.
    - Never use repeated `LIMIT/OFFSET` queries.
    - CSV header in every part when enabled.
    - Deterministic `part-00001` naming.
  - Acceptance: every non-final part has exactly the requested number of rows.
  - Tests: zero rows, exact boundary, boundary+1, many batches, batch larger than chunk, CSV/Parquet readback.
  - Commit: `feat(export): stream exact row chunks`
  - Notes:
    - Added engine-side Arrow 58 CSV and Parquet writers beside DuckDB's streamed record batches. SQL executes once; prior statements are drained in order and only the final row set is exported, matching last-result semantics without `COUNT(*)`, `LIMIT`, `OFFSET`, result pages, or desktop row IPC.
    - `ChunkedExportWriter` slices batches at remaining part capacity. Every non-final part has exactly `rowsPerPart`, exact boundaries create no empty trailing file, and zero rows create zero files. CSV writes the configured header in every part; Parquet uses bounded 4 MiB row groups and configurable uncompressed/Snappy/Gzip/Zstd encoding.
    - Each part writes to an engine-generated hidden create-new stage, closes before publication, and returns a bounded path/rows/bytes summary. Fail-if-exists preserves collisions; replace publishes only a completed stage. RAII removes incomplete stages. CSV/Parquet readback tests cover zero, exact, boundary+1, one large batch, many batches, custom delimiter/header, collision/replace, row order, and one-shot multi-statement behavior.

- [x] **E9-T3 Add export progress, cancellation, and partial-failure policy**
  - Depends on: E9-T2, E2-T5
  - Owns: export worker events, history integration
  - Deliverables:
    - Rows/files/bytes written, elapsed time, current part.
    - Cancellation closes files and reports completed parts.
    - Failed/incomplete current part is removed or marked clearly; completed parts stay valid.
  - Acceptance: worker and project remain usable after cancel/disk failure.
  - Tests: cancellation, permission failure, simulated disk full, cleanup.
  - Commit: `feat(export): add resilient export lifecycle`
  - Notes:
    - Added asynchronous sidecar `export.execute/status/cancel` methods with one FIFO worker per engine session, keeping the protocol loop responsive and bounding concurrent Arrow writers. Status exposes exact rows/files/bytes, elapsed time, current part, structured error, and at most 100 recent completed-part summaries; terminal jobs are bounded to 32.
    - Running cancellation installs DuckDB's interrupt handle and also checks between streamed batch slices. Queued cancellation removes work before it starts; repeats are idempotent; session close requests all export cancellation. Panic, DuckDB, validation, collision, writer, and filesystem failures become structured terminal states without poisoning the session.
    - Hidden current stages are removed by RAII on cancel/failure, while published parts remain valid. Deterministic observer tests simulate disk-full and cancellation after one completed part; protocol tests cover preflight with no SQL/files, active/queued cancellation, no stage leaks, terminal idempotence, and session reuse.
    - Added desktop `ExportCoordinator` with active-project ownership checks, canonical preflight, bounded polling, and exactly-once immutable terminal SQLite history. Migration 7 stores SQL/options, duration, exact counters, structured error code/message, and completed parts. The prior frontend-writable export-history mutation command was removed.

- [x] **E9-T4 Build export dialog and completion summary**
  - Depends on: E9-T1, E9-T3, E1-T1
  - Owns: `src/features/export/`
  - Deliverables: accessible options form, progress view, cancel, completed parts, reveal output location.
  - Acceptance: long operations show continuous non-blocking feedback.
  - Tests: validation, progress events, cancel, success, partial failure.
  - Commit: `feat(export): add chunk export workflow`
  - Notes:
    - Added an editor-toolbar Export action after Actual Flow. The controlled Radix dialog is a compact single-page workflow: immutable SQL preview and options, then queued/running progress, then success/failure/cancel summary. Closing the dialog does not cancel active work; polling continues with structurally cleared/retried timers.
    - Options cover native folder selection, portable base name, positive rows per part, stop-or-replace collision policy, CSV delimiter/header, and Parquet compression (Snappy/Zstandard/Gzip/uncompressed). Frontend field checks are repeated by desktop and engine validators before work. Potentially mutating, `WITH`, unknown, or multi-statement SQL requires confirmation because export executes it once.
    - Progress uses exact rows/files/bytes and elapsed time without a decorative progress bar. Terminal states distinguish zero rows/no files, full success, failure, and cancellation; partial outcomes disclose that completed files remain valid and the incomplete stage was removed. Completed parts are shown in a bounded scroll area with Reveal output through the official Tauri opener plugin.
    - Responsive rules preserve single-line toolbar labels at narrow windows and collapse the dialog to one column below 680px. Tests cover disabled/preflight, canonical CSV/Parquet command shapes, polling while closed, cancellation, successful parts/reveal, zero rows, mutating SQL confirmation, and partial failure disclosure.

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
  - Depends on: E5.5-T2, E6-T3, E9-T3
  - Owns: cache cleanup service
  - Deliverables:
    - Startup cleanup for abandoned result cache entries.
    - Age/size limits and explicit clear-cache action.
    - Never delete user-completed exports.
  - Acceptance: cleanup is constrained to Tarik-owned cache/staging directories.
  - Tests: path safety, stale/fresh distinction, partial export handling.
  - Commit: `chore(storage): clean stale temporary artifacts`

- [ ] **E10-T4 Add graceful application shutdown coordinator**
  - Depends on: E5.5-T3, E5-T3, E6-T2, E9-T3
  - Owns: app shutdown composition
  - Deliverables:
    - Flush current drafts.
    - Cancel/finish active jobs according to explicit policy.
    - Stop engine processes, then close files, cursors, SQLite, and logger in order.
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

- [~] **E11-T2 Establish performance and memory budgets** — build-loop optimization foundation in progress
  - Depends on: E6-T5, E9-T3
  - Owns: benchmark harness and performance docs
  - Deliverables:
    - Maintain a project-local stable Linux development profile using Clang, mold, and sccache; document warm/cold measurements and optional Bacon/nextest workflow without changing release codegen.
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

## EPIC E12 — Windows portable application support

**Outcome:** Signed or checksummed Windows x64 portable ZIP that runs without an installer and keeps user data in normal Windows application-data directories.

**Portable definition:** The user downloads a ZIP, extracts it, and launches `Tarik.exe` without administrator rights or an installer. Tarik operational metadata remains in Windows AppData by default. User DuckDB, CSV, Parquet, and export files remain where the user chooses. A fully self-contained mode that writes metadata beside the executable is explicitly out of scope unless requested later.

- [ ] **E12-T1 Add Windows x64 compile and test CI**
  - Depends on: E3
  - Deliverables: `windows-latest` checks for frontend, Rust, bundled DuckDB, and Tauri build using `x86_64-pc-windows-msvc`.
  - Acceptance: every pull request proves the current source compiles and tests on Windows.
  - Commit: `ci(windows): add native windows build checks`

- [ ] **E12-T2 Make development and filesystem boundaries cross-platform**
  - Depends on: E12-T1
  - Deliverables: replace Bash-only reset workflow with cross-platform tooling; test drive-letter, spaces, Unicode, long paths, UNC paths, read-only files, and file-lock errors.
  - Acceptance: Windows development and all local file workflows require no Unix compatibility layer.
  - Commit: `fix(platform): support windows paths and development`

- [ ] **E12-T3 Configure portable Windows release artifact**
  - Depends on: E11-T3, E12-T2
  - Deliverables: release-mode `Tarik.exe`, required runtime files, licenses, README, and checksums packaged as `Tarik-<version>-windows-x64-portable.zip`.
  - Acceptance: archive runs after extraction on a clean supported Windows machine without installation or administrator access.
  - Commit: `chore(windows): package portable x64 release`

- [ ] **E12-T4 Verify WebView2, DPI, and clean-machine behavior**
  - Depends on: E12-T3
  - Deliverables: Windows 10/11 smoke matrix, WebView2 prerequisite behavior, 100/125/150/200 percent DPI, mixed-monitor scaling, light/dark, keyboard focus, large-result memory, and long-running export checks.
  - Acceptance: missing WebView2 produces clear prerequisite guidance; normal supported systems launch the portable executable directly.
  - Commit: `test(windows): verify portable runtime behavior`

- [ ] **E12-T5 Add portable upgrade, signing, and release documentation**
  - Depends on: E12-T4
  - Deliverables: replacement/upgrade instructions, AppData backup and migration behavior, optional Authenticode signing pipeline, checksum verification, known SmartScreen behavior if unsigned, and uninstall-by-folder-deletion guidance.
  - Acceptance: users know which files are portable, which data stays in AppData, and how to upgrade/remove Tarik safely.
  - Commit: `docs(windows): document portable release lifecycle`

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

- [x] **D1:** Linux remains the development platform; Windows 10/11 x64 portable ZIP is the first additional release target. No Windows installer is required.
- [x] **D2:** Pin `duckdb` Rust binding `1.10505.0`. Normal development no longer uses `bundled`; the DuckDB adapter links a pinned prebuilt dynamic library. Binary Arrow transport is resolved as `crates/arrow-page-format` in E5.5-T2 instead of a separate measurement task.
- [ ] **D3:** Choose the Arrow batch transport strategy across Tauri IPC after measuring JSON vs binary transfer overhead.
- [ ] **D4:** Define default result page size, cache size, and worker count from E11-T2 measurements rather than guesses.
- [ ] **D5:** Define whether “empty export” creates no files or one schema-only file; document consistently.
- [x] **D6:** Ownership-safe project removal: Tarik-managed projects may delete their managed directory after explicit confirmation; externally opened DuckDB files can only be forgotten from Tarik metadata and are never deleted, renamed, or moved automatically.

Do not resolve an open decision implicitly inside unrelated code. Record the decision here and in the implementing commit.

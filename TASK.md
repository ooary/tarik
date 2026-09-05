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
E12 Windows portable        -> native package and platform review
E13 Desktop UX/lifecycle    -> packaged visual and process review
E13.5 Interaction/resources -> modal, saved-query, and engine-settings review
E14 Profiles/quality checks -> beginner workflow and correctness review
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
  - Historical ownership: result-page model, engine result writer, bounded page reader. The placeholder `crates/arrow-page-format/` was superseded by E6's concrete DuckDB page writer and desktop bounded reader, then removed with production-unreachability proof in E12-T0.
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

**Status:** `APPROVED` (2026-09-04) - user accepted exact CSV/Parquet exports, validated options, progress/cancellation, safe partial failure and replacement, persistent terminal history, and the editor export workflow. E9.5 is ready.

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
  - Review: `docs/review/E9-EXPORTS.md`
  - Notes:
    - Added an editor-toolbar Export action after Actual Flow. The controlled Radix dialog is a compact single-page workflow: immutable SQL preview and options, then queued/running progress, then success/failure/cancel summary. Closing the dialog does not cancel active work; polling continues with structurally cleared/retried timers.
    - Options cover native folder selection, portable base name, positive rows per part, stop-or-replace collision policy, CSV delimiter/header, and Parquet compression (Snappy/Zstandard/Gzip/uncompressed). Frontend field checks are repeated by desktop and engine validators before work. Potentially mutating, `WITH`, unknown, or multi-statement SQL requires confirmation because export executes it once.
    - Progress uses exact rows/files/bytes and elapsed time without a decorative progress bar. Terminal states distinguish zero rows/no files, full success, failure, and cancellation; partial outcomes disclose that completed files remain valid and the incomplete stage was removed. Completed parts are shown in a bounded scroll area with Reveal output through the official Tauri opener plugin.
    - Responsive rules preserve single-line toolbar labels at narrow windows and collapse the dialog to one column below 680px. Tests cover disabled/preflight, canonical CSV/Parquet command shapes, polling while closed, cancellation, successful parts/reveal, zero rows, mutating SQL confirmation, and partial failure disclosure.

---

## EPIC E9.5 — Beginner SQL intelligence

**Status:** `APPROVED` (2026-09-04) - user accepted aggregate-specific beginner flow semantics, catalog/alias-aware completion, safe identifier quoting, and non-executing pre-run DuckDB diagnostics. E10 is ready but not started.

**Outcome:** Make SQL construction and pre-run correction approachable for beginners while preserving DuckDB's physical truth.

- [x] **E9.5-T1 Design semantic SQL intelligence boundaries**
  - Depends on: E5-T1, E7-T2, E7-T4
  - Owns: `docs/design/E9-5-DESIGN-GRAPH.md`
  - Deliverables:
    - Separate native physical operators from beginner semantic steps without inventing execution order or duplicating actual metrics.
    - Define aggregate expression shapes, conservative SQL-range mapping, completion contexts, diagnostic revisions, debounce/cancellation, and parser/binder trust boundaries.
    - Fix the example-query contract for `SELECT DISTINCT commodity, count(market) FROM "main"."data_2021" GROUP BY commodity`.
  - Acceptance: graph specifies exact success/failure/resource/test paths before implementation.
  - Tests: graph-protocol completeness review and fixture inventory.
  - Commit: `docs(design): define beginner SQL intelligence graph`
  - Notes:
    - Fixed three separate graphs for semantic plan expansion, catalog-aware completion, and no-execution diagnostics. Immutable SQL is now a required plan-normalization input because DuckDB positional details such as `#0`/`#1` cannot safely identify source expressions alone.
    - Semantic concept nodes must have exactly one physical metric owner; Group and Count can be separate teaching steps backed by one native aggregate, while multiple calculations remain one non-sequential `Calculate summaries` node. Ambiguous semantics preserve the generic native operator and raw plan.
    - Completion is catalog-revision scoped and conservative for aliases/unqualified columns. Diagnostics use per-statement `EXPLAIN (FORMAT JSON)` without ANALYZE, carry SQL revisions, accept only reliably mapped ranges, clear on edit, and require mutation-sentinel tests proving no user statement executes.

- [x] **E9.5-T2 Show specific grouping and aggregate calculations**
  - Depends on: E9.5-T1, E7-T2, E7-T3, E7-T4
  - Owns: `src-tauri/src/plan/`, `src/features/query-flow/`
  - Deliverables:
    - Replace generic `Group & summarize` with semantic steps derived only from verified DuckDB details and conservative SQL mapping.
    - Distinguish `Group rows by …` from aggregate calculations. Label one verified calculation specifically: `Count rows`, `Count non-null <column>`, `Count matching rows`, `Count unique <column>`, `Sum <column>`, `Calculate average`, `Find minimum`, or `Find maximum`.
    - When one physical aggregate computes multiple expressions, show one `Calculate summaries` semantic node with an ordered list of calculations; never imply sequential Count → Sum → Average execution.
    - Represent aggregate-without-`GROUP BY` as one whole-input summary, and aggregate-with-`GROUP BY` as one result per group.
    - Recognize aggregate-free grouping used for `DISTINCT` as `Remove duplicate result rows`; disclose redundant `DISTINCT` only when equivalence is mechanically provable.
    - Collapse DuckDB internal compression/decompression projections and retain raw/native details.
    - Keep actual operator time, rows scanned, cardinality, and estimate accuracy attached once to their native operation; conceptual child steps must not duplicate measured cost.
  - Agreed example:
    ```sql
    SELECT DISTINCT commodity, count(market)
    FROM "main"."data_2021"
    GROUP BY commodity;
    ```
  - Beginner flow must look like:
    ```text
    ┌──────────────────────────────────────────────┐
    │ Read data_2021                               │
    │ Use columns: commodity, market               │
    └──────────────────────┬───────────────────────┘
                           ↓
    ┌──────────────────────────────────────────────┐
    │ Group rows by commodity                      │
    │ Make one group for each commodity            │
    └──────────────────────┬───────────────────────┘
                           ↓
    ┌──────────────────────────────────────────────┐
    │ Count non-null market values per group       │
    │ COUNT(market) does not count NULL values     │
    └──────────────────────┬───────────────────────┘
                           ↓
    ┌──────────────────────────────────────────────┐
    │ Remove duplicate result rows                 │
    │ DISTINCT; redundant after GROUP BY here      │
    └──────────────────────┬───────────────────────┘
                           ↓
    ┌──────────────────────────────────────────────┐
    │ Query result                                 │
    │ Return: commodity, count(market)              │
    └──────────────────────────────────────────────┘
    ```
  - Truthfulness note: `Group rows by commodity` and `Count non-null market values per group` are separate beginner concepts backed by the same DuckDB aggregate operator. They must not duplicate native timing, row counts, or imply two physical passes. `Remove duplicate result rows` appears because DuckDB plans `DISTINCT`; the redundant note appears only when equivalence is mechanically proven.
  - Acceptance: the agreed example renders exactly in the order above while the raw plan and native aggregate details remain inspectable.
  - Tests: stable Explain/Profile fixtures for count variants, sum/average/min/max, multiple aggregates, grouped/ungrouped aggregate, DISTINCT, redundant DISTINCT, ambiguous expressions, and metric non-duplication.
  - Commit: `feat(flow): explain aggregate calculations explicitly`
  - Notes:
    - Plan capture now passes the immutable SQL snapshot into a conservative ASCII top-level SELECT semantic parser. Typed `PlanSemantic` metadata carries verified title/summary/input/output, exact UTF-16 SQL range, and concept-only ownership; unsupported nested/unicode/ambiguous shapes preserve the native graph and raw plan without guesses.
    - SQL calculation kinds must match DuckDB aggregate detail kinds and counts exactly. One calculation becomes a specific Count/Sum/Average/Minimum/Maximum node; several calculations stay one `Calculate summaries` node with an ordered list, never a false sequential chain. Ungrouped calculations describe the whole input.
    - Grouping is inserted as a concept-only teaching node before the physical calculation and owns no estimated/actual rows, timing, or rows scanned. Verified aggregate plumbing projections collapse only when connected to an aggregate; other projections remain. Native metrics stay on exactly one physical node.
    - Aggregate-free DISTINCT grouping becomes `Remove duplicate result rows`. Redundancy is stated only when selected non-aggregate keys exactly cover all grouping keys and the projection is a verified simple group/aggregate shape.
    - Added sanitized real DuckDB Explain/Profile fixtures for the accepted query. Both render exactly `Read data_2021 → Group rows by commodity → Count non-null market values per group → Remove duplicate result rows → Query result`; semantic node selection highlights GROUP BY, count(market), or DISTINCT, and the inspector discloses shared physical work.

- [x] **E9.5-T3 Complete catalog-aware SQL autocomplete**
  - Depends on: E9.5-T1, E4-T3, E5-T1
  - Owns: `src/features/editor/sqlCompletion.ts`, `src/features/editor/SqlEditor.tsx`
  - Deliverables:
    - Prioritize project schemas, tables, and views after `FROM`/`JOIN`; filter schema-qualified object suggestions after `schema.`.
    - Resolve common table aliases so `alias.` suggests only that source's columns; provide unqualified column suggestions when unambiguous.
    - Insert safely quoted identifiers for spaces, reserved words, and embedded quotes.
    - Distinguish schema/table/view/column/function/keyword suggestions with concise labels and preserve `Ctrl+Space` manual completion.
    - Refresh completion state after import, link, create, drop, rename, and project switch without rebuilding the editor.
  - Acceptance: typing `FROM data_` suggests `data_2021`; accepting a non-simple identifier inserts valid quoted SQL; `d.` after `FROM "main"."data_2021" d` suggests its columns.
  - Tests: completion-source interaction tests for FROM/JOIN, schema qualification, aliases, ambiguous columns, safe quoting, manual trigger, catalog refresh, and empty catalog.
  - Commit: `feat(editor): add catalog-aware SQL completion`
  - Notes:
    - Replaced schema-only completion with a synchronous project-catalog `CompletionSource` layered alongside CodeMirror's uppercase SQL keyword/function source. FROM/JOIN contexts rank tables/views and schemas with textual type labels; duplicate relation names insert schema-qualified identifiers.
    - `schema.` scopes relations, and recognized unique aliases scope `alias.` columns even when the FROM clause follows the SELECT cursor. Manual completion offers only columns unambiguous across recognized source relations; ambiguous aliases/columns make no catalog claim and preserve general language completion.
    - Completion applies portable safe identifier quoting for spaces, reserved words, and embedded quotes. Catalog changes reconfigure only the completion compartment, preserving editor text, selection, history, and Ctrl+Enter behavior; typing after `.` or pressing Ctrl+Space opens contextual suggestions.
    - Direct CompletionContext tests cover FROM/JOIN, schema qualification, table/view distinction, aliases, ambiguity, unqualified columns, safe quoting, and empty catalogs. Mounted CodeMirror tests prove Ctrl+Space and live catalog refresh; a test-only Range geometry shim supports tooltip layout in jsdom.

- [x] **E9.5-T4 Add non-executing pre-run SQL diagnostics**
  - Depends on: E9.5-T1, E5.5-T2, E5-T1
  - Owns: engine validation protocol, `src-tauri/src/query/`, CodeMirror diagnostics
  - Deliverables:
    - Add a sidecar validation method that parses and binds an immutable SQL snapshot against the active DuckDB catalog without executing user statements.
    - Debounce editor validation after idle, identify every request by SQL revision, cancel or ignore stale work, and keep validation off the query/export execution queues.
    - Show accessible CodeMirror lint markers, gutter indicators, hover text, and a compact current-problem summary for reliable source ranges.
    - Distinguish blocking errors from a small allowlist of high-confidence warnings; never claim that a query is guaranteed to run.
    - Clear stale diagnostics immediately when editing resumes and avoid flashing errors for transient incomplete typing.
    - Use the official CodeMirror lint package and existing semantic error/warning tokens; no custom parser claims beyond tested local structural checks.
  - Acceptance: syntax, missing-table, and missing-column errors appear before Run without changing data; fixing SQL clears them; runtime-only failures remain documented as undetectable pre-run.
  - Tests: no-execution mutation sentinel, syntax/binder locations, no reliable range fallback, stale response race, debounce, project/catalog change, warning allowlist, keyboard/screen-reader semantics, and engine/session reuse.
  - Commit: `feat(editor): show safe pre-run SQL diagnostics`
  - Notes:
    - Added revision-tagged `SqlValidation/SqlDiagnostic` protocol shapes and synchronous sidecar `query.validate`. Each immutable statement is prepared as `EXPLAIN (FORMAT JSON) <statement>` without ANALYZE or query.execute; mutation-sentinel integration proves validation of CREATE/INSERT/UPDATE/DELETE changes neither catalog nor rows.
    - DuckDB Parser/Binder/Catalog errors become structured codes and concise messages. A marker range is emitted only when an observed `LINE n` excerpt and caret map exactly back through the injected Explain prefix and statement source range; end-of-input, unicode, or mismatched excerpts remain message-only with no arbitrary underline.
    - Added one high-confidence warning family after successful bind: top-level UPDATE/DELETE without a top-level WHERE. Strings, comments, nested expressions, and statements with WHERE are excluded. No speculative style warnings were added; redundant DISTINCT remains in the verified flow inspector.
    - QueryWorkspace clears diagnostics immediately on edit, waits 650 ms, validates the immutable SQL/catalog revision, rejects stale or wrong-revision responses, revalidates on catalog change, and treats engine unavailability as non-blocking. Clean state says `No problems detected before execution` and explicitly allows runtime-only failure.
    - Official `@codemirror/lint` supplies accessible gutter markers, hover messages, keyboard navigation, and error/warning wavy underlines for reliable ranges. Message-only diagnostics remain in the compact toolbar summary. Tests cover debounce, stale races, unmount cleanup, catalog revisions, mounted marker clearing, clean/unavailable states, multiline/offset caret mapping, warnings, project command shape, and no execution.

- [x] **E9.5-T5 Review beginner SQL intelligence together**
  - Depends on: E9.5-T2, E9.5-T3, E9.5-T4
  - Owns: `docs/review/E9-5-SQL-INTELLIGENCE.md`
  - Deliverables:
    - Manual checklist covering aggregate semantics, multi-aggregate physical truth, table/alias completion, identifier quoting, stale diagnostics, and no-execution validation.
    - Re-run E7 Estimate/Actual Flow terminology and immutable-snapshot checks because aggregate normalization changes both modes.
  - Acceptance: a beginner can write the example query with completion, correct mistakes before Run, and explain each displayed group/count/distinct step without being taught a false physical sequence.
  - Tests: full Rust/TypeScript/lint/build gates and focused regression matrix.
  - Commit: `docs(review): add E9.5 SQL intelligence checklist`
  - Review: `docs/review/E9-5-SQL-INTELLIGENCE.md`
  - Notes:
    - Combined review covers the accepted five-step grouped COUNT DISTINCT graph, single-owner physical metrics, aggregate variants/multiple summaries, conservative DISTINCT redundancy, raw plan preservation, and Estimate/Actual Flow immutable snapshots.
    - Completion review covers FROM/JOIN/schema tables/views, aliases, unambiguous columns, safe quoting, Ctrl+Space, live catalog/project refresh, and ambiguity fallbacks.
    - Diagnostic review covers clean/checking/problem/unavailable states, syntax/binder/catalog errors, reliable and message-only locations, debounce/stale races, catalog revalidation, keyboard access, the narrow mutation warning, and manual proof that validation never changes data.

---

## EPIC E10 — Diagnostics, recovery, and cleanup

**Status:** `PROVISIONALLY ACCEPTED` (2026-09-04) - user authorized E11 progression and deferred the combined logging, incident, cleanup, and shutdown review. E10 remains reopenable and its manual checklist remains unsigned until final release acceptance.

**Outcome:** Diagnosable failures and bounded on-disk application state.

- [x] **E10-T0 Design diagnostics, cleanup, and shutdown boundaries**
  - Depends on: E9.5 approval
  - Owns: `docs/design/E10-DESIGN-GRAPH.md`
  - Deliverables:
    - Define typed/redacted log events and bounded size-based retention.
    - Define exact cache/export ownership boundaries, crash manifests, and user-export preservation.
    - Define draft-first, cancellation-bounded, idempotent shutdown order and failure escape hatches.
  - Acceptance: graph covers success, failure, resource ownership, boundary parsing, behavior layers, and test substitutions before implementation.
  - Tests: Graph Protocol completeness review and implementation mismatch inventory.
  - Commit: `docs(design): define E10 recovery lifecycle`
  - Notes:
    - Logging is an orthogonal JSONL behavior layer: SQL text, parameters, previews, result rows, and exported payload data are excluded; IDs, durations, stable codes, and aggregate counters are allowed.
    - Cleanup may recurse only below Tarik-owned cache roots. User-selected export directories require exact per-export crash manifests and exact hidden filenames; canonical completed export parts are never deleted.
    - Shutdown policy is explicit: flush latest drafts/preferences, cancel queued/running query and export work, wait up to two seconds for terminal persistence, release ephemeral results, close DuckDB/sidecar, checkpoint SQLite, then flush logs and close.

- [x] **E10-T1 Add structured rolling file logging**
  - Depends on: E0-T4
  - Owns: `src-tauri/src/observability/`
  - Deliverables:
    - Daily or size-based rolling logs with retention.
    - Levels, timestamps, operation IDs, project IDs, and duration.
    - Redact result data; avoid logging full SQL at info level by default.
  - Acceptance: user can reveal logs from settings; rotation bounds disk use.
  - Tests: rotation/retention and redaction tests.
  - Commit: `feat(logging): add rolling diagnostic logs`
  - Notes:
    - Added a closed-schema JSONL logger with 2 MiB active-file rotation and six archives (seven files total). It degrades to stderr on directory/write/rotation failure instead of failing user operations.
    - Events accept only stable target/event names, operation/project/incident IDs, duration, status, error code, and one bounded operational message. SQL-like messages are redacted and there is no payload/result/arbitrary-map channel.
    - Project create/open/close and query/export submission now emit paired operation spans with generated operation IDs and durations; app startup/shutdown are recorded without SQL or result data.
    - Settings shows the exact retention policy and reveals the Tauri-resolved log directory through the existing opener boundary.

- [x] **E10-T2 Add panic/error boundary and support diagnostics**
  - Depends on: E10-T1, E1-T1
  - Owns: Rust panic hook, frontend error boundary, diagnostics UI
  - Deliverables:
    - Friendly crash/error surface with log location and copyable incident ID.
    - No raw panic details presented as the only user message.
  - Acceptance: simulated backend and frontend failures lead to recoverable diagnostics.
  - Tests: error boundary and panic-hook unit tests where practical.
  - Commit: `feat(diagnostics): add app failure recovery surfaces`
  - Notes:
    - Installed a backend panic hook that records a generated incident ID, writes a one-shot local incident marker, emits a support event when the WebView is available, prints a concise stderr fallback, and then chains to Rust's prior panic hook.
    - Frontend render failures report one typed, bounded incident (`frontend.render`) and show a friendly recovery surface with a copyable incident ID, local log disclosure, Reveal logs, and Retry. Raw render details are neither the heading nor required user guidance.
    - The workbench consumes backend panic incidents live and once after restart; the marker is removed after reading so old incidents do not recur forever. Reporting/clipboard/reveal failures keep visible fallback instructions.

- [x] **E10-T3 Clean stale caches and incomplete exports safely**
  - Depends on: E5.5-T2, E6-T3, E9-T3
  - Owns: cache cleanup service
  - Deliverables:
    - Startup cleanup for abandoned result cache entries.
    - Age/size limits and explicit clear-cache action.
    - Never delete user-completed exports.
  - Acceptance: cleanup is constrained to Tarik-owned cache/staging directories.
  - Tests: path safety, stale/fresh distinction, partial export handling.
  - Commit: `chore(storage): clean stale temporary artifacts`
  - Notes:
    - Startup cleanup now applies a 24-hour age limit and 512 MiB oldest-first budget only to direct non-symlink artifacts below Tarik's resolved `<cache>/results` root; errors are bounded warnings and outside sentinels are preserved.
    - Explicit Settings cleanup first asks the live sidecar to release every published result, clears the desktop decoded-page LRU, then removes owned result artifacts. It never accepts a frontend path and clearly states completed exports are preserved.
    - Every export registers an atomic cache-owned recovery manifest before sidecar submission. Sidecar hidden stage/backup names include the export UUID and exact part number, allowing next-start reconciliation to remove only exact incomplete stages or restore an exact backup when its canonical part is absent.
    - Cleanup never recursively deletes a user-selected output directory and never deletes canonical `<base>-part-NNNNN.<ext>` files. Malformed/unsafe manifests are removed without touching their claimed output paths.

- [x] **E10-T4 Add graceful application shutdown coordinator**
  - Depends on: E5.5-T3, E5-T3, E6-T2, E9-T3
  - Owns: app shutdown composition
  - Deliverables:
    - Flush current drafts.
    - Cancel/finish active jobs according to explicit policy.
    - Stop engine processes, then close files, cursors, SQLite, and logger in order.
  - Acceptance: forced test shutdown leaves databases reopenable and no owned temp file locked.
  - Tests: shutdown during edit, query, and export.
  - Commit: `feat(app): coordinate graceful shutdown`
  - Notes:
    - One idempotent backend coordinator owns all shutdown resources at setup. Close is intercepted only after the mounted workbench registers; the frontend flushes the latest immutable tab snapshot and preferences before invoking terminal shutdown.
    - Draft flush failure keeps the window open and presents Retry save or explicit Quit without latest changes; Tarik never claims an unsaved draft persisted.
    - Terminal shutdown cancels queued/running query and export work, waits at most two seconds for terminal history, releases all sidecar results, closes the DuckDB session and sidecar, truncates/checkpoints SQLite WAL, records a bounded shutdown event, flushes logs, then destroys the window.
    - If the frontend never registered, native close is not intercepted and the Destroyed fallback still stops the engine and flushes logs.

- [x] **E10-T5 Review diagnostics and recovery together**
  - Depends on: E10-T1, E10-T2, E10-T3, E10-T4
  - Owns: `docs/review/E10-DIAGNOSTICS-RECOVERY.md`
  - Deliverables:
    - Combined manual checklist for logging, incidents, cleanup, recovery, and shutdown.
    - Re-run E9.5 diagnostics and E9 export regression boundaries after storage and shutdown changes.
  - Acceptance: a user can find logs, recover from crashes without data loss, clear temporary state safely, and reopen cleanly after forced shutdown.
  - Tests: full Rust/TypeScript/lint/build gates and focused regression matrix.
  - Commit: `docs(review): add diagnostics recovery checklist`
  - Review: `docs/review/E10-DIAGNOSTICS-RECOVERY.md`
  - Notes:
    - Combined review covers the closed log schema and 2 MiB/seven-file retention, friendly frontend/backend incident surfaces with one-shot restart recovery, 24-hour/512 MiB owned-root startup cleanup, path-free explicit cache clearing, exact abandoned-export manifests, draft-first close interception, two-second bounded job cancellation, result release, DuckDB/SQLite close order, and the destroyed fallback.
    - Regression checks re-verify E9.5 diagnostics, E9 export safety, saved-query reopen semantics, and retention isolation.

---

## EPIC E11 — Quality, performance, packaging, and release

**Status:** `REVIEW / RELEASE BLOCKED` - E11-T0 through E11-T5 implemented and automated evidence passes. Awaiting E11 package/documentation review plus deferred E6, E7, and E10 manual gates before final publication.

**Outcome:** Repeatable, measured MVP release with documented limits.

- [x] **E11-T0 Design the release evidence pipeline**
  - Depends on: E10 implementation and provisional progression
  - Owns: `docs/design/E11-DESIGN-GRAPH.md`
  - Deliverables:
    - Define real-sidecar golden workflow and restart evidence boundaries.
    - Define fixed-scenario RSS/disk measurements and version-controlled budget verdicts.
    - Define invoke/filesystem audit, Linux packaging/checksum/smoke, compatibility docs, and deferred-gate release verdict.
  - Acceptance: graph covers success/failure/resource/test paths and inventories current release gaps before implementation.
  - Tests: Graph Protocol completeness review and implementation mismatch inventory.
  - Commit: `docs(design): define E11 release evidence`
  - Notes:
    - Golden tests use production managers/coordinators, real SQLite migrations, the real DuckDB sidecar, deterministic fixtures, bounded polling, and a structurally removed isolated root; browser pixel behavior remains in component tests.
    - Memory budgets measure the sidecar separately because DuckDB/Arrow live there, require machine/dataset metadata in every report, and may change bounded defaults only with recorded evidence.
    - Security review removes generic invoke mutators that bypass ownership, uses non-null CSP and action-specific plugin permissions, and proves native-dialog/owned-root boundaries with adversarial paths and identifiers.
    - E11 ships Linux artifacts only; Windows remains E12. The ship verdict remains REVIEW while deferred E6/E7/E10 manual gates are unsigned.

- [x] **E11-T1 Add end-to-end golden workflows**
  - Depends on: E4, E5, E6, E7, E8, E9 core tasks
  - Owns: E2E tests and fixtures
  - Deliverables:
    - Create project → import/link → join query → inspect flow → save → export → restart → history/session restore.
    - Missing linked file and cancelled query/export paths.
  - Acceptance: workflows pass from a clean application data directory.
  - Tests: automated E2E suite on supported CI environment.
  - Commit: `test(e2e): cover core tarik workflows`
  - Notes:
    - Added one isolated real-composition golden test using production ProjectManager, QueryCoordinator, ResultStore, Plan capture, ExportCoordinator, metadata repositories, the actual DuckDB sidecar, and deterministic CSV/Parquet fixtures.
    - The workflow creates a managed project, imports orders, links markets, executes/paginates a grouped join, captures Explain and Actual Flow, saves SQL, exports exact one-row parts, cancels long query/export work, proves session reuse, persists a draft, closes every service, creates fresh service objects, reopens the project, detects and repairs a missing link, and verifies catalog/session/saved/history/export continuity.
    - CI now formats, Clippy-checks, and tests the whole Rust workspace, explicitly builds/checks the real sidecar before tests, and caches the workspace root rather than only `src-tauri`.

- [x] **E11-T2 Establish performance and memory budgets**
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
  - Notes:
    - Added a release-sidecar Linux `/proc` harness with fixed idle/import/100K-row result/12-cycle query-release/large-export-cancel scenarios, 20 ms RSS/HWM and owned-disk sampling, machine/tool/DuckDB/dataset metadata, JSON evidence, and check/record modes.
    - Version-controlled ceilings are 512 MiB fixed-workload peak sidecar RSS, 64 MiB retained growth after twelve run/page/release cycles, zero result cache bytes after release, and zero hidden export stages after cancellation. These are regression limits, not arbitrary-SQL memory promises.
    - On the i5-1235U/15.3 GiB release run, the checked baseline measured 40.5 MiB idle, 106.7 MiB peak, 3.5 MiB post-cycle growth, zero residual result bytes, and zero hidden stages. A repeat measured 117.4 MiB peak and 7.5 MiB growth; both passed.
    - The fixed evidence supports retaining 500-row/~4 MiB engine pages, a 12-page desktop decoded LRU, one query/export FIFO worker per session, and 4 MiB Parquet row groups. `scripts/build-engine.sh` now correctly maps `CARGO_BUILD_PROFILE` to the Cargo profile.

- [x] **E11-T3 Security and filesystem boundary review**
  - Depends on: E4, E9, E10 core tasks
  - Owns: Tauri capabilities, boundary tests, review notes
  - Deliverables:
    - Least-privilege Tauri commands/capabilities.
    - Identifier/path escaping review.
    - Confirm cleanup cannot traverse outside owned directories.
  - Acceptance: UI cannot invoke arbitrary filesystem or SQL-adjacent internal operations outside declared commands.
  - Tests: malicious path/identifier and command-input tests.
  - Commit: `fix(security): harden local trust boundaries`
  - Review: `docs/security/E11-BOUNDARY-REVIEW.md`
  - Notes:
    - Invoke audit now has exact parity at that review point: 54 registered handlers and 54 typed frontend invoke names; E11.5 adds the reviewed `create_table` pair for 55/55. Ten unused generic metadata mutators that bypassed project/query/export ownership were removed from the public handler and wrapper modules.
    - Removed broad `opener:default` WebView permission. Logs are revealed by a no-path backend command; export reveal sends only export ID/part number and Rust derives an existing canonical file from immutable validated options plus the coordinator's completed-part record.
    - Added a non-null local CSP allowing only self scripts/default content, Tauri IPC, local asset/data images, and inline styles required by CodeMirror/XYFlow geometry; remote scripts/frames/network origins are excluded.
    - Boundary review records active-project/kind checks, identifier/path escaping, external/managed ownership, export publication/reveal, bounded result paging, cleanup/manifest/symlink behavior, incident redaction, shutdown ownership, and intentional explicit-SQL capabilities with adversarial test evidence.

- [x] **E11-T4 Add packaging, versioning, and release artifacts**
  - Depends on: E11-T1, E11-T2, E11-T3
  - Owns: packaging/release config and docs
  - Deliverables:
    - Application name/icon/version, platform bundles, licenses/notices.
    - Upgrade compatibility note for SQLite migrations and DuckDB projects.
    - Checksums for distributed artifacts where supported.
  - Acceptance: clean-machine install/start/uninstall smoke test on each declared platform.
  - Tests: release build and packaged smoke test.
  - Commit: `chore(release): package tarik desktop mvp`
  - Notes:
    - Added reproducible `npm run release:linux` packaging for Linux x86_64 only. It verifies npm/Tauri/Cargo version parity, builds the pinned release sidecar, checks `$ORIGIN`/`libduckdb.so`, stages target-triple Tauri external binaries, builds DEB/AppImage, and assembles a portable tarball.
    - The script generates exact Cargo/npm dependency inventories, includes Tarik MIT license, DuckDB/third-party notices and compatibility/backup guidance, verifies sidecar handshake, checks DEB contents, launches the portable desktop and AppImage under clean XDG roots, generates SHA-256 checksums, verifies them, and writes a machine-readable release manifest.
    - Built artifacts: 139 MiB AppImage, 30 MiB DEB, and 29 MiB portable tarball in `target/release-artifacts`; all checksums pass. Artifacts are explicitly unsigned and the manifest retains deferred E6/E7/E10 release gates.
    - AppImage uses `NO_STRIP=true` because rolling-distribution symbols can break linuxdeploy's strip tool; Cargo's release profile already strips Rust binaries. Windows packaging remains E12 and is not implied.

- [x] **E11-T5 Write user documentation and ship checklist**
  - Depends on: E11-T4
  - Owns: README/user docs/release checklist
  - Deliverables:
    - Explain CSV vs Parquet, link vs import, tables/views, joins, Explain vs Profile, chunk export, history, logs, and data locations.
    - Document known limitations and backup behavior.
  - Acceptance: a new user can complete the golden workflow without developer help.
  - Tests: manual documentation walkthrough.
  - Commit: `docs(app): add mvp user guide`
  - Guide: `docs/user/USER-GUIDE.md`
  - Ship gate: `docs/release/SHIP-CHECKLIST.md`
  - Notes:
    - Replaced the stale foundation README with exact setup, verification, memory, Linux packaging, data-location, license, and pre-release/deferred-gate guidance.
    - Added a beginner walkthrough for project ownership, CSV/Parquet import/link tradeoffs, table/view semantics, completion/diagnostics, bounded result browsing, Estimate versus Actual Flow, saved queries/history, exact export/cancel/recovery behavior, logs/cache, backup/upgrades, removal, and known limitations using current visible labels.
    - Added an evidence-based ship checklist covering versions, direct full gates, memory budgets, security boundaries, artifact content/checksums/smokes, documentation walkthrough, clean-machine package tests, and final publication. E6/E7/E10 remain explicit unchecked release blockers.
    - Added `npm run docs:check` and CI enforcement for local Markdown links, documented npm commands, version parity, required beginner topics, and retained deferred gates.

- [x] **E11-T6 Review release evidence together**
  - Depends on: E11-T1, E11-T2, E11-T3, E11-T4, E11-T5
  - Owns: `docs/review/E11-RELEASE.md`
  - Deliverables:
    - Combined manual checklist for the golden workflow, memory evidence, security boundaries, Linux packages, upgrade/backup/notices, and beginner documentation.
    - Preserve E6/E7/E10 and cross-distro package smoke as explicit unchecked blockers; do not claim final release from automated evidence.
  - Acceptance: E11-specific evidence is reviewable and final publication remains mechanically blocked until every deferred/manual item is signed.
  - Tests: full workspace/frontend/docs/build/memory/package checks against a precise source revision.
  - Commit: `docs(review): add E11 release checklist`
  - Review: `docs/review/E11-RELEASE.md`
  - Notes:
    - Clean Linux artifacts were generated from commit `fb0c71d`: AppImage 138.7 MiB, DEB 30.1 MiB, portable tarball 29.4 MiB; sidecar/RPATH/content/clean-XDG smoke and all SHA-256 checks pass.
    - Final release-candidate memory run measured 105.0 MiB sidecar peak and 1.4 MiB post-cycle growth with zero residual result bytes or hidden export stages.
    - Release remains blocked by E11 package/doc manual checks and deferred E6 result, E7 flow, and E10 recovery reviews.

---

## EPIC E11.5 — Desktop theme, table creation, and context interactions

**Status:** `APPROVED` (2026-09-04) - user accepted the complete desktop correction pass after Linux review; E12 is unblocked.

**Design read:** Focused correction pass for a calm, dense SQL workbench. Preserve the existing IDE structure and restrained green accent while making effective-theme behavior, command affordances, context menus, selection, and connection state explicit rather than browser-like.

**Outcome:** Dark mode is legible and consistently Dracula-themed in SQL editors, New table is a real safe workflow, native browser context menus never leak through, and each desktop surface exposes only the context actions appropriate to it.

**Interaction policy:**

| Surface                                                        | Right-click behavior                                                                                                           |
| -------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| App header, panel headings, result column headers, empty space | Suppress the browser/WebView menu; show no custom menu                                                                         |
| SQL editor                                                     | Tarik menu with **Run query** plus applicable editing actions; use the same run/mutation path as the toolbar                   |
| Result body cell                                               | Tarik menu for bounded cell/selection copying and **Run query again**                                                          |
| Left explorer project row                                      | Tarik project menu, after selecting that project/workspace row                                                                 |
| Other left explorer content                                    | Suppress right-click; table/source actions must use an explicit accessible row action rather than browser or right-click menus |

- [x] **E11.5-T0 Design the effective-theme and context-command graph**
  - Depends on: E11 implementation
  - Owns: `docs/design/E11-5-DESIGN-GRAPH.md`
  - Deliverables:
    - Define one effective theme shape (`light | dark`) derived from manual Light/Dark or live system preference and consumed by CSS and CodeMirror.
    - Define scoped context targets, commands, selection coordinates, immutable rerun snapshots, mutation confirmation, and browser-menu suppression boundaries.
    - Define New table DDL generation/identifier validation, catalog refresh, connection-state truth, keyboard equivalents, and teardown of media/context listeners.
  - Acceptance: graph uses Graph Protocol sections and resolves click/right-click/Shift/Ctrl-or-Command semantics, no-project/error states, and virtualized-page boundaries before implementation.
  - Tests: design completeness and current-code mismatch inventory.
  - Commit: `docs(design): define E11.5 desktop interaction fixes`
  - Notes:
    - Added a Graph Protocol contract for effective theme and live CodeMirror reconfiguration, safe allow-listed table DDL, global native-menu suppression with target classification, current-page coordinate selection/TSV copying, immutable result SQL reruns, and handshake/session-owned connection state.
    - Current-code verdict explicitly records the six gaps this epic must close; listener, editor, selection, menu, clipboard, and DuckDB session resources all have structural scope.

- [x] **E11.5-T1 Make dark mode legible and theme CodeMirror with Dracula**
  - Depends on: E11.5-T0
  - Owns: effective theme resolver, `src/styles/tokens.css`, result styles, `src/features/editor/SqlEditor.tsx`, theme tests
  - Deliverables:
    - Use readable near-white primary text for all result values in effective dark mode, including nulls, active/selected rows, loading, error, and virtualized cells; retain secondary hierarchy and WCAG AA contrast.
    - Apply the Dracula editor palette whenever the **effective** theme is dark: manual Dark and System while `prefers-color-scheme: dark`. Keep the existing light editor theme when effective light.
    - Reconfigure existing CodeMirror views without destroying editor history, selection, diagnostics, completion state, or focus; react live to system-theme changes and remove listeners on teardown.
    - Use Dracula's canonical background/foreground/selection and syntax colors consistently rather than approximating dark mode with only a background swap.
  - Acceptance: manual Light, Dark, System-light, System-dark, and live OS theme changes keep Results and every SQL snapshot/editor readable; system dark produces the Dracula editor theme.
  - Tests: token contrast checks, effective-theme resolver tests, CodeMirror reconfiguration tests, result loading/error/selected states, both manual and mocked system media changes.
  - Commit: `fix(theme): align dark results and editor colors`
  - Notes:
    - Added one effective-theme resolver and scoped media-query listener; manual Light/Dark override the OS while System follows live `prefers-color-scheme` changes.
    - CodeMirror reconfigures through a theme compartment using canonical Dracula background, foreground, selection, gutter, cursor, and syntax colors without remounting or losing focus/text. Result body values use dedicated near-white/null tokens in effective dark mode.

- [x] **E11.5-T2 Implement the New table workflow**
  - Depends on: E11.5-T0
  - Owns: explorer New table action, accessible dialog/form, typed Tauri command/service, DuckDB DDL boundary, catalog refresh
  - Deliverables:
    - Replace the inert **New table** control with a project-scoped dialog for a table name and one or more columns containing name, supported DuckDB type, and nullability.
    - Require an open project and at least one valid uniquely named column. Use a reviewed type allow-list and the central DuckDB identifier-quoting path; never concatenate raw identifiers or arbitrary type text into DDL.
    - Show submitting, inline validation, duplicate-table, engine, and success states. On success, close the dialog, refresh catalog/completion, and make the table immediately available without an implicit query run.
    - Preserve the dialog on recoverable failure and create no phantom SQLite source record or partially represented catalog entry.
  - Acceptance: a beginner can create, discover, insert into, query, and later delete an empty table; quoted/reserved-word names are either safely supported or rejected with exact guidance.
  - Tests: no-project disabled state, keyboard/focus cycle, empty/duplicate/quoted/malicious names, duplicate columns, supported types/nullability, engine failure, exactly one CREATE execution, catalog/completion refresh.
  - Commit: `feat(sources): add safe new table workflow`
  - Notes:
    - Added a project-scoped Radix dialog for table name and dynamic name/type/nullability columns with inline validation, retry-preserving engine errors, and catalog/completion refresh only after success.
    - The typed desktop/sidecar protocol accepts eight exact DuckDB types, validates active-project ownership, quotes every identifier centrally, rejects case-insensitive duplicate columns, and creates no SQLite source metadata for an empty user table.

- [x] **E11.5-T3 Replace browser context menus with scoped Tarik menus**
  - Depends on: E11.5-T0
  - Owns: app-shell context policy, editor menu, explorer project/row actions, `ContextMenu` accessibility and tests
  - Deliverables:
    - Prevent WebView/browser entries such as Reload and Inspect element across application chrome, headings, empty areas, and unsupported explorer/result targets in both development and packaged builds.
    - Add an editor context menu whose **Run query** command delegates to the same immutable submission and mutation-confirmation path as toolbar Run/`Ctrl-or-Command+Enter`; retain applicable Cut, Copy, Paste, and Select all behavior.
    - Reserve explorer right-click for a selected active/recent project row. Empty explorer space, headings, table rows, and linked-source rows must not open browser or context menus.
    - Keep existing catalog/source operations reachable through explicit visible, keyboard-accessible row action controls when their old right-click menu is removed.
    - Support keyboard invocation (`Shift+F10`/Menu key), focus restoration, Escape dismissal, disabled states, and screen-reader labels; never rely on pointer-only commands.
  - Acceptance: exhaustive surface testing produces either the declared Tarik menu or no menu, never browser Reload/Inspect; toolbar/keyboard alternatives remain available.
  - Tests: contextmenu event matrix for header/editor/result header/result body/explorer/project rows/empty space, editor command delegation, clipboard permissions/failure, keyboard menu navigation, production WebView smoke.
  - Commit: `fix(app): scope desktop context menus`
  - Notes:
    - A document-scoped lifecycle listener now suppresses the native WebView browser menu everywhere; Radix context triggers still receive and render only Tarik commands.
    - SQL editor right-click offers Run query plus editing actions and delegates Run to the same workspace callback. Explorer context menus remain only on recent project rows; table and linked-source operations moved to visible Phosphor overflow controls using the keyboard-accessible dropdown primitive.

- [x] **E11.5-T4 Add bounded spreadsheet-style result selection and context actions**
  - Depends on: E11.5-T3, E6 result paging
  - Owns: `src/features/results/ResultGrid.tsx`, result selection model/menu, query rerun integration, tests
  - Deliverables:
    - Make a body cell the selection anchor; Shift-click/right-click extends one rectangular range, while Ctrl-click on Windows/Linux or Command-click on macOS toggles disjoint cells. Right-click inside the current selection preserves it.
    - Render selected cells with dark/light accessible styling independent of the active-row keyboard cue. Store selection as row/column coordinates, not DOM nodes, so virtualization remains bounded and correct.
    - Provide context actions **Copy cell**, **Copy selected cells**, **Copy row**, **Copy page with headers**, and **Run query again** when applicable. Serialize rectangular selections as TSV with stable row/column order and explicit `NULL`; clear selection on result/page replacement.
    - Limit multi-cell selection to the currently loaded bounded page. Do not fetch hidden pages, collect the full result, or increase the 500-row/12-page memory limits for copying.
    - **Run query again** must explicitly rerun the immutable SQL snapshot that produced the displayed result, not silently use edited SQL. It must pass through normal mutation warnings, cancellation/history, terminal-state, and previous-result release behavior.
    - Result column headers and their resize handles never open a context menu; resize and keyboard-resize behavior remain intact.
  - Acceptance: Shift and Ctrl/Command selections copy exactly the visibly selected bounded data, work with row/column virtualization, and rerun cannot execute a different editor revision accidentally.
  - Tests: single/range/disjoint selection, right-click preservation, NULL/tab/newline clipboard escaping policy, page/result reset, offscreen virtualized cells, header suppression, resize regression, immutable SELECT/mutation rerun, clipboard failure feedback.
  - Commit: `feat(results): add scoped selection context actions`
  - Notes:
    - Result body cells now own coordinate-based single, Shift-rectangle, and Ctrl/Command-disjoint selection independent of virtual DOM nodes; page/result replacement clears selection and never fetches beyond the loaded page.
    - Cell context actions copy cell/selection/row/page as stable TSV with explicit NULL and quoted tabs/newlines, report clipboard failures, and rerun the immutable SQL retained on the terminal execution through the shared mutation-confirmed query lifecycle. Column headers remain menu-free and resize behavior is unchanged.

- [x] **E11.5-T5 Correct explorer/header actions and DuckDB connection status**
  - Depends on: E11.5-T0
  - Owns: header/explorer controls, semantic action tokens, engine status indicator, accessibility and visual tests
  - Deliverables:
    - Remove the `+` button beside **Workspace / Explorer** entirely, including its misleading Refresh catalog label and unused icon import. Existing automatic refresh after project/source/query/table changes remains the catalog truth.
    - Style **New project** as the green primary action and **Close project** as a red action in light and dark themes, with WCAG AA text/icon contrast and non-color labels. Do not make unrelated neutral actions green/red.
    - Increase the DuckDB connection orb to a clearly visible 9–10 px indicator. Connected uses a restrained green inner highlight/halo; disconnected/no-project uses a neutral reflective ring with no false green glow; connecting and failed states remain textually distinguishable.
    - Drive “connected” only after the sidecar handshake, protocol check, and project session open succeed—not merely from the presence of project metadata. Preserve adjacent text such as **Connected to …** / **No DuckDB project open** so color is never the only signal.
    - Any status transition motion must communicate connection change, honor `prefers-reduced-motion`, and stop after the transition rather than pulse forever.
  - Acceptance: no plus control appears in the explorer; action hierarchy and connection truth are obvious at minimum viewport in both themes and with reduced motion/high contrast.
  - Tests: button semantic states/contrast, no-project/connecting/connected/failed/closed engine states, handshake/session failure cannot show connected, reduced-motion styles, minimum viewport and keyboard focus.
  - Commit: `fix(shell): clarify project and engine states`
  - Notes:
    - Removed the Explorer `+`/mislabelled refresh control. New project is the restrained green primary action and Close project is a labelled red destructive action in both themes.
    - The 10 px reflective orb renders idle/connecting/connected/failed states with adjacent text; only successful engine-backed create/open/reopen or restored active session sets connected, failures set failed, and close returns idle. Its finite color transition is disabled under reduced motion.

- [x] **E11.5-T6 Review the desktop correction pass**
  - Depends on: E11.5-T1, E11.5-T2, E11.5-T3, E11.5-T4, E11.5-T5
  - Owns: `docs/review/E11-5-DESKTOP-UX.md`
  - Deliverables:
    - Combined checklist mapping every user report to implementation and automated/manual evidence.
    - Light/Dark/System screenshots at normal and minimum viewport, context-menu surface matrix, New table walkthrough, selection clipboard samples, immutable rerun proof, and engine failure/recovery states.
    - Re-run E6 bounded-results, E9.5 diagnostics, E10 shutdown/draft, and E11 security/CSP/invoke parity regression gates.
  - Acceptance: user manually approves all nine reported corrections before E12 becomes READY; deferred E6/E7/E10 and final-release blockers remain separately reopenable.
  - Tests: full direct Rust/workspace/frontend/docs/lint/typecheck/build gates plus packaged Linux smoke; no critical command piped through `tail` without preserving its exit status.
  - Commit: `docs(review): add E11.5 desktop correction checklist`
  - Notes:
    - Full direct gates against the implementation commits: Rust workspace formats, Clippy-clean, and green including the real sidecar; 151 UI tests across 25 files, 12 typed command tests, docs checker, TypeScript, and production build pass. Lint has zero errors and six pre-existing warnings.
    - The invoke surface grew intentionally to 55/55 handler/frontend parity with the reviewed `create_table` pair; E11 release review is updated accordingly.
    - Manual sign-off items and remaining regression gates are recorded in `docs/review/E11-5-DESKTOP-UX.md` and stay unchecked pending the user's desktop review.

---

## EPIC E12 — Windows portable application support

**Status:** `IN PROGRESS` - E11.5 was manually approved on 2026-09-04 and the production-safe E12-T0 cleanup is complete. E12-T1 Windows CI is next.

**Outcome:** Signed or checksummed Windows x64 portable ZIP that runs without an installer and keeps user data in normal Windows application-data directories. Before the first Windows compilation/package attempt, remove accumulated codebase crust only where production unreachability is proven.

**Portable definition:** The user downloads a ZIP, extracts it, and launches `Tarik.exe` without administrator rights or an installer. Tarik operational metadata remains in Windows AppData by default. User DuckDB, CSV, Parquet, and export files remain where the user chooses. A fully self-contained mode that writes metadata beside the executable is explicitly out of scope unless requested later.

- [x] **E12-T0 Audit and remove production-safe codebase crust before Windows compilation**
  - Depends on: E11.5 approval
  - Blocks: E12-T1 and every Windows compile/package task
  - Owns: production reachability inventory, frontend/Rust/sidecar dependency graph, dead-code removals, stale API cleanup, `docs/design/E12-CODEBASE-CLEANUP.md`
  - Deliverables:
    - Capture a green pre-cleanup baseline and map every production root: frontend entry and reachable workspaces/dialogs, Tauri `generate_handler!` commands, engine protocol methods, DuckDB sidecar entry, build scripts, capabilities, migrations, persisted compatibility fields, release overlays/assets, and packaging scripts.
    - Inventory dead code, unlinked or unreachable pages/components, unused files/exports/types/hooks/styles/assets, stale typed commands/protocol methods, redundant dependencies/features, obsolete scripts/configuration, and tests or docs that describe removed behavior. Classify each candidate as **remove**, **retain with runtime reason**, or **defer because reachability is uncertain**.
    - Use compiler/linter/reference evidence plus production entry-point tracing; static "unused" reports are leads, not deletion proof. Check dynamic imports, macro registration, serde/migration compatibility, CSS selectors, Tauri command strings, `build.rs`, `cfg`/feature/platform branches, sidecar/release manifests, and test-only consumers before deleting anything.
    - Remove only artifacts proven unreachable from production and unnecessary for persisted-data or upgrade compatibility. Do not delete a public command, protocol variant, migration, persisted field, release asset, platform branch, or recovery path merely because the current UI has no direct textual import.
    - Group removals into small atomic commits by boundary (frontend, desktop/commands, sidecar/protocol, dependencies/assets/tooling), with a direct relevant test after each group so a regression can be bisected or reverted without restoring unrelated crust.
    - Re-run the complete Linux production behavior contract after cleanup: clean metadata migration/reopen, golden project/import/query/result/plan/export workflow, E11.5 interactions, shutdown/restart recovery, command parity, real sidecar handshake, memory/cache bounds, CSP/capabilities, and packaged Linux smoke.
    - Record before/after source, dependency, binary, and package-size measurements as observations only; safety and behavior preservation take precedence over reducing line count or artifact size.
  - Acceptance: every deletion has reviewable reachability evidence; no orphan production page/file/API remains among audited candidates; all retained exceptions state why runtime or compatibility needs them; the full direct gate and production smoke baseline remains green with no user-visible workflow removed or changed.
  - Tests: `cargo fmt --all --check`; Clippy with `-D warnings`; all Rust workspace targets; frontend format/lint/typecheck/unit/component/build; docs; exact invoke parity (54/54 after removing the unused `get_app_directories` pair); real sidecar; fresh and upgraded metadata; golden restart; release memory/cache checks; Linux DEB/AppImage/portable content and launch smoke.
  - Commits: `docs(design): inventory pre-windows codebase reachability`, then atomic `refactor(cleanup): ...` commits per proven removal boundary, then `test(cleanup): verify production behavior after crust removal`
  - Notes:
    - Reachability inventory `d72a43d`; removals `3b0611d`, `f170ea2`, `1952a69`, `3d0c7cc`, `255c0b6`, `a93345b`, `7377da9`, `bb3cb3f`, `a2607fb`, `7fca04f`. Removed three runtime npm dependencies, one placeholder Cargo package, uncalled protocol/sidecar/Tauri APIs, a stale member lock, three unlinked UI files, orphan styles/exports, test-only wrappers, and emitted starter assets.
    - Production graph after cleanup: no unlinked runtime frontend page/module; exact 54/54 Tauri handler/frontend parity; 22 sidecar methods all caller-rooted or lifecycle-owned. Migrations, persisted compatibility, platform branches/icons/build scripts, release/recovery paths, generated declarations, and active dynamic CSS were explicitly retained.
    - Before/after: tracked files 236 → 229; tracked bytes −133,506; frontend −208 lines; Rust −219 lines; desktop binary −5,760 B; sidecar −576 B; portable −7,801 B; DEB −7,156 B; AppImage unchanged because its runtime/container dominates.
    - Full direct gates, 150 UI tests, real sidecar/golden restart, release memory (108.5 MiB peak, 3.5 MiB growth, zero residual cache/stages), and Linux portable/DEB/AppImage content/launch/checksums pass. Evidence: `docs/design/E12-CODEBASE-CLEANUP.md`.

- [x] **E12-T1 Add Windows x64 compile and test CI**
  - Depends on: E12-T0, E3
  - Deliverables: `windows-latest` checks for frontend, Rust, bundled DuckDB, and Tauri build using `x86_64-pc-windows-msvc`.
  - Acceptance: every pull request proves the cleaned current source compiles and tests on Windows.
  - Commit: `ci(windows): add native windows build checks`
  - Notes:
    - Added a native `windows-latest` job pinned to Node 24 and Rust 1.91/MSVC. It runs every frontend gate, target-specific Clippy, downloads/builds against pinned DuckDB 1.5.5, stages `duckdb.dll`, performs a real sidecar handshake, runs the complete workspace with the real `.exe`, and compiles the Tauri release executable without bundling.
    - Windows engine discovery now uses `tarik-engine-duckdb.exe`; the sidecar emits `$ORIGIN` only for Linux, and integration tests set `LD_LIBRARY_PATH` only on Unix. Full Linux regression remains green.
    - This local checkout has no Git remote, so a hosted runner could not be triggered here. The workflow is fail-closed and becomes authoritative on its first push/PR; local MSVC-target checking reached native SQLite/zstd build scripts before stopping because Linux has no MSVC C linker.

- [x] **E12-T2 Make development and filesystem boundaries cross-platform**
  - Depends on: E12-T1
  - Deliverables: replace Bash-only reset workflow with cross-platform tooling; test drive-letter, spaces, Unicode, long paths, UNC paths, read-only files, and file-lock errors.
  - Acceptance: Windows development and all local file workflows require no Unix compatibility layer.
  - Commits: `00444e4 fix(platform): add native cross-platform dev tooling`; `3e99dc3 test(engine): isolate sidecar result caches`; `19ba067 test(platform): cover native windows filesystem boundaries`
  - Notes:
    - `npm run engine:build`, `engine:check`, `dev:reset`, `tauri:dev:clean`, and `dev:webview-reset` now use Node 24/native OS APIs. Windows process discovery uses CIM and termination uses `taskkill`; Linux wrappers delegate to the same implementation. No Git Bash/WSL is required.
    - Engine staging is constrained to the active Rust host triple and pinned DuckDB 1.5.5. Sidecar protocol tests own isolated native result roots rather than racing over a shared `/tmp` path.
    - Real sidecar coverage now opens a >260-character Unicode/space DuckDB path, imports a read-only Unicode CSV, pages a result, exports to a Unicode directory, and runs the same flow through a localhost UNC share on Windows CI. On-disk metadata reopens from a Unicode/space path.
    - Native Windows exclusive-handle tests require managed rename and export replacement to fail without changing project metadata or the original file. Unix read-only output coverage requires no final/hidden export artifact. Full Linux Rust/frontend/sidecar gates remain green.

- [x] **E12-T3 Configure portable Windows release artifact**
  - Depends on: E11-T3, E12-T2
  - Deliverables: release-mode `Tarik.exe`, required runtime files, licenses, README, and checksums packaged as `Tarik-<version>-windows-x64-portable.zip`.
  - Acceptance: archive runs after extraction on a clean supported Windows machine without installation or administrator access.
  - Commit: `chore(windows): package portable x64 release`
  - Notes:
    - `npm run release:windows` is native-Windows-only and fails closed elsewhere. It verifies x64 MSVC host and npm/Tauri/Cargo version parity, builds pinned DuckDB 1.5.5, compiles Tauri without an installer, and assembles exactly `Tarik.exe`, `tarik-engine-duckdb.exe`, `duckdb.dll`, Windows README/compatibility/license/notices, dependency inventories, and internal checksums.
    - The packager validates Windows PE headers, exact allow-listed contents, every internal checksum before and after ZIP extraction, and real sidecar handshakes before and after extraction. It emits an outer SHA-256 file and unsigned release manifest; CI upload remains optional supplemental evidence.
    - `npm run verify:windows-manual` verifies the extracted package, desktop launch/restart, WebView2, bounded memory/workloads, export cancellation, and graceful shutdown without deleting existing Tarik AppData. Native Windows 10 x64 packaging, extraction, checksums, direct launch, and graceful close were accepted on September 5, 2026.

- [ ] **E12-T4 Verify WebView2, DPI, and clean-machine behavior** _(manual QA in progress)_
  - Depends on: E12-T3
  - Deliverables: Windows 10/11 smoke matrix, WebView2 prerequisite behavior, 100/125/150/200 percent DPI, mixed-monitor scaling, light/dark, keyboard focus, large-result memory, and long-running export checks.
  - Acceptance: missing WebView2 produces clear prerequisite guidance; normal supported systems launch the portable executable directly.
  - Commits: `39495a4 docs(windows): design portable runtime verification`; `f480499 test(windows): exercise extracted portable runtime`
  - Notes:
    - `npm run verify:windows-runtime` remains available for destructive fresh-profile CI evidence. `npm run verify:windows-manual` provides the same package, desktop, WebView2, memory, sidecar, result, export, cancellation, and shutdown checks locally while preserving existing Tarik AppData and recording whether the profile already existed.
    - The extracted sidecar runs a bounded 100,000-row result/page/release scenario, a completed 250,000-row three-part CSV export, active cancellation of a one-billion-row requested export, zero-residue checks, sidecar memory under 512 MiB, and clean process exit. A schema-versioned report is uploaded even on a reached native failure.
    - Local Windows 10 Enterprise manual automation passed on September 5, 2026: both launches were responsive and graceful with WebView2 152.0.4191.62 at device scale factor 1.5, desktop-tree peak was 374,861,824 bytes, sidecar peak was 20,901,888 bytes, result cache released to zero, exact export partitions passed, cancellation left no residue, and existing AppData was preserved. Visual 150% DPI acceptance remains manual.
    - `docs/review/E12-WINDOWS-RUNTIME.md` is the acceptance matrix. GitHub Actions is not required for acceptance; T4 remains unchecked until the local evidence report and every required manual Windows/DPI/WebView2 row are accepted.

- [ ] **E12-T5 Add portable upgrade, signing, and release documentation**
  - Depends on: E12-T4
  - Deliverables: replacement/upgrade instructions, AppData backup and migration behavior, optional Authenticode signing pipeline, checksum verification, known SmartScreen behavior if unsigned, and uninstall-by-folder-deletion guidance.
  - Acceptance: users know which files are portable, which data stays in AppData, and how to upgrade/remove Tarik safely.
  - Commit: `docs(windows): document portable release lifecycle`

---

## EPIC E13 — Desktop project UX, results correctness, branding, and invisible engine lifecycle revision

**Status:** `IN PROGRESS` — E13-T0 through E13-T5 implemented and focused validation passed September 5, 2026; E13-T6 packaged/manual acceptance remains.

**Outcome:** Project creation uses a native-feeling Tarik modal instead of browser prompts, the active tab never overlays or replaces a real result with the initial empty state, packaged desktop binaries use the supplied Tarik logo, and `tarik-engine-duckdb.exe` remains a strictly invisible, single-instance background sidecar that starts lazily, stays available between project sessions, recovers predictably, and exits with Tarik.

**User-visible process contract:** Only `Tarik.exe` may create a window, taskbar button, Alt+Tab entry, or visible console. `tarik-engine-duckdb.exe` may appear in Task Manager as a child/background process, but must never appear as a second application or flash a console window.

- [x] **E13-T0 Define project-modal and engine-lifecycle design graph**
  - Depends on: E12-T5
  - Owns: E13 interaction/state graph and implementation inventory
  - Deliverables:
    - Map New project modal states: closed, editing, invalid, submitting, failed, succeeded, and cancelled.
    - Map engine states: stopped, starting, ready without session, opening session, connected, closing session, recovering, failed, and shutting down.
    - Define ownership between frontend project state, Tauri `ProjectManager`, `EngineManager`, sidecar process, project session, and coordinated shutdown.
    - Inventory every remaining browser `prompt`, `confirm`, or `alert`; only New project is implementation scope unless a later task explicitly expands it.
    - Reproduce and trace the reported same-tab Results regression where `No results yet / Run a query to see results here` appears while that tab already has a real result; identify whether execution state is lost, the workspace remounts, or packaged UI output is stale before selecting a fix.
  - Acceptance: graph covers success, cancellation, duplicate/invalid names, engine startup failure, crash recovery, project switching, and application shutdown without ambiguous process ownership.
  - Tests: Graph Protocol completeness review and current-code mismatch inventory.
  - Commit: `docs(design): define project modal and engine lifecycle revision`
  - Notes: `docs/review/E13-DESKTOP-UX-LIFECYCLE.md` records the modal, engine, Results ownership, recovery, and shutdown state graphs plus the browser-dialog inventory and same-tab regression trace.

- [x] **E13-T1 Replace New project prompt with an accessible Tarik modal**
  - Depends on: E13-T0
  - Owns: New project trigger, modal component, validation, loading/error UX, and component tests
  - Deliverables:
    - Replace `window.prompt` for New project with a Tarik-styled modal containing project name, managed-storage explanation, Cancel, and Create project actions.
    - Focus the name input on open; Enter submits valid input; Escape cancels; closing restores focus to New project.
    - Trim and validate names before submission, prevent duplicate submits, and present backend duplicate/path/startup failures inline without browser alerts.
    - Show a stable `Creating project…` state while DuckDB starts and the managed project/session opens.
    - Keep external DuckDB selection and Open project naming outside this task unless explicitly approved during E13 review.
  - Acceptance: New project never opens a browser prompt/alert, all states are keyboard and screen-reader accessible, and failure leaves the user’s input available for correction.
  - Tests: open/cancel/submit, autofocus/focus restoration, Enter/Escape, empty/whitespace/duplicate names, submission lock, backend failure, successful catalog transition, and no `window.prompt` call.
  - Commit: `feat(projects): replace new project prompt with modal`
  - Notes: `NewProjectDialog` is a controlled Radix dialog with trimmed/duplicate validation, focus management, keyboard submit/cancel, submission locking, and inline backend errors. Focused frontend acceptance passed 54 tests.

- [x] **E13-T2 Guarantee an invisible Windows DuckDB sidecar process**
  - Depends on: E13-T0
  - Owns: Windows process creation flags, packaged runtime behavior, and process-visibility tests
  - Deliverables:
    - Launch `tarik-engine-duckdb.exe` on Windows with no console window using the appropriate native process creation flags, including `CREATE_NO_WINDOW` or an equivalent verified configuration.
    - Ensure the sidecar creates no top-level window, taskbar button, notification-area icon, Alt+Tab entry, terminal, or startup console flash.
    - Preserve piped stdin/stdout/stderr protocol transport and actionable captured startup errors.
    - Keep Linux behavior unchanged and retain the sidecar as a normal observable process in Task Manager/process inspection.
  - Acceptance: only Tarik appears as an application; the sidecar remains invisible through initial start, project switch, recovery, and shutdown while protocol/error reporting continues to work.
  - Tests: Windows creation-flag unit test, packaged launch process/window enumeration, taskbar/Alt+Tab/manual flash review, handshake/error capture regression, and Linux sidecar regression.
  - Commit: `fix(windows): keep duckdb sidecar process invisible`
  - Notes: Windows process creation now applies `CREATE_NO_WINDOW` while retaining piped protocol streams. The native process test and runtime verifier assert that the sidecar has no main window handle; packaged visual confirmation remains in E13-T6.

- [x] **E13-T3 Formalize lazy start, standby, recovery, and shutdown lifecycle**
  - Depends on: E13-T1, E13-T2
  - Owns: `EngineManager`, project-session transitions, status reporting, recovery, and shutdown tests
  - Deliverables:
    - Do not start the sidecar when Tarik opens with no active project or when the user only focuses/clicks the empty workspace.
    - Start one sidecar lazily on create/open/reopen, perform the protocol handshake, then open exactly one project session.
    - On Close project, close the session but keep the healthy process in standby for fast reuse; switching projects must close the previous session before opening the next.
    - Detect an exited/broken sidecar, clear stale process/session state, restart it invisibly on the next safe engine operation, reopen the active project when possible, and show truthful recovering/failed status.
    - Prevent duplicate concurrent sidecars and ensure coordinated Tarik shutdown closes jobs/results/session, sends engine shutdown, and leaves no `tarik-engine-duckdb.exe` process.
  - Acceptance: workspace focus never starts DuckDB; project operations reuse one invisible process; a killed sidecar recovers without creating duplicates; closing Tarik leaves no sidecar residue.
  - Tests: no-project lazy state, first open, close-to-standby, reopen/switch reuse, concurrent open serialization, forced sidecar exit and recovery, failed restart, status transitions, and graceful/forced app shutdown.
  - Commit: `feat(engine): formalize background sidecar lifecycle`
  - Notes: `EngineManager` now owns one lazy child process, session identity/project/PID state, close-to-standby reuse, health-based stale-state cleanup, next-operation recovery, truthful stopped/standby/connected/failed status, and final process termination. Focused Rust lifecycle and query coordinator tests pass.

- [x] **E13-T4 Fix the same-tab Results empty-state regression**
  - Depends on: E13-T0
  - Owns: active-tab execution/result state selection, Results panel rendering conditions, and regression coverage
  - Deliverables:
    - Reproduce the reported state where a tab with a real query result also shows or falls back to `No results yet / Run a query to see results here`.
    - Fix the root cause so the empty state is rendered only when the active tab has never produced or started an execution and no startup error applies.
    - Preserve the active tab's queued, running, failed, cancelled, statement-completion, and row-grid states across Results collapse/expand and unrelated UI rerenders.
    - Do not hide the symptom with CSS or remove the valid first-use empty state.
  - Acceptance: one active tab can render exactly one truthful Results state at a time; a real result never shares space with or unexpectedly reverts to the initial empty state.
  - Tests: initial empty state, successful same-tab row result, DML completion, collapse/reopen, rerender, tab switch/return, rerun, and mutual-exclusion assertions for every Results state.
  - Commit: `fix(results): prevent same-tab empty-state regression`
  - Notes: The root cause was frontend execution state loss when the workspace was recreated. `QueryCoordinator` now retains the latest project/tab execution, `get_tab_execution` restores it, and the UI renders explicit starting/restoring states instead of falling through to the initial empty state.

- [x] **E13-T5 Replace generated Tauri icons with the supplied Tarik logo**
  - Depends on: E13-T0
  - Owns: canonical desktop icon source, generated Tauri icon set, bundle configuration verification, and packaged branding checks
  - Inputs:
    - Canonical source: `src-tauri/icons/TarikLogo-transparent.png` (`1156x864`, alpha-enabled).
    - Visual reference only: `src-tauri/icons/Tarik Logo.jpeg` (`1200x896`, no alpha).
  - Deliverables:
    - Normalize the transparent logo onto a square transparent canvas with consistent safe-area padding; do not stretch, crop essential artwork, or use the JPEG background in generated application icons.
    - Regenerate the complete Tauri icon family, including PNG sizes, Windows `icon.ico`, macOS `icon.icns`, Windows square assets, and store logo required by the existing bundle configuration.
    - Keep `src-tauri/tauri.conf.json` pointed at the generated Tarik icon outputs and verify `Tarik.exe` embeds the new icon for the file, running window, taskbar, and Alt+Tab surfaces.
    - Complete icon generation before the next Windows package/release command so no new artifact ships with the previous Tauri icon.
  - Acceptance: generated assets consistently show the supplied Tarik logo at small and large sizes without distortion, clipping, opaque JPEG corners, or fallback/default Tauri branding.
  - Tests: source-dimension/alpha check, generated-file inventory, ICO/ICNS decode smoke, Tauri config validation, release build resource inspection, and packaged Windows visual review.
  - Commit: `chore(branding): apply tarik desktop icons`
  - Notes: The supplied transparent artwork was normalized as `TarikLogo-square.png`, then used to regenerate the complete Tauri desktop/mobile icon family. `npm run verify:icons` validates source properties, generated inventory, formats, and Tauri configuration before every Windows release build.

- [ ] **E13-T6 Run desktop UX, Results, branding, and process-lifecycle acceptance**
  - Depends on: E13-T1, E13-T2, E13-T3, E13-T4, E13-T5, E13.5-T5
  - Deliverables: Windows packaged manual matrix plus automated frontend/Rust/process lifecycle, Results regression, and branding evidence.
  - Acceptance:
    - New project modal is accepted in light/dark themes and at 100/125/150/200 percent DPI using pointer and keyboard-only navigation.
    - A successful row result or statement completion in the active tab never displays or reverts to the initial `No results yet` state during collapse/expand, rerender, tab switching, or rerun workflows.
    - `Tarik.exe`, its window, taskbar button, and Alt+Tab entry use the supplied Tarik logo, with no default Tauri icon remaining in the packaged artifact.
    - Only Tarik appears on the taskbar and in Alt+Tab; no sidecar console flashes during first start, reuse, crash recovery, or shutdown.
    - Task Manager shows at most one sidecar while a project is active or the engine is in standby, and none after Tarik exits.
    - Create, close, reopen, switch, kill/recover, and final shutdown workflows preserve project/session data and report truthful status.
  - Tests: full frontend component suite, Rust workspace lifecycle suite, icon/resource verification, packaged Windows smoke, process/window enumeration, and signed manual checklist.
  - Commit: `test(desktop): accept ux results branding and sidecar lifecycle`
  - Notes:
    - Packaged automated acceptance passed September 5, 2026 for `Tarik-0.1.0-windows-x64-portable.zip`: checksums/extraction, two responsive graceful desktop launches with zero no-project sidecars, WebView2 detection, bounded 100,000-row result paging/release, completed three-part 250,000-row export, one-billion-row cancellation with no residue, sidecar `MainWindowHandle = 0`, bounded memory, clean engine exit, and zero Tarik/sidecar processes afterward.
    - Evidence: `target/windows-manual-evidence/runtime-report.json`; archive SHA-256 `f782ceae64553510a6fbafc050e840e51bf617d332cb98337f82159f7ecf6939`.
    - T6 remains open for human visual/input acceptance of the modal, same-tab Results workflows, packaged EXE/window/taskbar/Alt+Tab icon, no console flash, light/dark, keyboard-only use, DPI matrix, mixed-monitor scaling, and clean Windows 10/11 machines.

---

## EPIC E13.5 — Desktop interaction consistency, saved-query correctness, and engine resource controls

**Status:** `REVIEW` — E13.5-T0 through T4 and the automated/review-ready portion of T5 are implemented. Manual UI and native Windows package acceptance remain open before E13-T6.

**Outcome:** Tarik uses one accessible desktop interaction language instead of browser prompts/confirms, newly created query folders are immediately truthful even when empty, saving the active SQL is a direct toolbar workflow with an explicit name/folder modal, and the status bar reports verified DuckDB resource settings that users can safely customize.

**Design read:** preserve Tarik's calm dense IDE language. Dialogs are compact task surfaces with labels above fields, explicit verbs, inline errors, restrained motion, correct destructive emphasis, keyboard completion, and focus restoration—not generic browser chrome or oversized card layouts.

**Truthfulness contract:** Current source contains no `window.alert`, but it contains five `window.prompt` and twelve `window.confirm` calls. All seventeen browser dialogs are in scope. Native operating-system file/folder pickers remain native. The current `Balanced / 2 threads` footer is hard-coded while the sidecar exposes no resource configuration/readback method; it must not claim a profile until DuckDB confirms effective values.

- [x] **E13.5-T0 Design modal, saved-query, and resource-setting graphs**
  - Depends on: E13-T1, E13-T3, E8-T3, E8-T4
  - Owns: `docs/design/E13-5-DESIGN-GRAPH.md`, interaction inventory, resource semantics, and implementation boundaries
  - Deliverables:
    - Inventory and classify every browser dialog by text-entry, destructive confirmation, unsaved-state confirmation, or potentially mutating SQL confirmation; record the current count and fail validation if a new call appears unnoticed.
    - Define controlled dialog shapes and transitions for closed, editing, invalid, confirming, submitting, failed, succeeded, and externally closed states, including target identity and immutable action payloads.
    - Trace the empty-folder bug from successful `create_query_folder` through refresh/grouping/rendering. Specify that folder visibility depends on folder records, never on whether any saved query exists.
    - Define the direct Save query graph: active project/tab SQL snapshot → modal name/folder selection → validated metadata write → library refresh/status, with no SQL execution, editor mutation, or implicit tab creation.
    - Define typed `EngineResourcePreset(low_memory|balanced|fast|custom)`, requested settings, effective DuckDB settings, pending-apply state, validation failures, active-work refusal, project open/switch, standby, crash recovery, and shutdown behavior.
    - Document that DuckDB `memory_limit` constrains DuckDB working/buffer memory but is not a strict total Tarik process cap; WebView, sidecar overhead, some DuckDB allocations, and bounded result caches remain outside it.
  - Acceptance: Graph Protocol sections cover success, cancellation, stale targets, duplicate submit, backend failure, project switch, active jobs, sidecar restart, persistence/readback mismatch, and focus/resource cleanup before implementation.
  - Tests: source dialog inventory, current folder reproduction, A/E/R/cardinality/boundary completeness, and fixed resource-profile matrix.
  - Commit: `docs(design): define desktop interaction and resource graph`
  - Notes: Completed in `docs/design/E13-5-DESIGN-GRAPH.md`. The audit confirms 17 browser dialogs; identifies the empty-folder render branch; and requires sidecar-level active-job refusal plus DuckDB readback, so UI idleness cannot race queued Actual Flow/query/export work.

- [x] **E13.5-T1 Add controlled accessible prompt and confirmation foundations**
  - Depends on: E13.5-T0
  - Owns: shared Radix dialog primitives, form/confirmation contracts, accessibility behavior, and focused component tests
  - Deliverables:
    - Extend the existing dialog foundation for controlled open state, initial/cancel focus, focus restoration, busy-state close prevention, inline validation/errors, labelled descriptions, and explicit footer actions.
    - Provide a compact text-entry form pattern with label, current/default value, optional contextual detail, Cancel, and an exact submit verb; Enter submits only valid idle forms and Escape cancels only when safe.
    - Provide confirmation variants for normal, warning, and destructive actions. Destructive confirmations initially focus Cancel and name the exact operation/object instead of using generic `OK` or `Confirm` labels.
    - Keep pending action state within the owning feature rather than introducing a global promise-based replacement for browser dialogs; stale or overlapping target identities must not execute.
    - Preserve light/dark/system contrast, minimum viewport containment, reduced motion, keyboard traversal, screen-reader announcement, and pointer behavior.
  - Acceptance: foundations support every inventoried interaction without browser APIs, nested focus traps, lost focus, accidental Enter submission, or modal dismissal during an active commit.
  - Tests: controlled open/close, initial focus, Tab containment, Enter/Escape, click outside, focus restoration, busy/error states, destructive cancel focus, stale target, themes, and minimum viewport.
  - Commit: `feat(ui): add controlled desktop dialog patterns`
  - Notes: Controlled Radix foundations now provide explicit text-entry and confirmation contracts, feature-supplied return focus, valid-idle Enter submission, destructive Cancel focus, busy dismissal prevention, inline operation errors, and compact responsive styling. The full frontend suite passes with 158 tests.

- [x] **E13.5-T2 Replace every browser prompt and confirmation**
  - Depends on: E13.5-T1
  - Owns: project/source/catalog/editor/saved-query/history/export interaction state and regression tests
  - Deliverables:
    - Replace text-entry prompts for externally opened project naming, project rename, query-tab rename, folder creation, and folder rename with feature-owned Tarik form modals that trim/validate input and preserve it after backend failure.
    - Replace confirmations for linked-source removal; table/view deletion; project delete/forget; potentially mutating Run, Actual Flow, and Export; failed-draft tab close; saved-SQL replacement; folder deletion; history retention/clear; and saved-query deletion.
    - Use exact action labels and consequences. Managed delete shows the project name/path; external forget and linked-source removal explicitly preserve user-owned files; mutation dialogs explain that execution may change project data/catalog.
    - Capture immutable target IDs and SQL snapshots when opening a confirmation; changing active tabs, project, catalog, or selection while a modal is open must cancel/refuse stale execution rather than act on the wrong target.
    - Retain native operating-system file/folder pickers and the existing coordinated shutdown recovery boundary.
    - Add a repository guard that rejects production `window.alert`, `window.prompt`, or `window.confirm` usage.
  - Acceptance: no production browser dialog API remains; all seventeen known interactions complete/cancel/fail accessibly and execute at most once against the target shown to the user.
  - Tests: each inventoried trigger, exact visible consequence, validation/backend failure retention, immutable target/SQL, duplicate submit, keyboard/focus, project/tab switch races, repository guard, and native picker preservation.
  - Commit: `refactor(ui): replace browser dialogs with Tarik modals`
  - Notes: All five prompts and twelve confirmations now use feature-owned controlled intents with immutable project/tab/source/catalog/folder/query/SQL/options payloads, exact action labels, stale-target checks, inline failure retention, and at-most-once busy guards. A Node repository guard reports zero production browser dialogs while preserving Tauri OS pickers; 30 Node and 158 UI tests pass.

- [x] **E13.5-T3 Fix empty folders and add the direct Save query workflow**
  - Depends on: E13.5-T1, E8-T3
  - Owns: QueryWorkspace toolbar, save modal, Query Library folder rendering/refresh, metadata command integration, and saved-query tests
  - Deliverables:
    - Fix Query Library rendering so a successfully created folder appears immediately even when the project has zero saved queries, the folder itself is empty, or other queries are filtered out by search.
    - Keep empty folder rows visible with a truthful `No saved queries` state and working Rename/Delete actions; distinguish global no-folders/no-queries state from an empty folder.
    - Add a first-class **Save query** button immediately beside **Query library** in the editor toolbar. Disable it when no project is open or the active SQL is blank.
    - Open a focused Tarik modal containing required **Query name** and **Folder** selection (`Unfiled` default), plus an accessible path to create a folder without abandoning the save draft.
    - Capture the active tab's title and immutable SQL snapshot when the modal opens. Show a bounded read-only SQL preview or clear snapshot summary; do not allow subsequent editor changes to silently change what will be saved.
    - Save creates a new saved query only after explicit submission. It never executes SQL, changes the editor, opens a tab, or overwrites an existing saved query implicitly; duplicate-name behavior follows the existing metadata contract and is explained inline.
    - On success close the save modal, refresh the library/folder data, and announce a restrained success status including the saved name/folder. On failure keep name, folder, and SQL snapshot intact for retry.
    - Keep **Query library** focused on browse/search/edit/delete/open/history; its existing **Save current** action may remain as a secondary equivalent only if both entry points use the same save-modal behavior and tests.
  - Acceptance: creating the first empty folder displays it immediately; saving from the toolbar into that folder is possible without opening the library first; reopening the library shows the exact saved immutable SQL under the selected folder after restart.
  - Tests: zero-query empty-folder reproduction, multiple empty folders, search behavior, refresh/race/failure, toolbar position/enabled state, modal autofocus/validation/folder creation, immutable SQL after editor changes, no execution/no tab mutation, duplicate names, save failure retry, success announcement, library consistency, and restart persistence.
  - Commit: `fix(saved-queries): restore folders and direct save flow`
  - Notes: Folder records now render independently from saved-query count and remain visible with empty/search states. A Save query button sits directly before Query library and opens a no-execution modal with required name, folder selection, in-flow folder creation, a bounded immutable SQL snapshot, retry-preserving errors, and success announcement. The direct save path never opens or mutates an editor tab; 161 UI tests pass.

- [x] **E13.5-T4 Restore typed customizable DuckDB resource settings**
  - Depends on: E13.5-T0, E13-T3
  - Owns: engine protocol/capabilities, sidecar session configuration/readback, SQLite settings, Tauri commands, status state, and integration tests
  - Deliverables:
    - Restore Low memory (`512 MiB / 1 thread`), Balanced (`2 GiB / 2 threads`), and Fast (`8 GiB / 4 threads`) presets through the sidecar architecture; add Custom memory and thread values as a typed protocol—not interpolated arbitrary SQL.
    - Validate overflow-safe memory units and positive thread counts against documented hard bounds; warn rather than misrepresent when requested values exceed detected physical memory or logical CPUs.
    - Persist one application-wide requested default in SQLite and apply it whenever a project session opens, switches, or is recreated after sidecar recovery. Keep Tarik's spill directory app-owned; arbitrary temporary-directory customization is deferred.
    - Apply a changed setting to an active session only while query, export, Actual Flow, validation, and future profile jobs are idle. Never silently interrupt work; return a structured busy state with Cancel/keep editing guidance.
    - Configure the primary session and prove cloned query/export connections inherit the intended values. Read back DuckDB's effective `memory_limit` and `threads` after application before reporting success.
    - Distinguish requested, pending reconnect/apply, effective, invalid, busy, and failed states. Preserve the previous verified effective settings after a failed application and recover/reapply predictably after a sidecar crash.
    - Never label the value as total Tarik memory. Logs contain preset/error codes and safe numeric limits only, never project SQL or paths.
  - Acceptance: settings survive restart and project switching, effective readback matches every preset/custom value, active work is never interrupted, failures do not create a false footer state, and query/export cancellation/memory bounds remain intact.
  - Tests: preset matrix, minimum/maximum/overflow/unit parsing, CPU/RAM warnings, SQLite persistence/default migration, open/switch/standby/recovery, busy refusal, apply/readback mismatch, cloned query/export connections, cancellation, sidecar failure, and fixed-workload process memory.
  - Commit: `feat(engine): add verified custom resource settings`
  - Notes: A shared protocol contract now enforces canonical Low memory/Balanced/Fast profiles and bounded Custom values. SQLite stores one application-wide request; session open and crash recovery apply it before publishing a session; `session.configure` refuses queued/running sidecar work; bound DuckDB settings are read back before effective status is exposed; cloned query connections report the configured values. Hardware hints use cross-platform logical CPU and physical-memory detection.

- [ ] **E13.5-T5 Add Engine resources modal and run combined critical review** — implementation complete; awaiting explicit manual/native Windows sign-off
  - Depends on: E13.5-T2, E13.5-T3, E13.5-T4
  - Owns: status-bar resource trigger/modal, `docs/review/E13-5-INTERACTIONS-RESOURCES.md`, packaged evidence, and manual sign-off
  - Deliverables:
    - Replace the hard-coded footer text with a keyboard-accessible **DuckDB resources** trigger showing verified preset, effective memory, and effective threads; show `Not configured`, `Applying`, `Pending`, or `Unavailable` truthfully when appropriate.
    - Add a compact resources modal with preset selection and Custom memory/unit/thread fields, clear validation/warnings, current versus requested values, Apply/Cancel, and the statement: `Limits DuckDB working memory. Total Tarik process memory can be higher.`
    - Explain whether Apply is immediate or pending and why active work blocks it. Do not auto-cancel jobs, auto-restart the sidecar, or close the modal on failure.
    - Review all modal interactions and the direct Save query workflow in light/dark/system, pointer and keyboard-only modes, minimum viewport, Windows DPI 100/125/150/200 percent, and mixed-monitor movement.
    - Repackage Windows and rerun sidecar invisibility/single-instance/recovery, results, exports, cancellation, restart persistence, process-tree memory, and zero-residue checks before E13-T6 final acceptance.
  - Acceptance: users can name/confirm/save/manage folders without native browser chrome, can explain and change DuckDB memory/threads safely, and every visible resource value is verified rather than hard-coded; user gives explicit E13.5 sign-off.
  - Tests: full frontend/Rust/docs/package gates, browser-dialog guard, saved-query golden workflow, real-sidecar resource readback, active-job refusal, restart/crash recovery, Windows packaged modal/DPI/focus matrix, and memory/residue report.
  - Commit: `docs(review): accept desktop interactions and resources`
  - Review: `docs/review/E13-5-INTERACTIONS-RESOURCES.md`
  - Notes: The footer now opens a compact DuckDB resources modal and distinguishes checking, pending, effective readback, and unavailable states. Preset/Custom UI, overflow-safe validation, hardware warnings, busy guidance, and the total-process-memory disclaimer are covered by 166 UI tests. Linux production/Rust/real-sidecar gates pass; native Windows DPI/mixed-monitor/package review and explicit user approval remain open, so T5 is intentionally unchecked.

---

## EPIC E14 — Beginner data profiling and quality checks

**Status:** `PLANNED / BLOCKED` — requirements are recorded, but E14 must not start until E13.5-T5 and E13-T6 receive explicit user approval.

**Outcome:** Help beginners answer “What is in this data?”, “Can I trust it?”, and “What SQL proves that?” through explicit, local, cancellable profiles and reusable quality checks. Tarik must teach by showing deterministic SQL and metric provenance rather than hiding behavior behind an AI chat box or silently changing data.

**Product contract:** Profiling and checks are read-only analytical actions. Every metric is labeled **Exact**, **Approximate**, or **Sampled**; unavailable metrics explain why. Generated SQL is visible and may be copied or opened in an editor without execution. Definitions and bounded run summaries live in SQLite, analytical scans run in the DuckDB sidecar, failing-row previews use ephemeral bounded result pages, and logs contain no generated/custom SQL, accepted values, samples, or failing rows.

- [ ] **E14-T0 Design profiling and quality-check semantics**
  - Depends on: E13-T6 approval, E4-T3, E6-T4, E8-T4
  - Owns: `docs/design/E14-DESIGN-GRAPH.md`, domain vocabulary, provenance rules, resource budgets, and privacy boundaries
  - Deliverables:
    - Define `ProfileRequest`, `ProfileMetric`, `MetricProvenance(exact|approximate|sampled)`, `ProfileSnapshot`, `QualityCheckDefinition`, immutable `CheckRevision`, `CheckRun`, `CheckOutcome(pass|fail|error|cancelled)`, and bounded `FailurePreview` shapes.
    - Draw the complete Graph Protocol flow for profile and check execution, including async submission/status/cancellation, project/session ownership, catalog/source changes, missing links, stale responses, sidecar restart, and application shutdown.
    - Specify type-applicable profile metrics: row count, NULL count/rate, approximate distinct count by default, minimum/maximum, numeric summary, text length, temporal range, bounded common values, and bounded representative values. Unsupported metrics must be absent with a reason, not encoded as zero.
    - Specify first-party check semantics and NULL policy for non-empty table, not-null, unique/composite unique, accepted values, numeric/date range, relationship integrity, freshness, and read-only custom SQL that passes only when it returns zero failing rows.
    - Define scan cost disclosure and exact/approximate choices before execution. Full scans may take time but must remain bounded in desktop memory and cancellable; approximate output must never be presented as exact.
    - Define privacy/retention: profile values and failing rows are ephemeral; SQLite stores bounded definitions and aggregate run facts only; logs retain stable IDs/codes/durations without SQL or data values.
  - Acceptance: every metric/check has explicit SQL semantics, provenance, cardinality, failure strategy, dependency, trust boundary, and resource lifecycle; no design path mutates user data or collects an unbounded result.
  - Tests: Graph Protocol completeness, adversarial NULL/type/catalog matrix, cancellation/resource inventory, and code-to-graph review before implementation.
  - Commit: `docs(design): define profiling and quality check graph`

- [ ] **E14-T1 Add bounded DuckDB profiling primitives**
  - Depends on: E14-T0
  - Owns: adapter-friendly profile protocol, DuckDB profile compiler/executor, bounded status payloads, and profile tests
  - Deliverables:
    - Add explicit table/column profile requests keyed by project, fully qualified catalog object, selected columns, immutable catalog revision, and requested exact/approximate mode.
    - Compile identifiers only through the central quoting boundary; derive metric SQL from trusted typed options rather than concatenating UI text.
    - Execute profile scans asynchronously in the sidecar with queued/running/terminal status, cancellation through DuckDB interruption, fixed deadlines, and no competition that can starve ordinary query/export cancellation.
    - Return only bounded summaries: at most 100 columns per request, 20 common values, and 20 representative values per column; truncate individual displayed values through the existing safe-value policy. Never return a full column or table through status/IPC.
    - Mark every metric with provenance and observation time. Reject stale project/catalog identity and surface linked-source missing/type/read failures without fabricating partial success.
    - Release all profile cursor/cache state on success, error, cancellation, project close, sidecar restart, and Tarik shutdown.
  - Acceptance: a wide or million-row table can be profiled without desktop result growth proportional to row count; cancelling a profile leaves the project session usable and no result artifacts.
  - Tests: empty/all-NULL/mixed tables; boolean, integer, decimal, floating/NaN, text/Unicode/long values, date/timestamp, binary, list/struct, and unsupported types; quoted identifiers; linked Parquet missing mid-run; exact/approximate labels; 100-column/value caps; cancellation; process restart; memory/cache residue.
  - Commit: `feat(profile): add bounded DuckDB data profiling`

- [ ] **E14-T2 Persist project-scoped quality definitions and bounded run history**
  - Depends on: E14-T0
  - Owns: metadata migration 0008, repositories, typed Tauri commands, retention, and compatibility tests
  - Deliverables:
    - Add forward-only SQLite tables for project-scoped check definitions, immutable revisions, and terminal run summaries with foreign keys and explicit cascade behavior.
    - Persist check name, type, target identity, typed bounded options, NULL policy, severity, enabled state, revision, and timestamps. Custom SQL is limited to one read-only statement and follows the same local SQL privacy contract as saved queries.
    - Persist only aggregate run facts: definition/revision, terminal outcome, failure count, duration, observation time, and stable error code. Do not persist common/sample values or failing-row payloads.
    - Enforce bounded configuration sizes: maximum checks per project, accepted-value entries/bytes, composite-key columns, custom SQL bytes, and retained runs per check/project.
    - Add create/update/list/delete/get-history/clear-history commands with exact project isolation. Updating a check creates a new revision; historical runs continue to identify the revision actually executed.
  - Acceptance: definitions and summaries survive restart, never cross projects, upgrade transactionally from schema 7, reject malformed/oversized variants before writes, and remain readable without analytical data.
  - Tests: fresh migration, schema-7 upgrade, newer-schema refusal, CRUD/revisions, project cascade, retention, malformed JSON, size limits, transaction rollback, and Unicode names/options.
  - Commit: `feat(quality): persist check definitions and history`

- [ ] **E14-T3 Execute check suites with immutable SQL and bounded failure previews**
  - Depends on: E14-T1, E14-T2
  - Owns: check SQL compiler, suite coordinator, sidecar jobs, history finalization, and failure-page lifecycle
  - Deliverables:
    - Compile every first-party check into visible, deterministic, safely quoted read-only SQL. Validate custom SQL as exactly one result-producing read-only statement; reject DDL, DML, `COPY`, `ATTACH`, extension installation, and other mutating/external side effects.
    - Run one check or an ordered project suite from immutable definition revisions. Capture one consistent DuckDB read snapshot when supported; otherwise disclose per-check observation boundaries rather than implying atomicity.
    - Produce exact pass/fail semantics, aggregate failure count, duration, and stable errors. Failure examples are a separately requested bounded result with existing 500-row pages and 12-page desktop LRU; the suite never collects all failures.
    - Support queued/running progress, active/queued cancellation, exactly-once terminal SQLite history, and continued session use after fail/error/cancel.
    - Ensure sidecar crash/restart, project close, check deletion, result release, cache cleanup, and application shutdown cannot strand jobs, previews, or duplicate history.
  - Acceptance: a suite with passing, failing, errored, and cancelled checks records one truthful terminal row per started check, exposes only bounded examples, and does not mutate tables or leak resources.
  - Tests: generated SQL golden cases for every check/NULL policy, composite keys, reserved/quoted names, custom mutation sentinels, pass/fail/error/cancel transitions, suite order/snapshot disclosure, exactly-once history, preview paging/release, sidecar crash recovery, and fixed-workload memory/disk budgets.
  - Commit: `feat(quality): run cancellable data quality suites`

- [ ] **E14-T4 Add a beginner-focused Profile workspace**
  - Depends on: E14-T1
  - Owns: Explorer Profile action, profile workspace, provenance/cost language, accessibility, and component tests
  - Deliverables:
    - Add **Profile data** only to supported table/view Explorer context menus and keyboard actions. Opening the workspace does not scan until the user explicitly starts it.
    - Show object identity, source kind/health, scan-cost disclosure, selected columns, exact/approximate choice, Run/Cancel, progress, observation time, and stale state when catalog/source identity changes.
    - Present a calm dense table rather than dashboard cards: metric, value, provenance label, and plain-language meaning. Common/representative values are bounded, copyable, never logged, and hidden until explicitly expanded.
    - Explain beginner concepts inline: NULL versus empty text, distinct versus unique, minimum/maximum, distribution limits, and why approximate values can differ.
    - Offer **Create check** from supported observations such as NULLs, duplicate risk, suspicious range, or stale dates, while requiring review in the check builder before saving or running anything.
    - Preserve keyboard focus, screen-reader labels, light/dark/system legibility, minimum viewport behavior, and tab/workspace state without adding decorative charts or unbounded DOM.
  - Acceptance: a beginner can profile a table, identify one concrete issue, state whether each number is exact/approximate/sampled, cancel a slow scan, and reach a prefilled but unexecuted check.
  - Tests: no-auto-run, empty/loading/progress/success/partial-unavailable/error/cancel/stale states, metric provenance, unsupported types, bounded expansion, context scope, keyboard/focus, theme/minimum viewport, and profile-to-check handoff.
  - Commit: `feat(profile): add beginner data profile workspace`

- [ ] **E14-T5 Add guided quality-check authoring and SQL teaching**
  - Depends on: E14-T2, E14-T3, E14-T4
  - Owns: Checks workspace, typed builder, generated SQL explanation, definition lifecycle, and accessibility tests
  - Deliverables:
    - Add a project **Checks** workspace with bounded searchable definitions, enabled/severity/type/target labels, latest outcome, Run, Edit, Duplicate, and Delete actions.
    - Build each first-party check through plain-language fields and catalog-backed table/column selectors. State exactly what one passing row represents, how NULLs are treated, and what failure means.
    - Show generated SQL and a sentence-by-sentence explanation before Save or Run. **Open SQL in new tab** and Copy never execute; edits in the new tab do not silently alter the saved check.
    - Validate duplicate names, missing/stale objects, type-incompatible thresholds, accepted-value limits, relationship key arity/types, freshness units, and custom read-only SQL inline while preserving input after failure.
    - Require explicit confirmation before running custom SQL or a suite containing it. Never auto-repair data, auto-apply a suggested check, or silently rewrite user SQL.
  - Acceptance: a beginner can create not-null, unique, relationship, and freshness checks from the catalog, explain the generated SQL at a basic level, save them, and reopen them after restart without any execution occurring implicitly.
  - Tests: every builder variant and NULL policy, catalog refresh/stale targets, profile prefill, validation/limits, generated SQL display, copy/open-without-run, save/revision/restart, custom mutation rejection/confirmation, keyboard/focus, themes, and minimum viewport.
  - Commit: `feat(quality): add guided check builder`

- [ ] **E14-T6 Present check runs, bounded failures, and learning-oriented recovery**
  - Depends on: E14-T3, E14-T5
  - Owns: suite progress, latest/history views, failure examples, rerun behavior, and recovery UX
  - Deliverables:
    - Show suite and individual status using text plus color: queued, running, passed, failed, error, and cancelled; distinguish a valid failed assertion from an execution error.
    - Present observed versus expected facts in beginner language, for example `14 rows have NULL customer_id; expected 0`, with exact/approximate disclosure inherited from the check semantics.
    - Browse bounded failing rows in the existing virtualized result grid, with immutable generated/custom SQL available for inspection and explicit rerun. Changing a definition never changes a historical run snapshot.
    - Provide actionable deterministic recovery for missing objects/columns, type changes, missing linked files, invalid thresholds, and sidecar interruption. Suggestions may open Edit, Profile, Repair link, or SQL; none execute automatically.
    - Show bounded history and simple outcome/duration trends without retaining or reconstructing historical row payloads. Clearing history does not delete definitions, projects, sources, or completed exports.
  - Acceptance: users can distinguish data failure from engine failure, inspect bounded examples, understand the expectation, correct the source/check, rerun explicitly, and verify a later pass without losing prior aggregate evidence.
  - Tests: mixed suite states, observed/expected wording, paged failure rows, truncation/NULL fidelity, immutable rerun, definition revision mismatch, restart restoration, missing-link repair, sidecar recovery, history retention/clear isolation, keyboard/focus, and zero residual preview cache.
  - Commit: `feat(quality): explain check failures and recovery`

- [ ] **E14-T7 Review the complete beginner data-trust workflow**
  - Depends on: E14-T4, E14-T5, E14-T6
  - Owns: `docs/review/E14-PROFILES-QUALITY.md`, sample issue fixture, automated evidence, and manual sign-off
  - Deliverables:
    - Provide a small local fixture with intentional NULLs, duplicate keys, out-of-range values, stale dates, and unmatched relationships; document expected profile metrics and check outcomes.
    - Walk through Profile table → interpret provenance → create suggested check → inspect generated SQL → save → run suite → inspect bounded failures → repair data/check → rerun → restart/reopen.
    - Re-run query/result/export cancellation, sidecar recovery, memory, cache cleanup, migration, log-redaction, Windows process invisibility, and packaged Linux/Windows smoke because profiling/checks add analytical jobs and metadata.
    - Include light/dark/system and 100/125/150/200% DPI screenshots, keyboard-only flow, screen-reader labels, minimum viewport, large-table cancellation, and fixed-workload memory/disk reports.
  - Acceptance: a beginner can explain what the profile measured, why a check passed or failed, what SQL established the result, and how to recover—without developer help, hidden execution, false exactness, data mutation, or unbounded memory/disk growth.
  - Tests: full frontend/Rust/docs/package gates plus profile/check golden workflow, fresh/schema-7 migration, real sidecar restart, Windows/Linux filesystem paths, large-table memory, repeated suite retention, cancellation/residue, and manual review checklist.
  - Commit: `docs(review): add beginner data trust checklist`
  - Review: `docs/review/E14-PROFILES-QUALITY.md`

**Explicitly deferred beyond E14:** Join/grain coaching and transformation-pipeline DAGs may be planned after E14 review. Generic AI chat, cloud integrations, automatic data repair, and silent SQL rewriting are not part of this EPIC.

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
- [x] **D2:** Pin `duckdb` Rust binding `1.10505.0`. Normal development no longer uses `bundled`; the DuckDB adapter links a pinned prebuilt dynamic library. Bounded Arrow transport is implemented by the DuckDB adapter page writer and desktop bounded reader; E12-T0 removed the earlier unreferenced `crates/arrow-page-format` placeholder.
- [ ] **D3:** Choose the Arrow batch transport strategy across Tauri IPC after measuring JSON vs binary transfer overhead.
- [ ] **D4:** Define default result page size, cache size, and worker count from E11-T2 measurements rather than guesses.
- [ ] **D5:** Define whether “empty export” creates no files or one schema-only file; document consistently.
- [x] **D6:** Ownership-safe project removal: Tarik-managed projects may delete their managed directory after explicit confirmation; externally opened DuckDB files can only be forgotten from Tarik metadata and are never deleted, renamed, or moved automatically.

Do not resolve an open decision implicitly inside unrelated code. Record the decision here and in the implementing commit.

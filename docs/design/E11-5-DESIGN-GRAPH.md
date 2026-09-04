# E11.5 desktop interaction fixes design graph

PROBLEM: Correct Tarik's theme, table creation, context commands, bounded result selection, and connection-state behavior before Windows work.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: React, CodeMirror, Radix, Tauri invoke, DuckDB sidecar
│ │ │ └──── E: invalid input, clipboard denial, engine failure, stale result
│ │ └─────── A: EffectiveTheme, TableDefinition, ContextTarget, Selection, QuerySnapshot, EngineState
│ │
│ └─ nodes = functions, edges = data flow
│
└─ problem: make desktop behavior native, bounded, accessible, and truthful

SHAPES: ThemePreference(system|light|dark), EffectiveTheme(light|dark), TableDefinition(name, columns), TableColumn(name, type, nullable), ContextTarget(editor|result-cell|project-row|unsupported), CellCoordinate(pageOffset, row, column), CellSelection(anchor, rectangularRange, toggledCells), ClipboardPayload(TSV), QuerySnapshot(tabId, sql, resultId), EngineState(idle|connecting|connected|failed), InteractionError

GRAPH:

```text
read preference (1) → resolve effective theme (T) → apply shell tokens (T)
│ R: preference store     │ R: matchMedia               │ R: CSS custom properties
│ E: read failure ↯escape(system)                        └─ A: readable result colors
│                         └─ E: unavailable media query ↯escape(light)
└─ 🔒 persisted string → ThemePreference

resolve effective theme (T) → reconfigure editor theme (T)
│ R: CodeMirror Compartment, Dracula extensions
├─ A: EffectiveTheme → transaction effects
└─ E: destroyed view ↯escape(no-op)

open New table (1) → parse form (1) → create table command (1) → refresh catalog (1)
│ R: active project       │ R: allow-listed types        │ R: Tauri invoke      │ R: catalog service
│ E: no project ↯escape(disabled)                        │ E: command failure ↯escape(dialog error)
│                         └─ 🔒 form strings → TableDefinition
│                                                        ↓
│                                              validate active project (1)
│                                              │ R: ProjectManager
│                                              └─ E: wrong project ↯escape(reject)
│                                                        ↓
│                                              sidecar create table (1)
│                                              │ R: open DuckDB session, identifier quoter
│                                              ├─ 🔒 protocol JSON → TableDefinition
│                                              └─ E: duplicate/DDL error ↯escape(structured error)
└─ dialog stays open on recoverable E; success creates no SQLite source record

capture contextmenu (N) → classify target (1) → editor/result/project/unsupported branch (1)
│ R: shell listener        │ R: data-context attributes
│ └─ 🔒 DOM event target → ContextTarget
├─ unsupported → prevent native menu (1) · R: Event API · E: none
├─ editor → preserve editor selection (1) → show editor commands (1) → run immutable editor SQL (1)
│            R: CodeMirror                  R: Radix menu             R: shared submit path
│            E: destroyed view ↯escape      E: focus loss ↯escape    E: mutation declined ↯escape(no run)
├─ project row → select project row (1) → show project actions (1)
│                 R: explorer state          R: Radix menu
└─ result cell → update selection from modifiers (1) → show result commands (1)
                  │ R: current page                         │ R: Radix menu, clipboard
                  ├─ E: stale coordinate ↯escape(clear)
                  └─ A: CellSelection

CellSelection (1) → materialize selected current-page cells (1) → serialize TSV (1) → clipboard write (1)
│ R: page rows/columns      │ R: bounded 500-row page          │ R: Clipboard API
├─ E: page changed ↯escape(clear)                            └─ E: denied ↯escape(feedback)
└─ no hidden-page fetch and no DOM-node ownership

result context command (1) → retrieve QuerySnapshot (1) → shared submit (1) → replace result lifecycle (N)
│ R: execution state         │ R: frontend immutable snapshot  │ R: query coordinator
├─ E: no snapshot ↯escape(disabled)
├─ E: mutation declined ↯escape(no run)
└─ A: exact SQL that produced the displayed result, never current edited SQL

project command (1) → EngineState(connecting) (1) → sidecar start/handshake/session open (1)
│ R: ProjectManager                         │ R: EngineManager, protocol version, DuckDB file
├─ E: failure → EngineState(failed) (1)      └─ A: EngineState(connected) only after all steps succeed
└─ close → release results/session (1) → EngineState(idle) (1)

EngineState (T) → render labelled connection orb (T)
│ R: semantic tokens, reduced-motion query
└─ A: size/shine/color plus visible status text; color is never the sole signal
```

CARDINALITY: read preference (1) · resolve/apply effective theme (T) · reconfigure editor (T) · open/parse/create/refresh table (1) · capture contextmenu (N) · classify/branch/menu command (1) · update/materialize/serialize/copy selection (1) · result lifecycle (N) · engine transitions (1) · render engine state (T)

BOUNDARIES: persisted theme string 🔒 ThemePreference · `matchMedia` result 🔒 EffectiveTheme · New table form strings 🔒 TableDefinition · Tauri JSON 🔒 protocol TableDefinition · DOM event target/modifiers 🔒 ContextTarget and CellCoordinate · current result metadata 🔒 QuerySnapshot · Clipboard API accepts only bounded serialized text · project metadata does not imply an open engine session

BEHAVIOR: ⛈ accessibility wraps dialog, context menus, cells, and status text · ⛈ diagnostics maps recoverable failures to inline feedback · ⛈ mutation confirmation wraps shared explicit execution · ⛈ reduced motion wraps connection transition · ⛈ virtualization wraps result rendering without changing coordinate selection

SCOPE: media-query listener acquire@theme hook → release@theme hook · CodeMirror view/Compartment acquire@SqlEditor → destroy@SqlEditor · shell context listener acquire@App → release@App · Radix menu acquire@trigger → dismiss@selection/Escape · result selection acquire@published page → clear@page/result replacement · DuckDB session acquire@project open → release@project close/shutdown

TEST LAYERS: R = {matchMedia: mutable fake, CodeMirror: mounted editor, Tauri invoke: typed spy, ProjectManager: isolated database, engine: real sidecar plus rejecting fake, clipboard: recording/rejecting fake, result page: bounded fixture, reducedMotion: true/false}; same graph and command paths, no test-only behavior nodes

VERDICT: Current code only applies CSS variables for system dark, uses CodeMirror's default highlight style, has an inert New table button, allows native menus on unsupported surfaces, attaches explorer menus beyond project rows, stores no cell selection or query snapshot for rerun, and equates project metadata with engine readiness. The intended graph separates input/DOM/protocol boundaries from trusted shapes, keeps failures at UI/command joins, bounds result copying to one page, and ties every listener/process state to lifecycle scope. Implementation must close each mismatch before E11.5 review.

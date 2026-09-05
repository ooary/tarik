# E13.5 desktop interactions and DuckDB resources design graph

## Design read

Reading this as a dense local SQL workbench for beginner data engineers, with Tarik's calm, precise desktop language and no decorative modal treatment.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 8`
- Foundation: existing Radix Dialog primitives, semantic CSS tokens, typed Tauri commands, and the DuckDB sidecar protocol
- Form rule: visible label above every input, error below its field or at the operation join
- Destructive rule: exact verb and target, Cancel receives initial focus

PROBLEM: Replace browser dialogs, make saved-folder/save behavior truthful, and let users safely persist and verify DuckDB memory/thread settings without interrupting active work.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: React feature state, Radix Dialog, SQLite, Tauri commands, EngineManager, DuckDB sidecar
│ │ │ └──── E: invalid/stale input, duplicate submit, metadata/engine failure, active work, readback mismatch
│ │ └─────── A: DialogIntent, ImmutableTarget, QuerySnapshot, RequestedResources, EffectiveResources
│ │
│ └─ nodes = functions, edges = data flow
│
└─ problem: desktop interaction consistency, saved-query correctness, and truthful engine resources

## SHAPES

- IDs: `ProjectId`, `TabId`, `SourceId`, `FolderId`, `SavedQueryId`, `CatalogObjectId(database,schema,name,kind)`
- Dialog records:
  - `TextDialogIntent(kind, target, initialValue, title, description, label, submitLabel)`
  - `ConfirmDialogIntent(kind, immutableTarget, title, consequence, confirmLabel, tone)`
  - `DialogState = closed | editing | invalid | confirming | submitting | failed`
  - `DialogError(code, message, field?)`
- Saved-query records:
  - `SaveQuerySnapshot(projectId, tabId, suggestedName, sqlText)`
  - `SaveQueryDraft(snapshot, name, folderId|null)`
  - `FolderCollection(folders)` independent of `SavedQueryCollection(queries)`
- Resource records:
  - `EngineResourcePreset = low_memory | balanced | fast | custom`
  - `EngineResourceSettings(preset, memoryLimitMiB, threads)`
  - `EffectiveEngineResources(preset, memoryLimitMiB, memoryLimitDisplay, threads)`
  - `ResourceStatus(requested, effective|null, state = not_configured | applying | pending | effective | unavailable)`
  - `ResourceEnvironment(logicalCpuCount|null, physicalMemoryMiB|null)`
- Errors: `DialogCancelled`, `InvalidName`, `StaleTarget`, `DuplicateSubmit`, `MetadataFailure`, `ResourceInvalid`, `ResourceBusy`, `SessionMissing`, `EngineUnavailable`, `ReadbackMismatch`, `PersistenceFailure`

Preset matrix:

| Preset     | DuckDB memory limit | Threads |
| ---------- | ------------------- | ------- |
| Low memory | 512 MiB             | 1       |
| Balanced   | 2,048 MiB           | 2       |
| Fast       | 8,192 MiB           | 4       |
| Custom     | 128–262,144 MiB     | 1–256   |

The limits are protocol hard bounds, not recommendations to allocate that amount. Tarik warns when a requested value exceeds detected device memory or logical CPUs. Missing hardware information is represented as unavailable, never guessed.

## Existing interaction inventory

Production source currently contains zero `window.alert`, five `window.prompt`, and twelve `window.confirm` calls. Native OS file/folder selection through `@tauri-apps/plugin-dialog` is retained.

| Owner               | Current browser call                      | Class                      | Immutable target/payload                                  | Exact primary action  |
| ------------------- | ----------------------------------------- | -------------------------- | --------------------------------------------------------- | --------------------- |
| `App`               | name externally opened project            | text entry                 | selected DuckDB path + default name                       | Open project          |
| `App`               | rename recent project                     | text entry                 | project ID + old name                                     | Rename project        |
| `QueryWorkspace`    | rename query tab                          | text entry                 | project ID + tab ID + old title                           | Rename tab            |
| `SavedQueryLibrary` | create folder                             | text entry                 | project ID                                                | Create folder         |
| `SavedQueryLibrary` | rename folder                             | text entry                 | project ID + folder ID + old name                         | Rename folder         |
| `App`               | remove linked source                      | destructive confirmation   | project ID + source ID/name/path-preservation fact        | Remove link           |
| `App`               | drop table or view                        | destructive confirmation   | fully qualified catalog object + source preservation fact | Delete table/view     |
| `App`               | delete managed or forget external project | destructive confirmation   | project ID/ownership/name/path                            | Delete/Forget project |
| `QueryWorkspace`    | run potentially mutating SQL              | mutation confirmation      | project ID + tab ID + SQL snapshot                        | Run query             |
| `QueryWorkspace`    | run potentially mutating Actual Flow      | mutation confirmation      | project ID + tab ID + SQL snapshot                        | Run Actual Flow       |
| `ExportDialog`      | export potentially mutating SQL           | mutation confirmation      | project ID + SQL/options snapshot                         | Start export          |
| `QueryWorkspace`    | close draft after save failure            | unsaved-state confirmation | project ID + tab ID + title                               | Close without saving  |
| `SavedQueryLibrary` | replace stored SQL                        | destructive confirmation   | project ID + saved-query ID + old/new SQL snapshot        | Replace saved SQL     |
| `SavedQueryLibrary` | delete folder                             | destructive confirmation   | project ID + folder ID/name                               | Delete folder         |
| `SavedQueryLibrary` | apply history retention                   | destructive confirmation   | project ID + policy snapshot                              | Apply retention       |
| `SavedQueryLibrary` | clear project history                     | destructive confirmation   | project ID                                                | Clear history         |
| `SavedQueryLibrary` | delete saved query                        | destructive confirmation   | project ID + saved-query ID/name                          | Delete saved query    |

Repository validation scans production TypeScript/JavaScript sources and fails on `window.alert(`, `window.prompt(`, or `window.confirm(`. It does not reject Tauri's native file/folder picker.

## GRAPH

### Controlled text-entry and confirmation interactions

```text
feature event (N) → capture intent + immutable target (1) → render controlled dialog (T)
│ R: feature-owned reducer/state                         │ R: Radix Dialog, semantic tokens
│ E: no active target ↯escape(disabled/no dialog)        │ E: external close while idle ↯escape(cancel)
│ 🔒 DOM/context target → TextDialogIntent|ConfirmDialogIntent
│                                                        ↓
│                                      focus initial control + trap (1)
│                                      │ R: Radix focus scope, trigger ref
│                                      └─ E: removed trigger ↯escape(focus nearest owner)
│                                                        ↓
│                                      parse user input/action (N)
│                                      │ R: intent validator
│                                      ├─ E: invalid ↯escape(inline error, retain input)
│                                      ├─ E: Enter on invalid/busy ↯escape(no submit)
│                                      └─ 🔒 input string → trimmed domain value
│                                                        ↓
│                                      verify current identity (1)
│                                      │ R: current project/tab/catalog collections
│                                      └─ E: stale target ↯escape(close/refuse with status)
│                                                        ↓
│                                      submit once (1) → refresh owner data (1) → close + restore focus (1)
│                                      │ R: typed command, submitting guard
│                                      ├─ E: duplicate submit ↯escape(existing request)
│                                      └─ E: command failure ↯escape(failed, retain state)
└─ dialog state is owned by the feature that owns the operation; no global promise queue
```

For destructive confirmation, `render controlled dialog` initially focuses Cancel. For valid text forms, the first field receives focus. Escape and outside interaction cancel only while idle. While submitting, close controls, Escape, outside interaction, and primary re-submission are blocked.

### Saved folders and direct Save query

```text
open library/create folder (N) → load folders + filtered queries (1) → render folder collection (T)
│ R: QueriesRepository                                      │ R: folder records, grouped query map
│ E: metadata failure ↯escape(inline error)                  ├─ A: every folder renders even with zero rows
│                                                           └─ search filters query rows, not folder identity
│
└─ current defect: render branches on `queries.length === 0`, hiding valid `folders`

click Save query (N) → capture active editor snapshot (1) → load folders (1) → edit save draft (N)
│ R: active project/tab │ R: immutable string copy           │ R: QueriesRepository
│ E: blank SQL/no project ↯escape(disabled)                  │ E: load failure ↯escape(retryable dialog error)
│ 🔒 live editor state → SaveQuerySnapshot                   ↓
│                                                parse name + folder (1)
│                                                │ E: blank/unknown folder ↯escape(inline error)
│                                                └─ 🔒 form values → SaveQueryDraft
│                                                           ↓
│                                                create saved query once (1)
│                                                │ R: typed Tauri command, SQLite transaction
│                                                ├─ E: duplicate name ↯escape(retain draft)
│                                                ├─ E: stale project/tab ↯escape(refuse)
│                                                └─ A: SQL equals captured snapshot, never later editor text
│                                                           ↓
│                                                refresh shared library revision (1) → announce success + close (1)
```

Saving does not call query execution, mutate editor text, select/open a tab, or overwrite an existing saved query. Folder creation inside the save dialog preserves the save snapshot/name, refreshes folder choices, and selects the created folder.

### Persisted and verified DuckDB resources

```text
application setup (1) → load requested resources (1) → parse/default (1) → construct EngineManager (1)
│ R: SettingsRepository                         │ R: protocol validator
│ E: missing ↯escape(Balanced request)           └─ E: corrupt/invalid ↯escape(Balanced + unavailable warning)
│ 🔒 SQLite JSON → EngineResourceSettings
│
project open/recovery (N) → session.open(requested resources) (1) → validate + open DuckDB connection (1)
│ R: ProjectManager          │ R: EngineProcess              │ R: SessionManager, app-owned spill root
│ E: engine unavailable ↯escape(no effective value)          ├─ E: invalid/apply failure ↯escape(close connection)
│                                                            └─ 🔒 protocol JSON → validated settings
│                                                                       ↓
│                                                            parameterized SET memory/threads (1)
│                                                            │ R: DuckDB connection
│                                                            └─ E: DuckDB setting error ↯escape(close connection)
│                                                                       ↓
│                                                            current_setting readback (1)
│                                                            │ E: parse/mismatch ↯escape(no session publish)
│                                                            └─ A: EffectiveEngineResources
│                                                                       ↓
└──────────────────────────────────────────────────────────── publish session + effective status (1)

open resource modal (N) → copy requested draft (1) → choose preset/custom (N) → validate (1)
│ R: ResourceStatus, detected hardware                              │ R: protocol validator
│ E: unavailable status ↯escape(edit still allowed)                 ├─ E: hard bound ↯escape(field error)
│                                                                  └─ A: warning if above detected RAM/CPU
│                                                                             ↓
│                                                                  apply resource request (1)
│                                                                  │ R: QueryCoordinator, ExportCoordinator, EngineManager
│                                                                  ├─ E: query/export active ↯escape(ResourceBusy)
│                                                                  └─ state = applying
│                                                                             ↓
│                                                                  sidecar session.configure (1)
│                                                                  │ R: JobRegistry, ExportRegistry, SessionManager
│                                                                  ├─ E: queued/running job ↯escape(ResourceBusy)
│                                                                  ├─ E: no session ↯escape(pending request)
│                                                                  └─ no job cancellation/restart
│                                                                             ↓
│                                                                  parameterized apply + readback (1)
│                                                                  ├─ E: apply/readback mismatch ↯escape(previous verified status)
│                                                                  └─ A: EffectiveEngineResources
│                                                                             ↓
│                                                                  persist requested JSON (1) → publish effective status (1)
│                                                                  │ R: SettingsRepository
│                                                                  └─ E: persistence failure ↯escape(report failure; keep runtime truth)
```

Application-wide semantics:

1. The requested setting is one local application default, not project metadata.
2. With no session, Apply persists it as `pending`; the next project open verifies it.
3. With a session, Tarik persists only after the sidecar applies and reads it back. If persistence fails after a successful runtime apply, the UI reports that split truth and offers retry; it does not claim durable success.
4. Session open and crash recovery pass the manager's requested setting into the same typed sidecar path before publishing the session.
5. Sidecar `session.configure` refuses while any query/Actual Flow/export job for that session is queued or running. Synchronous validation already serializes through the engine process request lock.
6. Query/export connection clones are created only after the primary connection is configured and are integration-tested for matching `current_setting` values.
7. Tarik's spill root remains app-owned and is never accepted from frontend input.

## CARDINALITY

Feature event (N) · capture intent/target (1) · render dialog (T) · focus/trap (1 per open) · parse input (N) · verify identity (1) · submit/refresh/close (1) · load/render folders (N/T) · capture query snapshot (1) · create saved query (1) · load persisted resources (1) · project open/recovery (N) · apply/readback resources (1 per request/open) · render resource status (T).

## BOUNDARIES

- DOM/context events 🔒 closed `DialogIntent` variants with immutable IDs/payloads.
- Text fields 🔒 trimmed non-empty names with existing metadata byte/uniqueness validation retained server-side.
- Live editor state 🔒 immutable `SaveQuerySnapshot` and mutation-confirmation SQL snapshots at dialog-open time.
- SQLite `settings.value_json` 🔒 validated `EngineResourceSettings`; missing/corrupt settings never become unchecked SQL.
- Frontend resource form 🔒 typed preset, integer MiB, and integer thread fields at Tauri command decoding/validation.
- Engine newline JSON 🔒 the same protocol resource type and validator.
- DuckDB `current_setting` strings 🔒 parsed effective MiB/thread values; unparseable or mismatched values are errors, not labels.
- OS file/folder pickers remain the trusted Tauri plugin boundary and are outside browser-dialog replacement.

## BEHAVIOR

- ⛈ accessibility wraps every dialog: labelled title/description, focus containment/restoration, initial-focus policy, status/error announcements, and keyboard semantics.
- ⛈ submitting guards wrap mutations without changing their typed command graph.
- ⛈ stale-target checks wrap destructive and SQL actions immediately before submission.
- ⛈ repository source scan prevents browser-dialog regression.
- ⛈ resource warnings wrap validation and remain advisory; hard protocol bounds remain authoritative.
- ⛈ structured logging records operation, preset, safe numeric limits, status, and error code only—never SQL, source/project paths, names, or result values.
- ⛈ status polling reports engine/resource truth but does not mutate requested settings.

## SCOPE

- Dialog focus scope acquire@open → release/restore@cancel|success|owner-unmount.
- Dialog immutable target/snapshot acquire@open → release@cancel|success|project change|owner-unmount.
- Search/load debounce acquire@library open/change → clear@rerender/unmount.
- Save draft acquire@Save query open → release@cancel|success|project change.
- DuckDB primary connection acquire@session.open → release@failed configuration|session.close|shutdown.
- Query/export cloned connection acquire@job claim → release@terminal state.
- Engine process acquire@first operation → standby@session close → release@app shutdown.
- Requested resource state acquire@app setup → replace@verified/persisted Apply → release@app shutdown.
- Effective resource state acquire@successful readback → clear@process/session loss → replace@next verified readback.

## TEST LAYERS

R = {

- Dialog primitive: mounted Radix portal plus controllable owner state,
- Feature commands: resolving/rejecting typed invoke spies,
- Identity source: mutable project/tab/folder/catalog fixtures,
- Metadata: in-memory SQLite and reopenable temporary SQLite,
- Engine process: scripted protocol process and real DuckDB sidecar,
- Jobs/exports: queued/running/terminal registries with deterministic waits,
- Hardware environment: known/unknown RAM and CPU fixtures,
- Themes/viewports: light/dark/system, reduced motion, minimum viewport, Windows DPI manual matrix

}; same graphs, no browser dialog stubs or test-only execution paths.

Required assertions:

- Inventory: 0 browser calls after migration and native picker calls still present.
- Dialogs: focus trap/restore, destructive Cancel focus, valid Enter, idle Escape/outside close, busy close refusal, exact action text, input retention, stale target refusal, and at-most-once commit.
- Saved queries: first empty folder, multiple empty folders, search with zero matches, folder-create failure, direct immutable save, no execution/tab mutation, duplicate name, refresh, and SQLite restart.
- Resources: preset/hard-bound matrix, typed serde, default/corrupt persistence, no-session pending state, open/switch/recovery application, busy refusal for query/Actual Flow/export, no cancellation, cloned connection readback, apply/readback/persistence failure, and bounded workload memory/residue.

## VERDICT

The current implementation does not match this graph. Browser modal APIs own seventeen interactions, targets are often read again after confirmation, the Query Library's global `queries.length === 0` branch hides valid empty folders, direct toolbar save does not exist, and the footer reports hard-coded settings absent from the sidecar. The implementation must introduce feature-owned controlled intents, render folder records independently, capture immutable save/action payloads, and add one typed resource contract applied/read back inside the sidecar. Failure joins remain at feature, Tauri-command, engine-manager, and sidecar boundaries; active work is never cancelled as an implementation shortcut. T0 defines the target graph; T1–T5 must reconstruct and compare the resulting code against it.

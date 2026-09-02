# E8 Saved queries and historical executions

## Design read

Dense local SQL workbench for beginner data engineers. Saved SQL and execution history are editor-adjacent tools, not source-catalog objects. Use one compact Query Library dialog with Saved queries and History views, restrained controls, bounded lists, and explicit destructive actions.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 1`
- `VISUAL_DENSITY: 8`
- Foundation: existing semantic tokens, Radix Dialog, Phosphor icons
- Storage: existing project-scoped SQLite metadata only

## PROBLEM

Provide durable saved SQL and a bounded, filterable terminal execution audit trail without accidental overwrite, automatic execution, or cross-project leakage.

```text
X -> DesignGraph<A, E, R>
|              |  |  |
|              |  |  `- R: MetadataDb, active project, editor tab bridge, clock
|              |  `---- E: duplicate name, missing record, stale update, invalid folder, prune failure
|              `------- A: SavedQuery, QueryFolder, HistoryPage, HistoryFilter, RetentionPolicy
|
`- E8 query library
```

## SHAPES

- IDs: `SavedQueryId`, `QueryFolderId`, `HistoryExecutionId`, `ProjectId`
- Records: `SavedQuery`, `SavedQueryDraft`, `QueryFolder`, `QueryHistoryEntry`, `HistoryPage`, `HistoryFilter`, `RetentionPolicy`, `PruneSummary`
- Variants:
  - `HistoryStatus = succeeded | failed | cancelled`
  - `LibraryView = saved | history`
  - `SaveIntent = create | explicitUpdate`
  - `Retention = maxAgeDays? + maxCount?`
- Errors: `DuplicateSavedName`, `SavedQueryMissing`, `FolderMissing`, `ProjectMismatch`, `InvalidRetention`, `MetadataError`

## GRAPH

```text
open_library (1)
| R: active project, editor active SQL snapshot
| E: no project -> escape(disabled action)
v
load_saved(project, search?) (T)
| R: MetadataDb
| boundary: SQLite rows + tags JSON -> SavedQuery[]
| E: invalid stored tags -> escape(structured local-data error)
v
create_saved(draft) (1) -----------------------------> refresh_saved (1)
| boundary: form values -> normalized non-empty name/SQL/tags
| E: duplicate name -> escape(inline conflict)
| R: UUID + clock
|
`- update_saved(existingId, full replacement) (1)
   | R: explicit selected record + user confirmation when SQL changes
   | E: missing/stale/project mismatch -> escape(inline error)
   `-> refresh_saved

folder CRUD (1 each) -> refresh_saved
| E: duplicate name/missing folder -> escape(inline error)
| deleting folder uses existing ON DELETE SET NULL boundary

open_saved(saved) (1)
| R: editor bridge
`-> new editor tab with immutable SQL snapshot; never execute

load_history(filter, cursor/offset, bounded limit) (T)
| R: MetadataDb
| boundary: filter/status/time/text -> parameterized SQL
| E: invalid filter -> escape(empty/error state)
v
history_page(entries, total/hasMore) (1)
`- reopen_history(entry) (1) -> new editor tab; never execute

apply_retention(project, maxAgeDays?, maxCount?) (1)
| R: immediate SQLite transaction + clock
| E: invalid policy -> escape(inline error)
`-> delete only query_history rows -> PruneSummary

clear_history(project) (1)
| R: explicit confirmation + immediate transaction
`-> delete only query_history rows -> PruneSummary
```

## CARDINALITY

`open_library (1)`; `load_saved (T)`; create/update/delete/folder mutations `(1)`; `load_history (T)`; `history_page (1 page, N rows bounded)`; reopen `(1)`; retention/clear `(1)`.

## BOUNDARIES

- Active project ID scopes every list and mutation.
- Form input is normalized before command invocation: trimmed name, non-empty SQL, deduplicated trimmed tags.
- SQLite tags JSON is parsed once into typed tags.
- History status/time/text filters become SQL parameters, never string interpolation.
- Editor reopen receives SQL text only; no execution call is reachable from this action.
- Explicit create and update commands replace generic upsert at the user-facing boundary.

## BEHAVIOR

- Saved queries sort folder/name predictably; search matches name, SQL, and tags.
- Create never overwrites. Update requires an existing ID and UI confirmation when replacing SQL.
- Deleting a folder preserves saved queries by moving them to Unfiled through `ON DELETE SET NULL`.
- History order is terminal timestamp descending with ID as deterministic tie-breaker.
- History pages are bounded; no unbounded result collection.
- Success, failure, and cancellation remain exactly-once from E6 persistence.
- Reopening saved/history SQL creates a new dirty editor tab and never runs it.
- Retention and clear touch only `query_history`, never saved queries or query session drafts.

## SCOPE

- Dialog state: acquire on open -> release on close/project change.
- Debounced search: timer acquire on input -> clear on replacement/unmount.
- Metadata connection: existing mutex guard scoped per repository call.
- Retention transaction: acquire immediate transaction -> commit/rollback structurally.

## TEST LAYERS

- In-memory SQLite with project fixture for explicit create/update, duplicate, search/tag, folder SET NULL, project isolation.
- History fixture with statuses/timestamps/text for filters, stable pagination, clear, age/count retention, and saved-query isolation.
- Typed command tests for all new command argument shapes.
- React Query Library tests with mocked commands for save-as, overwrite confirmation, folder movement, search, history pagination/filter, reopen without execute, retention and clear confirmation.
- Existing E6 exactly-once coordinator tests remain unchanged.

## IMPLEMENTATION ORDER

1. **Preparation:** replace generic API assumptions with explicit domain contract and bounded history page shape.
2. **E8-T1:** repository/commands/types for saved queries and folders; Query Library Saved view; editor bridge.
3. **E8-T2:** filtered bounded history page; History view; reopen SQL only.
4. **E8-T3:** retention policy, transactional prune/clear, local settings controls and isolation tests.
5. Stop in REVIEW with a restart/persistence/manual safety checklist.

## UI STATES

- Disabled trigger: no active project.
- Saved loading: stable rows skeleton.
- Saved empty: functional prompt to save current tab.
- Save conflict: inline duplicate/overwrite guidance.
- History empty: explain that terminal executions appear after Run.
- History loading/error: bounded contextual states.
- Selected saved/history row: SQL preview, metadata, explicit Open in new tab.
- Destructive actions: confirmation with affected scope.

## VERDICT

The existing SQLite schema and E6 terminal history writer are viable foundations, but generic saved-query upsert and unpaged history list do not satisfy E8. Implement explicit create/update/folder mutations and bounded filtered history pages. Keep all reopen paths structurally separate from query execution. Retention must use one transaction and delete from `query_history` only.

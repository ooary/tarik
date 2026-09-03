# EPIC E8 Review - Query library and history

**Status:** APPROVED (2026-09-03)  
**Scope:** Project-scoped saved queries, folders/tags/search, bounded terminal history, reopen-without-execution, retention, and clear isolation.

## Build and launch

```bash
npm run tauri dev
```

Open a local project and enter recognizable SQL in the active editor tab.

## 1. Saved query persistence and explicit save behavior

1. Click **Query library** in the editor toolbar.
2. Click **Save current**.
3. Enter a unique name, optional comma-separated tags, choose Unfiled, then **Save as new**.
4. Close and reopen Query library. Confirm the record remains.
5. Restart Tarik and reopen the same project. Confirm the saved query remains.
6. Try to create another saved query with the same name using different letter casing. Confirm an inline conflict appears and the existing query is not overwritten.
7. Select the saved query, click **Edit**, change only its name/tags/folder, and update. Confirm no SQL replacement warning appears.
8. Edit its SQL and update. Confirm Tarik asks before replacing stored SQL; cancel preserves the prior saved SQL.

## 2. Folders, tags, search, and deletion

1. Create two folders and move a saved query between them using Edit.
2. Rename a folder. Confirm the saved query stays in it.
3. Search by query name, an SQL fragment, and a tag. Confirm each returns the query.
4. Delete a folder containing queries. Confirm the warning says queries are kept in Unfiled.
5. Confirm the queries appear under Unfiled afterward.
6. Delete a saved query. Confirm the editor tab and query history are unaffected.
7. Switch to another project. Confirm saved queries from the first project are not visible.

## 3. Open saved SQL safely

1. Select a saved query and review its complete SQL preview.
2. Click **Open in new tab**.
3. Confirm a new editor tab uses the saved query name and SQL.
4. Confirm it is not executed automatically and Results do not change.
5. Edit the new tab. Confirm the stored saved query does not change until an explicit library Edit/Update.

## 4. Historical terminal executions

1. Run one successful query, one invalid query, and one cancellable long query that you cancel.
2. Open Query library, then **History**.
3. Confirm succeeded, failed, and cancelled entries each appear once, newest first.
4. Select each entry. Confirm SQL snapshot, timestamp, duration, returned rows when available, and structured error summary are correct.
5. Close/reopen the library and restart Tarik. Confirm terminal history persists.
6. Confirm an Explain/Estimate action that does not execute SQL does not create normal query history.

## 5. History filtering and pagination

1. Filter separately by Succeeded, Failed, and Cancelled.
2. Search for a unique SQL fragment and error fragment.
3. Set From and To dates and confirm the range is inclusive for those local dates.
4. Generate more than 25 terminal history entries if practical. Confirm Next and Previous load bounded 25-row pages and preserve newest-first order.
5. Change any filter while on a later page. Confirm paging resets to the first page.
6. Switch projects. Confirm history remains project-scoped.

## 6. Reopen historical SQL safely

1. Select a history entry and click **Open in new tab**.
2. Confirm the new tab contains the immutable historical SQL snapshot.
3. Confirm no query executes automatically, including for historical DDL/DML.
4. Confirm the terminal history entry remains unchanged.

## 7. Retention and clear isolation

1. In History, open **Retention**.
2. Apply only Keep newest entries; verify the retained count.
3. Apply only Maximum age in days; verify old rows are removed.
4. Apply both; verify age is applied and then newest-N retention.
5. Confirm the dialog reports deleted and remaining counts.
6. Cancel retention confirmation. Confirm nothing changes.
7. Click **Clear history**, cancel once, then accept.
8. Confirm only history in the active project is cleared.
9. Confirm saved queries, folders, current editor tabs/drafts, sources, and another project's history remain intact.

## Automated verification recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `npm run lint` (0 errors; four known test-shim warnings)
- `npm run typecheck`
- `npm test`
- `npm run test:ui`
- `npm run build`

## Sign-off

- [x] Saved query persistence/create-update semantics approved
- [x] Folder/tag/search/delete behavior approved
- [x] Saved SQL opens without execution
- [x] Terminal history accuracy/filtering/pagination approved
- [x] Historical SQL opens without execution
- [x] Retention and clear isolation approved

User sign-off received on 2026-09-03. E9 is unblocked and remains not started.

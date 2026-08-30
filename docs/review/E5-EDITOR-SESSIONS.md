# E5 SQL editor and session review

## Available now

- Editable CodeMirror 6 SQL editor
- Line numbers, history, bracket matching, find, selection, indentation
- Ctrl/Cmd+Enter run hook (real execution is E6)
- Completion for DuckDB schemas, tables/views, and columns
- Project-scoped query tabs persisted in SQLite
- Add, activate, rename, duplicate, move, and close tabs
- Dirty indicator and save-error warning
- Serialized 400ms autosave; older drafts cannot overwrite newer edits
- Page-hide/unmount flush
- Right-click table actions: insert quoted name, open bounded preview query, copy qualified name

## Manual review

```bash
npm run tauri:dev:clean
```

1. Open a DuckDB project.
2. Type SQL and confirm CodeMirror is editable with line numbers.
3. Press Ctrl+F and verify find opens.
4. Type a known schema/table/column prefix and inspect completion suggestions.
5. Add multiple tabs, switch among them, and edit different SQL in each.
6. Double-click a tab title or right-click and choose **Rename**.
7. Right-click a tab to **Duplicate**, **Move left/right**, and **Close**.
8. Wait at least 400ms, close Tarik, reopen the same project, and verify tab order, active tab, title, and SQL restore.
9. Right-click an Explorer table and test:
   - **Insert name** adds a safely quoted qualified identifier.
   - **Preview rows** opens a new tab with `SELECT * ... LIMIT 100`.
   - **Copy qualified name** copies the quoted schema/table name.
10. Ctrl/Cmd+Enter is captured by the editor; E6 will connect it to real query execution.

## Persistence safety

Session IDs are deterministic per project. Saves are serialized and coalesced by revision. If SQLite persistence fails, the dirty marker remains and closing a dirty tab asks for confirmation.

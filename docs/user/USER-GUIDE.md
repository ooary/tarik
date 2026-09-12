# Tarik user guide

Tarik is a local desktop SQL workbench. It is designed to help you learn the relationship between SQL, returned data, and DuckDB's query plan without uploading data to a service.

## 1. Install and start

The E11 Linux build produces three x86_64 choices:

- **AppImage**: make it executable and run it from any folder;
- **DEB**: install on a compatible Debian/Ubuntu system;
- **portable tarball**: extract and run `tarik`; keep `tarik-mcp`, `tarik-engine-duckdb`, and `libduckdb.so` beside it.

Linux requires a glibc-compatible x86_64 userspace, GTK 3, and WebKitGTK 4.1. Artifacts are currently unsigned; verify the file against `SHA256SUMS` from the same release directory:

```bash
sha256sum -c SHA256SUMS
```

Tarik opens with no project. Choose **New project** to create a Tarik-managed DuckDB file, or **Open** to register an existing `.duckdb`, `.ddb`, or `.db` file.

## 2. Understand project ownership

### Managed project

**New project** creates a DuckDB file under Tarik's application data directory. Deleting a managed project after confirmation deletes that managed directory and its project-scoped metadata.

### External project

**Open** registers a DuckDB file from a path you chose. **Forget project** removes Tarik's metadata only. It never deletes, renames, or moves the external DuckDB file.

Only one project is active at a time. Tables, completion suggestions, saved queries, history, sessions, query jobs, and exports are scoped to it.

## 3. Add local data

With a project open, choose **Import file** in Explorer and select CSV or Parquet.

### Import CSV or Parquet

Import creates a physical DuckDB **table** and copies data into the project. Use import when:

- the source may move or disappear;
- you want project-local data;
- you accept the additional disk use.

For CSV, review delimiter, header, NULL text, inferred columns, and optional type overrides before **Import table**. CSV is text: it is portable but does not carry reliable database types. Tarik samples its schema and may report an estimated row count for files above the exact-count threshold.

### Link Parquet

Link creates a DuckDB **view** that reads the Parquet file at its original path. Use link when:

- the file should remain the source of truth;
- you want to avoid copying it;
- you can keep the path available.

Parquet is typed, compressed, columnar data and is usually preferable for repeat analytics. If a linked file moves, Tarik marks it **Missing** on project reopen. Select the missing source or choose **Locate replacement** to repair the view. Removing a link preserves the Parquet file.

### Table versus view

- A **table** stores rows in DuckDB.
- A **view** stores a query or file reference; linked Parquet rows remain outside DuckDB.

Explorer shows both types. Right-click an object to insert its safely quoted name, preview up to 100 rows, copy its qualified name, or remove it with an explicit confirmation.

## 4. Write SQL

Create tabs with **New query tab**. Tarik restores project tabs/drafts after restart.

Autocomplete knows the active project catalog:

- type after `FROM` or `JOIN` for tables/views;
- type `main.` for schema-scoped relations;
- type an alias such as `o.` for that relation's columns;
- press **Ctrl+Space** to request completion manually.

Identifiers containing spaces, quotes, or reserved words are quoted safely. Ambiguous alias or column contexts intentionally make no suggestion instead of guessing.

After about 650 ms of idle typing, Tarik uses DuckDB `EXPLAIN` (without `ANALYZE`) to check syntax and binding. Parser, missing-table, and missing-column errors can appear as editor markers before Run. Validation never executes your SQL, but a clean check does not guarantee runtime success. Tarik warns when a top-level UPDATE or DELETE has no top-level WHERE.

## 5. Run and browse results

Choose **Run query** or press **Ctrl+Enter**. Run submits the editor text as an immutable snapshot.

- Multiple statements run sequentially.
- Only the final row-returning statement becomes the browsable result.
- Earlier row sets are drained and not retained.
- DDL/DML may show rows affected instead of a grid.

Queries move through Queued, Running, Succeeded, Failed, or Cancelled. Choose **Cancel** while queued/running; the same project session should remain usable afterward.

Results are bounded. The sidecar writes Arrow IPC pages (normally 500 rows and around 4 MiB maximum); the desktop keeps at most 12 decoded pages. The grid virtualizes rows and columns, so it does not create one DOM element per result cell. Use Previous/Next or PageUp/PageDown to browse. NULL values and truncated large cells are marked; truncation protects IPC/UI memory and does not change source data.

Result pages are ephemeral. Superseding a result, closing a project/application, or choosing **Clear cache** releases them. Tarik never runs an automatic full-result `COUNT(*)` merely to populate the grid.

## 6. Estimate versus Actual Flow

Both actions open the same three-pane analysis workspace: immutable SQL on the left, beginner flow graph in the center, and selected operation details on the right.

### Estimate

**Estimate** runs DuckDB Explain without executing the user statement. Row counts are planning guesses. Use it to ask:

- Which sources will DuckDB read?
- Where does filtering/joining/grouping occur?
- What cardinality does DuckDB expect?

### Actual Flow

**Actual Flow** executes the SQL using DuckDB Profile/Explain Analyze. It can change data for INSERT, UPDATE, DELETE, CREATE, ALTER, and DROP, so Tarik asks for confirmation when the SQL is not clearly read-only. Use it for measured operator rows, rows scanned, and operator time.

Editor changes do not silently alter either analysis. The workspace keeps its Planned/Profiled SQL snapshot, marks **Editor SQL changed**, and requires **Build current SQL** or **Run current SQL** explicitly.

Tarik interprets verified native operators into beginner concepts while preserving raw DuckDB JSON. For example, one grouped aggregate may teach **Group rows by …** and **Count non-null … values per group** as separate concepts while assigning physical time/row metrics only once. Ambiguous plans fall back to native/generic wording rather than guessing.

## 7. Save and reopen SQL

Choose **Query library**.

### Saved queries

In **Saved queries**, choose **Save current**, add a unique name, optional folder, and comma-separated tags, then **Save as new**. Editing an existing item uses **Update saved query** and never silently overwrites by name.

Folders organize saved SQL. Deleting a folder moves its queries to **Unfiled**. Search matches name, SQL, and tags. **Open in new tab** copies SQL into the editor without executing it.

### History

**History** stores one terminal record for successful, failed, and cancelled Run operations. Filter by status, text/error, or date. Retention can keep a maximum count, age, or both. **Clear history** and retention affect query history only; saved queries and editor drafts remain.

Selecting history shows its immutable SQL and terminal outcome. **Open in new tab** never executes it.

## 8. Export exact CSV or Parquet parts

Choose **Export** from the query toolbar. Export captures an immutable SQL snapshot and executes it once; it does not download the current grid pages.

1. Choose an existing output folder.
2. Choose Parquet or CSV.
3. Enter a portable base name and positive **Rows per part**.
4. Choose collision behavior:
   - **Stop without replacing**: atomically refuses an existing part;
   - **Replace completed parts**: publishes complete replacements and removes stale canonical tail parts only after success.
5. For CSV, choose a one-byte delimiter and whether every part includes headers.
6. For Parquet, choose Snappy, Zstandard, Gzip, or Uncompressed.
7. Choose **Start export**. Non-read-only/multi-statement SQL requires confirmation because export executes it.

Parts are named:

```text
<base>-part-00001.csv
<base>-part-00002.csv
```

or `.parquet`. Row boundaries are exact even when an Arrow batch crosses them. Zero rows create no files. CSV headers repeat per part when enabled. Closing the dialog does not cancel active work; use **Cancel export**. Completed published parts remain valid on cancellation/failure; the current incomplete hidden stage is removed or recovered at next startup. **Reveal output** is available only for a tracked completed part.

## 9. Agent access and delegated export destinations

Tarik can expose the active project to a local MCP host through the packaged `tarik-mcp` process. Open **Agent access**, enable the local bridge, approve the visible pairing request, and grant only the project capabilities the client needs. New clients receive no project access. SafeRead work may queue, and one paired profile may retain multiple bounded results. The host can use `tarik_list_active` to recover only its own IDs after reconnecting. Tarik owns analysis limits and expiry; an MCP client cannot raise them, select an extended deadline, approve work, or auto-run handed-off SQL. See [`MCP-AGENT-SETUP.md`](MCP-AGENT-SETUP.md) for host setup and the full safety model.

When a paired client has **Analyze data**, its project permissions include **Export destinations**. A destination is a reusable, create-new-only delegation:

1. Choose **Export destinations** under that client and active project.
2. Use **Choose folder** and select an existing local folder through the operating-system picker.
3. Give it a non-sensitive display label, choose CSV and/or Parquet, and set rows-per-part and total-byte limits.
4. Create, edit, disable, repair, or revoke the destination only from visible Tarik controls.

Tarik stores the canonical path and directory identity privately. The MCP client receives only an opaque destination ID, label, allowed formats, quota summary, enabled/readiness state, and revision. It cannot supply a path, discover the absolute folder, enable overwrite, or mutate destination grants. Roots, files, symlinks/reparse points, non-user-owned folders, network/remote filesystems, Tarik data/cache/log folders, and Tarik-managed project storage are rejected. You may choose a custom folder that also contains an externally opened DuckDB project file; typed CSV/Parquet output names and collision controls still apply. A moved, replaced, or missing folder becomes **Repair required** and cannot be used until you choose a valid replacement directly in Tarik.

Disabling a destination preserves its policy but prevents delegated use. Revoking it deletes the grant. Removing **Analyze data**, removing the project grant, or revoking the client also deletes that client's destination grants for the affected project. These controls do not delete already exported user files.

For raw exploration, agents should select only needed columns and begin with `LIMIT 100` unless aggregation naturally bounds the result. A browse-capped result is labelled incomplete and not exact; refine or aggregate first, then use the non-executing **Open in editor** draft for a user-controlled Run, or use guarded complete-query export. For a complete delegated export, the client classifies one SafeRead query, lists redacted destinations, and calls the guarded export tool with only server-issued IDs and typed CSV/Parquet options. Tarik reruns the complete immutable query independently of the browse cap. Within-policy create-new work is delegated; format/chunk exceptions wait for visible Tarik approval, while replacing existing canonical parts always requires a fresh critical typed confirmation. Status exposes exact aggregate counters and relative filenames only. Cancellation must be polled to a terminal state, and release preserves completed files. A successful zero-row export creates no files.

## 10. Diagnostics, logs, and cache

Open **Settings**.

- Theme: System, Light, or Dark.
- **Reveal logs**: opens the local JSONL diagnostic directory. Logs rotate at 2 MiB, retain seven files, omit SQL/result rows, and degrade to stderr if unavailable.
- **Clear cache**: releases live sidecar results and removes temporary result pages only. Completed exports, saved queries, drafts, and project data are preserved.

A serious frontend/backend failure shows a friendly incident with a copyable ID and local log action. Keep that ID when reporting a problem. E10's full manual recovery review remains deferred for this pre-release build.

## 11. Data, backup, upgrade, and removal

Linux locations:

```text
~/.local/share/com.tarik.desktop/tarik.sqlite
~/.local/share/com.tarik.desktop/projects/
~/.local/share/com.tarik.desktop/logs/
~/.cache/com.tarik.desktop/
```

Close Tarik before backups. Back up `tarik.sqlite` together with any `-wal`/`-shm` companions if present, Tarik-managed project directories, external DuckDB files, and external CSV/Parquet sources separately. Linked Parquet data is not copied into the project.

Migrations are forward-only. Older Tarik builds may reject newer metadata. Read [`../release/COMPATIBILITY.md`](../release/COMPATIBILITY.md) before upgrade/downgrade.

Removing the package does not intentionally remove XDG data. Delete application data manually only after backups. External projects and export directories are independent.

## Known limitations

- Local-only; no remote databases, accounts, credentials, or collaboration.
- Linux x86_64 is the E11 packaged target. Windows portable support is E12; macOS is not yet a declared release target.
- Artifacts are unsigned. SHA-256 proves byte integrity against the supplied manifest, not publisher identity.
- Only one active project; one FIFO query worker and one FIFO export worker per session.
- Result pages are ephemeral; only final row-returning statement is browsable.
- Runtime-only errors remain possible after clean pre-run validation.
- Actual Flow and Export execute SQL and may mutate data after confirmation.
- Linked files can break when moved; relink them explicitly.
- Full E6 result, E7 query-flow, and E10 recovery manual reviews remain reopenable before final release acceptance.

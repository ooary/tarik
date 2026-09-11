# Tarik upgrade, compatibility, and backup contract

## Versioned components

Tarik 0.1.0 uses:

- metadata SQLite schema version 11;
- private desktop ↔ `tarik-mcp` bridge protocol version 3;
- engine protocol version 2;
- DuckDB runtime 1.5.5;
- application identifier `com.tarik.desktop`.

`package.json`, the workspace/Tauri Cargo package, and `src-tauri/tauri.conf.json` must have the same application version before packaging.

## SQLite metadata upgrades

Metadata migrations are forward-only, ordered, and transactional. Tarik refuses to open a metadata database whose `user_version` is newer than the application supports. An older Tarik build may therefore not open metadata after a newer build migrates it.

Before upgrading or downgrading, close Tarik cleanly and back up:

```text
~/.local/share/com.tarik.desktop/tarik.sqlite
```

For a complete snapshot, also preserve adjacent `tarik.sqlite-wal`/`tarik.sqlite-shm` files if they exist. A clean shutdown checkpoints/truncates the WAL, but copying only the main file while Tarik is running is unsafe.

Metadata contains project registrations, editor sessions/drafts, saved queries, query/export history, project-scoped quality-check definitions with immutable revisions and bounded aggregate run summaries, source records, Agent Access client/grant records, private client/project-bound export destination paths and directory identities, bounded SQL-free agent audit facts, and preferences. Schema 11 adds these private delegated-destination records. They are removed when their client/project Analyze authority is removed or the client is revoked. Profile samples, common values, and failing-row previews are ephemeral and are not stored in SQLite. Result caches and logs are not required to restore projects.

## DuckDB projects

Tarik-managed DuckDB files live below:

```text
~/.local/share/com.tarik.desktop/projects/
```

Externally opened DuckDB files remain wherever the user chose. Tarik never relocates them automatically. Close Tarik before copying a DuckDB file so no operation is active. Back up external CSV/Parquet files separately; linked views store file references rather than embedding linked Parquet data.

DuckDB storage compatibility is governed by DuckDB. Tarik pins DuckDB 1.5.5 for this release. Before opening important projects with another DuckDB version, consult DuckDB's storage compatibility documentation and keep a copy of the original database.

## Export and cache recovery

Completed CSV/Parquet export parts are user files and are not deleted by Tarik cache cleanup or MCP export release. Export replacement uses exact hidden stages/backups and cache-owned recovery manifests. After a crash, startup may remove an incomplete hidden stage or restore an interrupted exact replacement; canonical completed parts are preserved. Guarded MCP exports reuse this same streaming/recovery path, while returning only aggregate counters and relative filenames.

`~/.cache/com.tarik.desktop` contains ephemeral result pages and recovery manifests. It can be cleared through Settings; do not treat it as a backup.

## Downgrade and uninstall

There is no automatic metadata downgrade. Restore the pre-upgrade `tarik.sqlite` backup when returning to an older Tarik build.

Uninstalling an application package does not intentionally remove XDG data/cache directories. Remove them manually only after backing up managed projects and saved SQL. External projects and export directories are independent of the package.

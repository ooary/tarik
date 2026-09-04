# E2 SQLite metadata review

## Database location

Tarik opens one metadata database at the platform application data directory:

```text
<Tarik app data>/tarik.sqlite
```

On Linux this normally resolves below the user data directory. Application paths are resolved and owned by the Rust backend; the unused WebView path-disclosure command was removed in E12-T0.

## Schema version

Current SQLite `PRAGMA user_version`:

```text
4
```

Migrations run in order and inside immediate transactions:

1. `0001_metadata_foundation.sql`
2. `0002_query_sessions.sql`
3. `0003_saved_queries_history.sql`
4. `0004_sources_exports.sql`

A database newer than the supported version is rejected instead of being modified.

## Operational data stored

- Typed application settings
- Recent local projects
- Query sessions, ordered tabs, active tab, and SQL drafts
- Saved queries, folders, and tags
- Successful, failed, and cancelled query history
- Linked/imported source definitions and source health
- Export attempt metadata and completed part summaries

## Data not stored

- Analytical rows
- Full query results
- CSV or Parquet file contents
- Credentials
- Diagnostic log contents

## UI preference behavior

The E1 workbench now loads and stores:

- Theme preference
- Source explorer width and collapsed state
- Result panel height and collapsed state
- Last selected Results, Flow, or Profile panel

Writes are debounced by 250ms and normalized before crossing the SQLite boundary. Invalid stored values fall back to documented defaults.

## Review commands

Run all checks:

```bash
npm run format:check
npm run lint
npm run typecheck
npm test
npm run test:ui
npm run build
cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build -- --debug --no-bundle
```

Manual persistence check:

1. Start Tarik with `npm run tauri dev`.
2. Resize and collapse workbench panels.
3. Switch between Results, Flow, and Profile.
4. Close Tarik cleanly.
5. Reopen Tarik.
6. Confirm panel state, dimensions, and active output panel restore.
7. Confirm `tarik.sqlite`, `tarik.sqlite-wal`, and `tarik.sqlite-shm` stay in the app data directory and are ignored by Git.

## Migration and repository coverage

Automated Rust tests cover:

- Fresh migration
- Idempotent startup
- Future-version rejection
- Transaction rollback
- Foreign-key configuration
- Typed setting round trip and invalid JSON
- Stable project ID for a repeated path
- Recent-project ordering and removal
- Session tab order and one-active-tab invariant
- Atomic session replacement rollback
- Saved-query CRUD, search, tags, and cascade
- Query-history terminal states and retention pruning
- Source state transitions
- Export completed-part metadata

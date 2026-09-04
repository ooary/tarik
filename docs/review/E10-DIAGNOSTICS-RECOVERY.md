# E10 diagnostics, recovery, cleanup, and shutdown review

**Status:** DEFERRED / REOPENABLE (2026-09-04)  
**Scope:** Bounded structured logging, support incidents, safe cache/abandoned-export cleanup, and graceful shutdown.

## Build and launch

```bash
cd /home/ooary/Projects/Tarik
./scripts/build-engine.sh
./scripts/check-engine.sh
npm run tauri dev
```

## 1. Structured rolling file logging

1. Start Tarik, create or open a project, run a query, and start one small export.
2. From Settings, choose **Reveal logs**. The file manager opens the Tauri-resolved log directory containing `tarik.log`.
3. Open `tarik.log` and confirm every line is JSON with a closed schema:
   - timestamp, level, target, event, and optional operation/project/incident IDs, duration, status, error code, and one short message;
   - no SQL text, parameters, CSV previews, result rows, export payload content, or arbitrary data maps.
4. Run a deliberately failing query and confirm the log shows a `query submit` span with `started` and `failed` entries, a stable error code, duration, and project ID, but the message does not contain the SQL.
5. Confirm the settings dialog states the retention policy (**7 files, up to 2 MiB each**).
6. Force growth by running many operations (or temporarily lower the limit) and confirm:
   - the active file stops growing past the size limit;
   - `tarik.log.1` through `tarik.log.6` appear and older archives are removed;
   - no more than seven `tarik.log*` files exist.
7. Make the log directory unwritable (or simulate failure) and confirm Tarik continues to work, logs fall back to stderr, and no user operation fails because of logging.

## 2. Support incidents and panic surfaces

### Frontend render failure

1. Temporarily introduce a render error in a child component and rebuild.
2. Confirm the app shows a friendly recovery surface titled **Tarik could not load the workbench** with:
   - a generated incident ID with a **Copy ID** action;
   - **Reveal logs** pointing at the real log directory;
   - a **Retry** action that reloads the workbench;
   - no raw render details as the heading or the only guidance.
3. Confirm the log contains an `incident frontend.render` error with the same incident ID.
4. Confirm the incident marker file (`last-incident.json`) exists next to the log.

### Backend panic

1. Trigger a backend panic during normal operation.
2. Confirm the workbench shows a non-blocking incident dock with a generated ID and summary; raw panic text is not the only message.
3. Confirm a `support-incident` event reached the UI without user interaction and stderr still printed the incident line.
4. Close and reopen Tarik. Confirm the same incident reappears once for awareness and does not return on subsequent launches.
5. Copy the incident ID and confirm the clipboard contains exactly the displayed ID.

## 3. Startup cleanup and explicit cache clearing

1. Run a query and leave its result open. Close Tarik, then reopen.
2. Confirm startup logs a `storage startup_cleanup` event and fresh result artifacts still exist until the age/size policy applies:
   - result artifacts older than 24 hours are removed;
   - when the cache exceeds 512 MiB, the oldest artifacts are removed first;
   - bounded warnings appear for unreadable or skipped entries.
3. Place a sentinel file outside the cache (for example `/tmp/keep.txt`) and a symbolic link inside `<cache>/results` pointing to it. Restart and confirm:
   - the sentinel file is untouched;
   - the link was skipped with a warning;
   - nothing outside `<cache>/results` or `<cache>/export-staging` was modified.
4. Run a query, browse a few pages, then open Settings and choose **Clear cache**.
5. Confirm the status line reports the removed artifact count, the grid refetches from the sidecar cleanly (or shows a bounded error), and the notice says completed exports are preserved.
6. Repeat Clear cache with the sidecar intentionally stopped and confirm a bounded, non-destructive error instead of deleting directories anyway.

## 4. Abandoned export recovery

1. Start an export to a chosen output directory and kill the Tarik process mid-export (after at least one part completes).
2. Restart Tarik and confirm:
   - the incomplete hidden stage (`.tarik-export-<exportId>-part-NNNNN-…tmp`) is removed;
   - completed canonical parts (`<base>-part-NNNNN.<ext>`) are untouched;
   - a hidden backup (`.tarik-export-backup-…`) is restored to its canonical name only when the canonical part is missing, and removed when it is present;
   - the recovery manifest under `<cache>/export-staging` is removed after reconciliation.
3. Repeat with a Replace-policy export interrupted between backup rename and publication: the previous complete part must come back.
4. Corrupt one manifest (invalid JSON or a foreign directory path) and restart. Confirm the malformed manifest is removed, a bounded warning is logged, and its claimed output directory is never touched.
5. Complete a normal export and confirm no manifest remains and no hidden files persist in the output directory.

## 5. Graceful shutdown

### Normal close with a dirty draft

1. Edit a query tab, change the theme, then close the window within the debounce window.
2. Confirm the window closes, the latest SQL survives in the reopened project (exact text), and the theme is preserved.
3. Confirm logs show `app graceful_shutdown` with a successful status, cancelled query/export counts, a released-result count, and `metadataCheckpointed` behavior (SQLite `-wal` file shrinks/empties after close).

### Close with running work

1. Start a long-running query and a multi-part export, then immediately close the window.
2. Confirm shutdown cancels both jobs, waits at most two seconds, persists exactly one terminal history entry each (failed/cancelled), releases results, and closes DuckDB and the sidecar.
3. Reopen the project and confirm the DuckDB file opens cleanly and completed export parts remain valid; the in-progress hidden part is gone.
4. Close Tarik during an export with multiple completed parts and confirm the startup recovery in section 4 reconciles the remaining work.

### Draft flush failure

1. Simulate draft persistence failure (for example, by making the metadata database read-only at the filesystem level or via a test hook) and close the window.
2. Confirm the window stays open with a clear **Tarik could not save the latest draft** surface offering **Retry save** and **Quit without latest changes**.
3. Confirm Retry attempts the flush again and Quit proceeds without silently claiming the draft was saved.

### Frontend never registered

1. Kill the WebView early (or block the frontend bundle) and close the window.
2. Confirm Tarik does not hang: native close is not intercepted, the engine is stopped via the destroyed fallback, and logs are flushed.

## 6. Regression checks

1. Normal query run, cancel, paging, and result release still work after shutdown/restart.
2. Estimate and Actual Flow remain non-executing/explicit; export confirmation and immutable SQL snapshots are unchanged.
3. Saved queries and history reopen without execution; retention/clear still touch only `query_history`.
4. Settings log reveal and cache clear never accept a filesystem path from the frontend; commands ignore injected paths.
5. Existing editor diagnostics, completion, and export dialogs behave as approved in E9.5/E9.

## Automated verification recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: 73 desktop tests plus protocol/sidecar suites (13 green test binaries)
- Logger redaction/rotation/span and incident marker tests
- Cleanup age/size, symlink, malformed-manifest, and exact stage/backup/canonical tests
- Export coordinator manifest registration/terminal cleanup tests
- Sidecar `result.release_all` and export hidden-name tests
- Shutdown state machine tests plus frontend flush/recovery/close-listener tests
- Real sidecar rebuild and `check-engine.sh` handshake (`Engine OK: duckdb 0.1.0 protocol 1`)
- `npm run typecheck`
- `npm run test:ui`: 136 tests across 22 files
- `npm test`: 12 typed command tests
- `npm run lint`: 0 errors; four pre-existing warnings
- `npm run build`

## Sign-off

- [ ] Log schema is closed and free of SQL/result data; rotation bounds disk use
- [ ] Settings reveals the real log directory and states retention
- [ ] Frontend render failures show friendly recovery with copyable incident ID
- [ ] Backend panics surface live and once after restart without raw text as sole guidance
- [ ] Startup cleanup honors 24-hour age and 512 MiB budget inside owned roots only
- [ ] Clear cache releases live sidecar results first and preserves completed exports
- [ ] Abandoned exports reconcile exact stages/backups only; canonical parts survive
- [ ] Malformed manifests never grant access to claimed output directories
- [ ] Normal close flushes the latest draft and preferences before backend teardown
- [ ] Running queries/exports cancel within two seconds and persist exactly-once terminal history
- [ ] DuckDB/SQLite reopen cleanly after forced test shutdown
- [ ] Draft flush failure offers explicit retry/quit choices; never silently claims saved
- [ ] Frontend-unregistered close still stops the engine and flushes logs

Manual review was postponed on 2026-09-04. The user authorized provisional progression to E11 without checking these items. E10 remains reopenable and requires final sign-off before release acceptance.

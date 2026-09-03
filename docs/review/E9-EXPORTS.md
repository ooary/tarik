# E9 Streaming chunked exports review

**Status:** APPROVED (2026-09-04)  
**Scope:** Exact-row CSV and Parquet parts, one-pass SQL execution, overwrite safety, bounded progress, cancellation, partial failure, terminal history, and output reveal.

## Build and launch

```bash
cd /home/ooary/Projects/Tarik
./scripts/build-engine.sh
./scripts/check-engine.sh
npm run tauri dev
```

Open a local project with a table containing enough rows to cross a small part boundary. For a quick fixture, run:

```sql
CREATE OR REPLACE TABLE export_review AS
SELECT i AS id, 'row-' || i AS label
FROM range(1, 12) t(i);
```

Then use this read-only SQL in the active editor:

```sql
SELECT id, label FROM export_review ORDER BY id;
```

Choose a new empty output folder for the first pass.

## 1. Options and preflight

1. Confirm the editor toolbar order ends with **Estimate**, **Actual Flow**, **Export**.
2. Clear the editor. Confirm **Export** is disabled.
3. Restore the query and click **Export**. Confirm one compact dialog opens with the current SQL preview.
4. Confirm Parquet is the default and offers Snappy, Zstandard, Gzip, and Uncompressed.
5. Switch to CSV. Confirm delimiter and “Include column names in every part” appear.
6. Leave the output folder empty and click **Start export**. Confirm an inline field error appears and no query or file starts.
7. Enter an unsafe base name such as `../orders`. Confirm it is rejected before execution.
8. Enter zero, a decimal, or a number above JavaScript's safe integer range for rows per part. Confirm it is rejected.
9. Enter a multi-byte or line-break CSV delimiter. Confirm it is rejected.
10. Confirm the folder picker selects a directory and the base name uses only letters, digits, hyphens, or underscores.

## 2. Exact CSV parts

1. Select CSV, choose a delimiter, enable headers, set rows per part to `4`, and use base name `orders_csv`.
2. Keep **Stop without replacing** selected and start the export.
3. Confirm progress shows rows, files, bytes, elapsed time, and current part without freezing the editor.
4. Close the dialog while work is active, then reopen Export. Confirm the same active status remains and the export was not cancelled.
5. Confirm the terminal summary reports 11 rows and 3 files.
6. Open the files and confirm names are deterministic:
   - `orders_csv-part-00001.csv`
   - `orders_csv-part-00002.csv`
   - `orders_csv-part-00003.csv`
7. Confirm row counts are exactly `4`, `4`, and `3`, in original order.
8. Confirm every CSV part contains the header once.
9. Repeat with headers disabled and a custom one-byte delimiter. Confirm no part contains a header and parsing remains valid.

## 3. Exact Parquet parts

1. Select Parquet, Snappy compression, rows per part `4`, and base name `orders_parquet`.
2. Start the export and confirm the same non-blocking progress/summary flow.
3. Confirm three files contain exactly `4`, `4`, and `3` rows.
4. Query or inspect every Parquet file and confirm schema and row order are preserved.
5. Repeat with Zstandard or Gzip and confirm the files remain readable.
6. Click **Reveal output** and confirm the system file explorer reveals a completed part.

## 4. Collision and replacement safety

1. Run the same base name again with **Stop without replacing**.
2. Confirm the export fails with a collision error and all existing files remain unchanged.
3. Change the SQL to return 7 rows, select **Replace completed parts**, and export to the same base name with rows per part `4`.
4. Confirm parts 1 and 2 are replaced only with complete readable files.
5. Confirm the stale old part 3 is removed after successful replacement.
6. Create a similarly named file such as `orders_parquet-part-not-a-sequence.parquet`; repeat Replace and confirm that file is preserved.
7. Export a zero-row query with Replace. Confirm old canonical parts for that base/format are removed and the summary says no files were created.

## 5. Cancellation and partial results

1. Start a large export with a small enough part size to publish at least one file while work continues.
2. Click **Cancel export**.
3. Confirm the dialog reaches Cancelled and remains responsive.
4. Confirm already completed parts remain readable.
5. Confirm no hidden `.tarik-export-*.tmp` current-stage file remains.
6. Confirm the summary says completed files remain valid and the incomplete current part was removed.
7. Start a small export immediately afterward. Confirm the project and engine remain usable.
8. Start two exports quickly. Confirm the second waits behind the first for the same project/session; cancel the queued export and confirm it creates no files.

## 6. Failure behavior

1. Choose a directory that becomes unwritable after selection, or otherwise cause a safe write failure.
2. Confirm the export ends Failed with a structured error rather than crashing the app.
3. If earlier parts completed, confirm they remain readable and are listed in the summary.
4. Confirm the incomplete current stage is absent.
5. Restore permissions and start another export. Confirm the session remains usable.

## 7. SQL execution truthfulness

1. Start an export from a clear read-only `SELECT`. Confirm no mutation warning appears.
2. Try export with `WITH`, multi-statement SQL, or a potentially mutating statement. Confirm Tarik warns that Export executes SQL once before writing files.
3. Cancel that confirmation. Confirm no SQL or file operation starts.
4. For a safe disposable project, accept a multi-statement export that inserts one marker row then selects it.
5. Confirm the marker is inserted exactly once and only the final row-returning statement is exported.
6. Edit the editor after starting an export. Confirm the progress view keeps the immutable submitted SQL snapshot.

## 8. Zero rows and bounded summaries

1. Export `SELECT 1 WHERE false` to a new base name.
2. Confirm success reports zero rows, zero files, and “Query returned no rows; no files were created.”
3. For an export producing over 100 parts, confirm aggregate file/row/byte counters remain exact while the list discloses that only the latest 100 completed files are shown.

## 9. Persistence and isolation

1. Complete one success, one failure, and one cancellation.
2. Restart Tarik and reopen the project. Confirm metadata migration succeeds and the project remains usable.
3. Confirm export lifecycle records persist exactly one terminal entry internally for each export, including SQL/options, counters, completed parts, duration, and structured error where applicable.
4. Confirm query history, saved queries, folders, SQL drafts, sources, and another project's data are unchanged.
5. Close a project during active export work. Confirm session close requests cancellation and the app can reopen the project.

## Automated verification recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- Engine protocol integration for CSV/Parquet lifecycle, active/queued cancellation, validation-before-SQL, stage cleanup, and session reuse
- CSV and Parquet readback for zero, exact boundary, boundary remainder, large batch, many batches, compression/options, collision, replacement, and stale-tail cleanup
- Deterministic injected disk-full and cancellation tests after a completed part
- Desktop coordinator tests for project validation, bounded progress, immediate queued cancellation, and exactly-once immutable terminal history
- `npm run lint` (0 errors; four existing ResultGrid/query timer warnings)
- `npm run typecheck`
- `npm test` (11 typed command tests)
- `npm run test:ui` (107 tests)
- `npm run build`

## Sign-off

- [x] CSV exact row boundaries, naming, delimiter, and per-part headers approved
- [x] Parquet exact row boundaries, schema/readback, and compression approved
- [x] Collision and Replace behavior, including stale-tail cleanup, approved
- [x] Queued/running progress and close-without-cancel behavior approved
- [x] Active and queued cancellation cleanup approved
- [x] Partial failure keeps completed parts and removes incomplete stage
- [x] Mutation warning and immutable submitted SQL approved
- [x] Zero-row and bounded-summary behavior approved
- [x] Restart/persistence and project isolation approved
- [x] Reveal output location approved

User sign-off received on 2026-09-04. E9.5 is unblocked and E10 remains queued behind it.

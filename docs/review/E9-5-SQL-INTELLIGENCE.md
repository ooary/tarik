# E9.5 Beginner SQL intelligence review

**Status:** APPROVED (2026-09-04)  
**Scope:** Specific grouped/aggregate/DISTINCT query flow, project catalog completion, alias-safe columns, identifier quoting, and non-executing pre-run DuckDB diagnostics.

## Build and launch

```bash
cd /home/ooary/Projects/Tarik
./scripts/build-engine.sh
./scripts/check-engine.sh
npm run tauri dev
```

Open a local project that contains `main.data_2021` with at least these columns:

- `commodity`
- `market`

Use this accepted query:

```sql
SELECT DISTINCT commodity, count(market)
FROM "main"."data_2021"
GROUP BY commodity;
```

## 1. Accepted beginner query flow

1. Click **Estimate** for the accepted query.
2. Confirm the graph contains exactly these beginner steps in source-to-result order:
   - **Read data_2021**
   - **Group rows by commodity**
   - **Count non-null market values per group**
   - **Remove duplicate result rows**
   - **Query result**
3. Select **Read data_2021**. Confirm its inspector says DuckDB reads the needed columns from that source.
4. Select **Group rows by commodity**. Confirm the inspector says this is a teaching step, not extra physical work.
5. Confirm Group has no duplicated estimate, actual rows, rows scanned, or operator time.
6. Select **Count non-null market values per group**. Confirm the inspector explains that `COUNT(market)` excludes NULL values.
7. Confirm Count retains the native `HASH_GROUP_BY` detail and owns the physical estimate/actual/time metrics once.
8. Select **Remove duplicate result rows**. Confirm its inspector explains DISTINCT and says it is redundant for this exact query.
9. Confirm selecting Group highlights the verified grouping expression, Count highlights `count(market)`, and DISTINCT highlights `DISTINCT` in the immutable Planned SQL.
10. Expand **Show raw plan** and confirm the original DuckDB JSON remains available.
11. Repeat with **Actual Flow** and confirm the same semantic order, immutable Profiled SQL, and measured metrics only on physical nodes.

## 2. Aggregate variants and physical truth

Run Estimate for each query and confirm the specific calculation wording:

```sql
SELECT count(*) FROM "main"."data_2021";
SELECT count(market) FROM "main"."data_2021";
SELECT count(DISTINCT market) FROM "main"."data_2021";
SELECT count_if(market IS NOT NULL) FROM "main"."data_2021";
SELECT sum(value_column) FROM some_numeric_table;
SELECT avg(value_column), min(value_column), max(value_column) FROM some_numeric_table;
```

1. Confirm COUNT(*) says **Count rows for the whole input**.
2. Confirm COUNT(column) says **Count non-null … values**.
3. Confirm COUNT(DISTINCT column) says **Count unique … values**.
4. Confirm COUNT_IF says **Count matching rows**.
5. Confirm SUM/AVG/MIN/MAX use Sum/Calculate average/Find minimum/Find maximum wording.
6. For multiple aggregates in one SELECT, confirm one **Calculate summaries** node lists the calculations.
7. Confirm Tarik does not show Count → Sum → Average as sequential physical passes.
8. Confirm aggregate without GROUP BY has no artificial Group node and describes the whole input.
9. Check `SELECT DISTINCT commodity FROM main.data_2021`. Confirm **Remove duplicate result rows** appears without claiming DISTINCT is redundant.
10. Check a query grouped by more keys than it returns. Confirm Tarik does not claim DISTINCT is redundant.
11. Check a complex nested or unsupported aggregate query. Confirm Tarik falls back to native/generic wording rather than guessing.

## 3. Table and view completion

1. Start a new query and type `SELECT * FROM data_`.
2. Press `Ctrl+Space` if completion is not already visible.
3. Confirm `data_2021` appears with a textual **table** or **view** label and its schema.
4. Type a JOIN and a partial project relation. Confirm project relations are prioritized after JOIN.
5. Type `SELECT * FROM main.`. Confirm only relations from `main` appear.
6. If two schemas contain the same relation name, confirm the unqualified suggestion inserts a schema-qualified name.
7. Confirm general SQL keyword/function completion remains available and keywords are uppercase.
8. Import/link/create/drop a relation, refresh the catalog as normal, and confirm completion updates without losing editor text, selection, or undo history.
9. Switch projects and confirm relations from the prior project are not suggested.

## 4. Alias and column completion

1. Type:

   ```sql
   SELECT d.
   FROM "main"."data_2021" AS d;
   ```

2. Place the cursor after `d.` and confirm only columns from `data_2021` appear, including `commodity` and `market`.
3. Confirm this works even though FROM appears later than the SELECT cursor.
4. Join another relation with a different alias and confirm each alias shows only its own columns.
5. Create an intentionally duplicated alias. Confirm Tarik makes no alias-column claim instead of showing unrelated columns.
6. Press Ctrl+Space for unqualified columns with two sources. Confirm only columns that are unambiguous across those sources appear.

## 5. Safe identifier insertion

Use or create relations/columns with non-simple names where practical.

1. Confirm `order value` inserts as `"order value"`.
2. Confirm a reserved identifier such as `select` inserts as `"select"`.
3. Confirm an embedded quote is doubled inside a quoted identifier.
4. Confirm ordinary names such as `data_2021` remain unquoted unless schema qualification is required for ambiguity.

## 6. Pre-run syntax and catalog diagnostics

1. Type a valid query and stop typing.
2. Confirm the toolbar briefly says **Checking SQL**, then **No problems detected before execution**.
3. Hover that state and confirm it does not promise execution success; runtime-only errors may still occur.
4. Type an invalid table:

   ```sql
   SELECT * FROM missing_table;
   ```

5. After the idle delay, confirm a compact problem summary, gutter marker, red wavy underline on the reliable source range, and DuckDB hover message.
6. Type an invalid column against a real relation. Confirm a binder diagnostic appears before Run.
7. Type malformed syntax. Confirm a parser diagnostic appears before Run.
8. Type an incomplete end-of-input expression such as `SELECT (`. Confirm the error can appear in the summary without an arbitrary underline when DuckDB provides no reliable caret.
9. Confirm validation itself does not open Results, create query history, run Actual Flow, or change project data.
10. Press F8 and Ctrl+Shift+M (Cmd+Shift+M on macOS) to verify official CodeMirror diagnostic keyboard navigation/panel behavior.

## 7. Debounce, stale state, and catalog refresh

1. Type continuously. Confirm errors do not flash on every keystroke.
2. Stop typing for about 650 ms. Confirm only the settled SQL is checked.
3. After an error appears, type one character. Confirm the old marker and summary clear immediately.
4. Quickly replace invalid SQL with valid SQL. Confirm a late old response never restores the stale error.
5. Switch tabs while a check is pending. Confirm diagnostics stay with the correct active tab and SQL revision.
6. Change/open another project while a check is pending. Confirm the stale response is ignored.
7. Create/import the table named in a prior missing-table diagnostic, refresh catalog, and confirm unchanged SQL is revalidated.
8. Simulate or observe engine unavailability. Confirm **SQL check unavailable** is non-blocking and Run remains available.

## 8. High-confidence mutation warning and no-execution proof

1. Type `UPDATE some_table SET value = 1` against a disposable real table.
2. Confirm a yellow warning states that UPDATE has no top-level WHERE and may affect every row.
3. Add a top-level WHERE. Confirm the warning clears after validation.
4. Repeat with DELETE without and with WHERE.
5. Confirm the word WHERE inside a string, comment, or nested expression does not produce a false top-level-condition result.
6. Most importantly, inspect the table before and after waiting for diagnostics. Confirm validation changed no rows.
7. Type valid CREATE and INSERT statements without pressing Run. Confirm no relation or row is created.
8. Confirm only explicit Run, Actual Flow, or Export execution can change data.

## 9. Regression checks

1. Confirm Ctrl+Enter still runs the active tab exactly once.
2. Confirm Estimate remains non-executing and Actual Flow remains explicit execution.
3. Edit SQL after Estimate/Actual Flow. Confirm the analysis workspace keeps its immutable SQL snapshot and marks editor changes.
4. Confirm Export keeps its immutable submitted SQL and still requires mutation confirmation.
5. Confirm saved/history SQL reopen without execution.
6. Confirm query tabs, draft persistence, result paging, import/link/drop, and project switching remain functional.

## Automated verification recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`: desktop 64 tests plus protocol/sidecar/fixture suites
- Real DuckDB Explain/Profile fixtures for accepted grouped COUNT DISTINCT flow
- Sidecar mutation sentinels for CREATE, INSERT, UPDATE, DELETE validation
- Parser/binder/catalog diagnostics, multiline/statement offsets, reliable/no-range cases
- Aggregate kind/count matching, multi-summary truthfulness, metric ownership, DISTINCT proof/fallback
- CompletionContext and mounted CodeMirror catalog/alias/quoting/refresh tests
- Debounce, stale response, wrong revision, catalog revision, unmount, lint marker/summary tests
- `npm run typecheck`
- `npm run test:ui`: 129 tests across 22 files
- `npm test`: 12 typed command tests
- `npm run lint`: 0 errors; four existing ResultGrid/query timer warnings
- `npm run build`

## Sign-off

- [x] Accepted query renders the exact five-step beginner flow
- [x] Group and Count are separate teaching concepts without duplicated physical metrics
- [x] Aggregate variants and multiple summaries are specific and truthful
- [x] DISTINCT labeling and conservative redundancy note approved
- [x] FROM/JOIN/schema table and view completion approved
- [x] Alias/unqualified column completion and ambiguity fallback approved
- [x] Safe identifier quoting and live catalog refresh approved
- [x] Syntax/table/column diagnostics appear before execution
- [x] Reliable ranges, message-only fallback, and keyboard accessibility approved
- [x] Debounce/immediate-clear/stale-response/catalog-revalidation behavior approved
- [x] UPDATE/DELETE no-WHERE warning approved
- [x] Validation proven non-executing and existing workflows remain intact

User sign-off received on 2026-09-04. E10 is unblocked and remains not started.

# EPIC E7 Review — Beginner-friendly query flow

**Status:** REVIEW  
**Scope:** Explain/Profile capture, normalized DuckDB plan graph, XYFlow visualization, beginner inspector, and conservative SQL-range mapping.

## Build and launch

```bash
scripts/build-engine.sh
npm run tauri dev
```

Open a project with at least two imported or linked relations. The checks below can use your own names; substitute them in the sample SQL.

## 1. Explain does not execute SQL

1. Put a read query in the editor, for example:

   ```sql
   SELECT * FROM orders WHERE amount > 20 ORDER BY amount DESC LIMIT 10;
   ```

2. Click **Explain**.
3. Confirm the lower panel switches to **Flow** and labels it **Estimated execution plan**.
4. Confirm the header says row counts are DuckDB planning guesses, not query results. Estimated nodes should read `DuckDB estimate · ~N output rows`, not imply actual rows.
5. Use a harmless DDL statement such as `CREATE TABLE e7_explain_guard AS SELECT 1 AS id;` and click **Explain** only.
6. Refresh the catalog and confirm `e7_explain_guard` was **not** created.

## 2. Directed graph and controls

1. Explain a two-input join:

   ```sql
   SELECT c.customer_name, count(*) AS orders
   FROM orders o
   JOIN customers c ON o.customer_id = c.id
   GROUP BY c.customer_name
   ORDER BY orders DESC
   LIMIT 10;
   ```

2. Confirm both read/source nodes are left of and visibly converge into one Join node.
3. Confirm persistent connector lines and arrowheads remain visible between every connected node.
4. Confirm a restrained pulse travels once from source to result and each reached node gets a brief border cue. The animation must not loop.
5. Confirm later group/sort/limit/result steps continue toward the right.
6. Pan, zoom, fit, and select nodes. Confirm controls remain compact and the graph remains readable.
7. Resize the bottom panel and app window. Confirm no node/inspector content overlaps outside the panel.
8. Enable reduced motion at OS/browser level and reopen Flow. Confirm connectors remain visible but traversal motion is effectively disabled.

## 3. Automatic flow on Run

1. Return to Results, then click **Run query** on a harmless read query.
2. Confirm Tarik immediately opens Flow and briefly shows **Preparing flow** while capturing a non-executing estimated plan.
3. Confirm the graph appears and animates, then the real query starts exactly once.
4. Confirm the Result tab still contains the normal running/succeeded result and can be selected at any time.
5. Run a harmless DDL statement and confirm it takes effect exactly once, not twice. Automatic Flow must use Explain, never Profile.
6. Use invalid SQL. Confirm the Flow error is visible and the actual execution still reports its normal structured SQL error.

## 4. Beginner inspector

1. Select Read data, Join, Group and summarize, Sort, and Limit nodes.
2. Confirm the inspector shows:
   - a plain-language description;
   - clear Input and Output meanings;
   - native DuckDB operator name;
   - a prominent `Planning estimate, not result count` explanation in Flow;
   - source and `DuckDB estimated output` when reported;
   - join type/conditions, filters, groups/aggregates, projections, sort keys, or limits when DuckDB reports them.
3. For a Filter followed by Choose columns, confirm both may repeat the same estimate and the projection inspector explains that choosing columns normally changes columns, not which rows match.
4. Click empty graph space and confirm the inspector returns to **Select an operation**.
5. Confirm no operator explanation makes claims beyond the native details shown.

## 5. Profile executes and reports actuals

1. Use a read-only query first and open **Profile**.
2. Click **Run Profile**.
3. Confirm the panel says **Actual execution profile** and explains that row counts show what happened during execution.
4. On operators where DuckDB reports both values, confirm nodes show `Est. ~N · Actual N` plus a textual estimate-accuracy badge:
   - difference below `10×` → green;
   - `10×` through `100×` → yellow;
   - above `100×` → red.
5. Confirm both over-estimates and under-estimates are labeled, and zero mismatches use `∞×` without crashing.
6. Select a compared node and confirm the inspector explains the factor measures estimate accuracy, not query speed. Use operator time and rows scanned to assess performance.
7. Confirm Explain and Profile retain independent last plans when switching tabs.
8. Profile a failed SQL statement and confirm a structured error with **Try again** appears.
9. Profile a DDL/DML statement only after acknowledging that Profile executes SQL. Confirm the statement's effect occurs exactly once.

## 6. SQL-range bridge (best effort)

1. Explain the join query from section 2.
2. Select:
   - Read data → the matching table token should highlight;
   - Join → the unique `JOIN` keyword should highlight;
   - Group and summarize → `GROUP BY` should highlight;
   - Sort → `ORDER BY` should highlight;
   - Limit → `LIMIT` should highlight.
3. Select Choose columns/Projection and confirm an unsupported mapping does not highlight unrelated SQL.
4. Try a nested query with two `WHERE` clauses. Select a Filter node and confirm Tarik does **not** knowingly choose one ambiguous range.
5. Edit SQL after producing the plan, then select a node. Confirm no stale/wrong range is left highlighted.
6. Switch query tabs and confirm a highlight does not leak into another tab.

## 7. Fallback and error resilience

1. Confirm empty Flow/Profile tabs show explicit Run Explain/Run Profile actions.
2. Confirm loading state appears while a plan/profile is being captured.
3. If a future or malformed native plan is encountered, confirm Tarik shows **Structured graph unavailable** with the raw plan rather than crashing or inventing nodes.
4. Confirm closing/reopening the project still permits normal query execution after Explain/Profile use.

## Automated verification recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (13 passing result groups)
- `npm run lint` (0 errors; four known test-shim warnings)
- `npm run typecheck`
- `npm test` (7 passing command/protocol tests)
- `npm run test:ui` (77 passing UI/unit tests)
- `npm run build`

## Sign-off

- [ ] Explain semantics and estimated labeling approved
- [ ] Join graph/connectors/traversal and controls approved
- [ ] Automatic estimated Flow on Run approved
- [ ] Beginner inspector/details approved
- [ ] Profile execution and actual labeling approved
- [ ] Best-effort SQL highlighting approved
- [ ] Fallback/error states approved

When all items pass, reply **“E7 approved”**. E8 remains blocked until that approval.

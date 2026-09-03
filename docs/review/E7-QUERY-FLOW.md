# EPIC E7 Review — Beginner-friendly query flow

**Status:** REVIEW  
**Scope:** Estimate/Actual Flow capture, interpreted DuckDB plan graph, XYFlow visualization, beginner inspector, and conservative SQL-range mapping.

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
3. Confirm the lower panel switches to **Estimate** and labels it **Estimate · DuckDB Explain**.
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
7. Click the toolbar **Estimate** action. Confirm a viewport-sized three-pane page opens with Planned SQL, graph, and inspector.
8. Click **Present**. Confirm the graph smoothly focuses the upper-left source operation, selects its node, highlights reliable Planned SQL, and opens its inspector.
9. Close Estimate with `Esc`. Click **Actual Flow** and confirm it uses the same three-pane structure with Profiled SQL and actual metrics.
10. Confirm both pages preserve empty/loading/error states, bounded side panes, pan/zoom/fit controls, and reduced-motion behavior.

## 3. Explicit query destinations

1. Click **Run query** on current SQL. Confirm Tarik stays on or opens **Results**, executes SQL exactly once, and does not call Estimate or Actual Flow.
2. Confirm Results is the only bottom output surface. Click the toolbar **Estimate** action to capture current SQL without execution.
3. Change the source table in the editor. Confirm an open Estimate says **Editor SQL changed** and remains tied to its older Planned SQL until **Build current SQL**.
4. Click **Actual Flow** beside Query Library. Confirm it profiles the current editor SQL and opens a dedicated analysis workspace.
5. Confirm the workspace has three visible panes: immutable **Profiled SQL** on the left, execution graph in the center, and operation details on the right.
6. Change editor SQL after profiling. Confirm the workspace says **Editor SQL changed**, keeps the old profiled SQL snapshot, and does not silently replace its graph.
7. Click **Run current SQL**. Confirm Actual Flow profiles the new current SQL only after this explicit action.
8. Run a harmless DDL statement with normal Run and confirm it executes once. Run Actual Flow only after accepting its mutation warning.

## 4. Beginner inspector and interpreted operators

1. Select Read data, Filter rows, Join, Group and summarize, Sort, Return columns, and Limit nodes.
2. Confirm the inspector shows:
   - a plain-language description;
   - clear Input and Output meanings;
   - native DuckDB operator name;
   - a prominent `Planning estimate, not result count` explanation in Estimate;
   - source and `DuckDB estimated output` when reported;
   - join type/conditions, filters, groups/aggregates, projections, sort keys, or limits when DuckDB reports them.
3. For a Filter followed by Choose columns, confirm both may repeat the same estimate and the projection inspector explains that choosing columns normally changes columns, not which rows match.
4. Click empty graph space and confirm the inspector returns to **Select an operation**.
5. For a filter pushed into DuckDB's scan, confirm the graph shows Read data → Filter rows, the filter condition appears, rows scanned and combined operator time remain on Read data, and estimated/actual output appears on Filter rows.
6. Confirm the inspector says the Filter is a beginner presentation of fused native scan work—not a separate physical DuckDB operator.
7. Confirm Actual Flow contains exactly one Query result node even when native details list QUERY and EXPLAIN_ANALYZE wrappers.
8. Confirm no operator explanation makes claims beyond the native details shown.

## 5. Actual Flow executes and reports measured metrics

1. Use a read-only query first and click the toolbar **Actual Flow** action.
2. Confirm the dedicated workspace opens and profiles that current SQL. Use **Run current SQL** for later reruns.
3. Confirm the panel says **Actual Flow · DuckDB Profile**, shows measured output, rows scanned, and explicit **Operator time** where DuckDB reports it.
4. On operators where DuckDB reports both values, confirm nodes show `Est. ~N · Actual N` plus a textual estimate-accuracy badge:
   - difference below `10×` → green;
   - `10×` through `100×` → yellow;
   - above `100×` → red.
5. Confirm both over-estimates and under-estimates are labeled, and zero mismatches use `∞×` without crashing.
6. Select a compared node and confirm the inspector explains the factor measures estimate accuracy, not query speed. Use operator time and rows scanned to assess performance.
7. Confirm Estimate and Actual Flow retain independent last plans when switching tabs.
8. Run Actual Flow for a failed SQL statement and confirm a structured error with **Try again** appears.
9. Run Actual Flow for a DDL/DML statement. Confirm the mutation warning says Actual Flow executes SQL, cancelling does nothing, and accepting executes the statement exactly once.

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

1. Confirm the empty Estimate workspace shows one Build current SQL action, and Actual Flow shows one Run current SQL action.
2. Confirm loading state appears while an Estimate/Actual Flow is being captured.
3. If a future or malformed native plan is encountered, confirm Tarik shows **Structured graph unavailable** with the raw plan rather than crashing or inventing nodes.
4. Confirm closing/reopening the project still permits normal query execution after Estimate/Actual Flow use.

## Automated verification recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace` (13 passing result groups)
- `npm run lint` (0 errors; four known test-shim warnings)
- `npm run typecheck`
- `npm test` (7 passing command/protocol tests)
- `npm run test:ui` (81 passing UI/unit tests)
- `npm run build`

## Sign-off

- [ ] Estimate semantics and labeling approved
- [ ] Join graph/connectors/traversal and controls approved
- [ ] Explicit Run query / Estimate / Actual Flow destinations approved
- [ ] Beginner inspector/details approved
- [ ] Actual Flow execution and actual labeling approved
- [ ] Best-effort SQL highlighting approved
- [ ] Fallback/error states approved

When all items pass, reply **“E7 approved”**. E8 remains blocked until that approval.

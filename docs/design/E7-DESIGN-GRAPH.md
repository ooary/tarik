# E7 Beginner-friendly query flow

## Design read

Dense local SQL workbench for beginner data engineers, with a calm IDE language and progressive disclosure. Query flow is an inspection tool, not a decorative diagram.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 2`
- `VISUAL_DENSITY: 7`
- Foundation: existing semantic tokens + Radix primitives
- Specialized UI: `@xyflow/react` with deterministic directed layout

## PROBLEM

Turn DuckDB Explain/Profile output into a connected, deterministic, truthful graph that teaches beginners what their query does without inventing semantics.

```text
X -> DesignGraph<A, E, R>
|              |  |  |
|              |  |  `- R: engine session, DuckDB plan JSON, parser, layout, editor
|              |  `---- E: unsupported syntax, malformed plan, unknown operator, stale SQL mapping
|              `------- A: PlanRequest, RawPlan, QueryPlan, PlanNode, PlanEdge, Inspector
|
`- E7 query flow
```

## SHAPES

- IDs: `ExecutionId`, `PlanId`, `PlanNodeId`
- Records: `PlanRequest`, `RawPlan`, `QueryPlan`, `PlanNode`, `PlanEdge`, `PlanMetric`, `SqlRange`
- Variants:
  - `PlanMode = explain | profile`
  - `PlanState = loading | ready | unsupported | failed`
  - `SqlMapping = reliable(range) | unavailable(reason)`
- Errors: `NoSession`, `EmptySql`, `ExplainUnsupported`, `PlanParse`, `MalformedPlan`, `EngineError`

## GRAPH

```text
request_plan (1)
| R: active project/session, immutable SQL snapshot
| E: EmptySql -> escape(empty state)
| boundary: editor text + mode -> PlanRequest
v
engine_explain (1)
| R: DuckDB adapter
| E: SQL/engine error -> escape(structured flow error)
| boundary: DuckDB JSON/text -> RawPlan
v
normalize_plan (N nodes)
| R: versioned adapter parser
| E: malformed structured JSON -> escape(raw fallback)
| unknown operator -> escape(generic node, preserve details)
| behavior: deterministic preorder IDs and source-to-result edges
v
layout_plan (1)
| R: deterministic directed layout
| E: layout defect -> die(ErrorBoundary)
| behavior: stable positions for the same normalized graph
v
render_flow (N visible nodes)
| R: XYFlow viewport
| behavior: pan/zoom/fit; no decorative motion
v
inspect_node (1)
| R: explanation dictionary + truthful raw details
| unknown operator -> escape(generic explanation)
|
`- map_sql_range (0..1)
   R: reliable adapter metadata / conservative SQL scanner
   unavailable -> escape(no highlight)
```

## CARDINALITY

`request_plan (1)`; `engine_explain (1)`; `normalize_plan (N nodes)`; `layout_plan (1)`; `render_flow (N visible nodes)`; `inspect_node (1)`; `map_sql_range (0..1)`.

## BOUNDARIES

- CodeMirror text -> immutable non-empty plan request
- Engine JSON envelope -> typed raw plan
- DuckDB Explain/Profile JSON -> versioned adapter shape
- Normalized graph -> XYFlow nodes/edges
- Best-effort SQL range -> editor selection only when reliable

## BEHAVIOR

- Explain labels row values as **estimated**.
- Profile labels row/timing values as **actual**.
- Unknown operators retain native name and raw details; no guessed explanation.
- Deterministic preorder IDs and layout make fixture snapshots stable.
- Plan capture never executes user DML for Explain; Profile is explicitly user-triggered because it executes the statement.

## SCOPE

- Plan request: acquire at button click -> finish/error/cancel at command completion
- Raw plan: response scoped; no result page artifacts required
- Selected node: acquire at graph selection -> clear on new plan/tab/project
- Editor highlight: acquire only for reliable mapping -> clear on deselect/new SQL/unmount

## TEST LAYERS

- Checked-in DuckDB 1.5.5 Explain/Profile fixtures with sanitized relation names
- Golden normalized graphs for every supported operator
- Malformed and unknown-operator fixtures
- Fake plan engine for command lifecycle
- Deterministic layout tests
- React graph/inspector tests with mocked viewport APIs
- SQL range reliable/unsupported fixtures

## IMPLEMENTATION ORDER

1. **E7-T1:** capture sanitized DuckDB 1.5.5 fixtures and compatibility manifest.
2. **E7-T2:** add `plan.explain` engine operation plus normalized graph parser and raw fallback.
3. **E7-T3:** install `@xyflow/react`, deterministic layout, loading/empty/error/unsupported states.
4. **E7-T4:** beginner explanation dictionary and selected-node inspector.
5. **E7-T5:** conservative, best-effort SQL range bridge; never highlight an uncertain range.

Each task receives an atomic Conventional Commit. E7 stops in REVIEW after T5 for visual and beginner-usability sign-off.

## UI STATES

- Empty: “Run Explain to see how DuckDB plans this query.”
- Loading: stable diagram-shaped skeleton
- Ready: source-to-result graph, fitted viewport
- Unsupported: raw plan is available; structured graph is not
- Failed: structured engine code/message
- Node selected: inspector with operation, input/output, estimated/actual metrics, native details
- SQL mapping unavailable: no highlight and plain “SQL location unavailable” text

## VERDICT

The design is viable using DuckDB 1.5.5 native JSON plans. Explain and Profile differ structurally, so the adapter must parse each explicitly into one normalized graph. Unknown operators and malformed plans must preserve raw output. Query execution/results remain independent: Explain does not need result pages, while Profile is a distinct explicit operation because it executes the query.

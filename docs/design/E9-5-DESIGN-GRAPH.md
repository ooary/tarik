# E9.5 Beginner SQL intelligence

## Design read

Tarik is a dense local SQL workbench for beginner data engineers. SQL intelligence must teach query meaning without pretending conceptual teaching steps are separate DuckDB work, and it must help before execution without claiming runtime certainty.

- `DESIGN_VARIANCE: 3`
- `MOTION_INTENSITY: 1`
- `VISUAL_DENSITY: 8`
- Foundation: existing semantic tokens, CodeMirror 6, DuckDB 1.5.5, XYFlow
- Trust rule: DuckDB native plans and diagnostics remain authoritative; SQL-derived labels and ranges appear only when mechanically reliable

## PROBLEM

Explain aggregate intent specifically, complete catalog-aware SQL safely, and show useful pre-run errors without executing user statements or inventing physical work.

```text
X → DesignGraph<A, E, R>
│              │   │  │  │
│              │   │  │  └─ R: immutable SQL, DuckDB plan/catalog/binder, CodeMirror, revision clock
│              │   │  └──── E: ambiguous SQL mapping, unsupported plan, stale completion/diagnostic, binder failure
│              │   └─────── A: SqlSemantics, SemanticPlanNode, Completion, SqlValidation, Diagnostic
│              │
│              └─ nodes = functions, edges = data flow
│
└─ E9.5 beginner SQL intelligence
```

## SHAPES

- IDs: `PlanNodeId`, `SqlRevision`, `ValidationRequestId`, `ProjectId`, `SessionId`
- Records: `SqlToken`, `SqlRange`, `SelectExpression`, `GroupExpression`, `AggregateCalculation`, `QuerySemantics`, `NativePlanNode`, `SemanticPlanNode`, `CatalogRelation`, `AliasBinding`, `Completion`, `SqlValidation`, `Diagnostic`
- Variants:
  - `AggregateKind = countRows | countNonNull | countMatching | countUnique | sum | average | minimum | maximum | unknown`
  - `AggregateScope = wholeInput | perGroup`
  - `SemanticOperator = read | filter | join | group | count | sum | average | minimum | maximum | summaries | distinct | sort | limit | result | unknown`
  - `Mapping = reliable(range) | ambiguous(reason) | unavailable(reason)`
  - `CompletionContext = relation | schemaRelation | aliasColumn | unqualifiedColumn | general`
  - `DiagnosticSeverity = error | warning`
  - `ValidationState = idle | editing | checking | clean | problems | unavailable`
- Errors: `SqlSemanticsUnsupported`, `PlanParse`, `SemanticMismatch`, `ValidationMissingSession`, `ValidationParseBind`, `DiagnosticRangeUnavailable`, `StaleRevision`, `CatalogUnavailable`

## GRAPH

### Aggregate-specific query flow

```text
capture_plan(sql, mode) (1)
│ R: active engine session, immutable SQL snapshot
│ E: Explain/Profile failure ↯escape(existing flow error)
│ 🔒 editor SQL + DuckDB JSON → PlanInput
├─> parse_sql_semantics(sql) (1)
│   │ R: conservative tokenizer/parser
│   │ E: unsupported/ambiguous expression ↯escape(unverified semantics)
│   │ 🔒 SQL text → QuerySemantics(reliable fields only)
│   └─> match_semantics_to_native_plan (N)
│       │ R: DuckDB details + physical edges
│       │ E: count/detail mismatch ↯escape(generic native aggregate)
│       └─> expand_aggregate_concepts (N)
│           │ R: verified groups/calculations/distinct
│           │ E: physical metric duplication ☠die(test invariant)
│           └─> collapse_internal_projections (N)
│               │ R: conservative aggregate-plumbing predicate
│               │ E: uncertain projection ↯escape(keep native projection)
│               └─> normalize_beginner_graph (1)
│                   └─> render_flow + inspect_node + map_sql_range (N)
│                       R: XYFlow, immutable SQL, explanation dictionary
└─> preserve_raw_plan (1)
    R: original DuckDB payload
```

For the accepted query:

```sql
SELECT DISTINCT commodity, count(market)
FROM "main"."data_2021"
GROUP BY commodity;
```

```text
Read data_2021 (physical scan metrics)
→ Group rows by commodity (concept only; no duplicated native metrics)
→ Count non-null market values per group (physical HASH_GROUP_BY metrics)
→ Remove duplicate result rows (outer aggregate-with-no-calculations metrics)
→ Query result
```

### Catalog-aware completion

```text
editor_change(sql, cursor, catalogRevision) (N)
│ R: CodeMirror document + current ProjectCatalog
│ 🔒 SQL prefix + cursor → CompletionContext
└─> classify_completion_context (1)
    │ E: incomplete/unsupported SQL ↯escape(general language completion)
    ├─ relation → suggest_relations(catalog) (N)
    ├─ schemaRelation → suggest_schema_relations(schema) (N)
    ├─ aliasColumn → resolve_aliases(sql) → suggest_bound_columns(alias) (N)
    │                 E: duplicate/ambiguous alias ↯escape(no alias columns)
    ├─ unqualifiedColumn → referenced_relations → suggest_unambiguous_columns (N)
    └─ general → preserve CodeMirror SQL keyword/function completion (N)
        │ R: safe_identifier_insert
        └─> Completion(label, quotedApply, type, detail)
```

### Non-executing diagnostics

```text
editor_change(sql, project, revision) (N)
│ R: CodeMirror update listener
├─> clear_visible_diagnostics(revision) (1)
└─> debounce_idle(650ms) (T)
    │ E: subsequent edit ↯escape(cancel timer)
    └─> request_validation(project, sql, revision) (1)
        │ R: active project/session, sidecar protocol
        │ E: project/session mismatch ↯escape(unavailable summary)
        │ 🔒 IPC request → immutable ValidationRequest
        └─> split_statements_with_ranges(sql) (N)
            │ R: tokenizer preserving byte offsets
            │ E: unterminated token ↯escape(local reliable syntax diagnostic)
            └─> duckdb_explain_without_analyze(statement) (N)
                │ R: DuckDB connection; `EXPLAIN (FORMAT JSON)` only
                │ E: parser/binder/catalog error ↯escape(structured Diagnostic)
                │ E: user statement execution ☠die(mutation-sentinel invariant)
                ├─> parse_duckdb_error_range(error, statementRange) (0..1)
                │   E: no reliable caret/range ↯escape(message-only diagnostic)
                └─> high_confidence_warnings(statement, semantics) (N bounded)
                    └─> SqlValidation(revision, diagnostics)
                        │ 🔒 sidecar response → typed validation
                        └─> accept_if_current_revision (1)
                            │ E: stale revision ↯escape(discard response)
                            └─> CodeMirror lint markers + gutter + summary (N)
```

## CARDINALITY

`capture_plan (1)` · `parse_sql_semantics (1)` · `match_semantics_to_native_plan (N nodes)` · `expand_aggregate_concepts (N semantic nodes)` · `collapse_internal_projections (N candidates)` · `normalize_beginner_graph (1)` · `render/inspect/map (N)` · `editor_change (N)` · `classify_completion_context (1 per request)` · suggestions `(N bounded catalog entries)` · `clear_visible_diagnostics (1 per edit)` · `debounce_idle (T)` · `request_validation (1 per settled revision)` · `split/EXPLAIN (N statements)` · diagnostics `(N bounded)` · `accept_if_current_revision (1)`.

## BOUNDARIES

- The immutable SQL snapshot is passed into plan normalization. DuckDB positional details such as `#0` and `#1` are never renamed from plan JSON alone.
- SQL tokenization skips comments and string literals, preserves quoted identifier content/ranges, tracks parenthesis depth, and returns only mechanically recognized SELECT/FROM/GROUP BY/DISTINCT/aggregate shapes.
- A semantic label is emitted only when SQL calculations and DuckDB aggregate details agree in count/kind. Otherwise Tarik retains the generic native aggregate and raw details.
- Concept nodes and physical nodes are explicit. Exactly one semantic node owns each native operator's estimated rows, actual rows, timing, and rows scanned.
- Redundant DISTINCT is disclosed only when every selected expression is a verified group key or deterministic recognized aggregate and no grouping sets/volatile unmatched expression is present.
- Completion uses current project catalog records only. Alias bindings come from recognized top-level FROM/JOIN clauses; ambiguous aliases produce no scoped column completion.
- Completion insertion always quotes identifiers that are not simple non-reserved SQL identifiers and doubles embedded quotes.
- Editor text/cursor and IPC are untrusted. Completion and validation parse at their respective boundaries.
- Validation wraps each statement in DuckDB `EXPLAIN (FORMAT JSON)` without `ANALYZE`. User SQL is never sent to query.execute and no result is published.
- DuckDB error source ranges are accepted only from a parsed line excerpt and caret that maps inside the exact immutable statement. Otherwise diagnostics carry a message with no underline.
- Validation responses carry the originating revision. Frontend diagnostics are applied only when project, revision, and current document still match.

## BEHAVIOR

- One verified aggregate calculation receives a specific beginner label. Examples: `Count rows`, `Count non-null market values per group`, `Count unique customer_id values`, `Sum amount`, `Calculate average amount`, `Find minimum date`, `Find maximum amount`.
- Multiple calculations from one native aggregate become one `Calculate summaries` node with an ordered list. Tarik never renders sibling calculations as sequential physical passes.
- `GROUP BY` and aggregate calculation may be separate teaching nodes backed by one physical operator. The concept-only Group node carries no native cost/cardinality; its inspector explains shared physical work.
- Aggregate without GROUP BY summarizes the whole input and does not show a Group node.
- DuckDB aggregate-with-empty-calculations caused by SELECT DISTINCT becomes `Remove duplicate result rows`.
- Internal compression/decompression and positional projections collapse only in a verified aggregate pipeline. Uncertain projections remain visible.
- Raw/native plan remains available unchanged.
- Relation suggestions rank exact-prefix project tables/views ahead of schemas and general SQL language items. Table and view labels remain textually distinct.
- `schema.` completion is scoped to that schema. `alias.` completion is scoped to the uniquely bound relation. Unqualified columns appear only when unambiguous among recognized sources.
- Catalog updates reconfigure CodeMirror compartments without destroying editor history, selection, or draft text.
- Diagnostics clear immediately on edit, begin only after idle, and never block typing, Run, Estimate, Actual Flow, or Export.
- Validation reports “No problems detected before execution,” never “valid” or “guaranteed.” Runtime-only failures remain possible.
- Initial warning allowlist is intentionally small: provably redundant DISTINCT after equivalent GROUP BY, and clearly mutating UPDATE/DELETE without a top-level WHERE. No style lint.

## SCOPE

- Plan SQL semantics: acquire at explicit Estimate/Profile request → release with normalized plan; raw plan retained in `QueryPlan`.
- Completion source: acquire with editor mount/catalog revision → reconfigure on catalog change → release on editor destroy.
- Alias parse: scoped to one completion request; no cross-document cache.
- Validation debounce timer: acquire after document change → clear on next edit/project change/unmount.
- Validation request: acquire immutable `{project, sql, revision}` → accept/discard exactly once.
- DuckDB Explain statement/Arrow iterator: acquire per validation statement → drop before next statement/response on success or error.
- CodeMirror diagnostics: acquire only for accepted current revision → clear immediately on next document change/unmount.
- Compact problem summary: active-tab scope; clear on tab/project change.

## TEST LAYERS

- Checked-in sanitized DuckDB Explain/Profile fixtures for grouped/ungrouped count variants, sum/avg/min/max, multiple calculations, DISTINCT, and aggregate plumbing.
- Pure SQL semantic parser tests for comments, quoted identifiers, nested expressions, aliases, DISTINCT, GROUP BY, ambiguity, and unsupported fallback.
- Plan invariant tests verify accepted example order, one owner per physical metric, no sequential multi-aggregate lie, raw preservation, and conservative fallback.
- Pure CodeMirror `CompletionContext` tests with catalog fixtures for FROM/JOIN, schema qualification, aliases, ambiguity, safe quoting, manual trigger, and empty catalog.
- Mounted editor tests verify catalog reconfiguration preserves document and `Ctrl+Space` opens completion.
- Sidecar validation integration uses temporary DuckDB tables and mutation sentinels. EXPLAIN validation of INSERT/UPDATE/DELETE/CREATE must leave rows/catalog unchanged.
- Diagnostic parser fixtures cover syntax, binder missing-table/column, quoted/multiline SQL, reliable caret mapping, and message-only fallback.
- Desktop command tests prove active-project validation and typed revision response.
- React fake validation layer controls delay and response order to prove debounce, immediate clear, stale response discard, project switch, accessibility, and unmount cleanup.
- Existing E7 graph, editor session, query execution, export, and catalog-refresh suites remain unchanged except intentional labels.

## IMPLEMENTATION ORDER

1. **E9.5-T1:** commit this graph and fixture inventory.
2. **E9.5-T2:** SQL semantic shapes/parser, plan semantic expansion/collapse, UI labels/inspector/ranges, fixtures and physical-metric invariants.
3. **E9.5-T3:** conservative catalog completion source, safe quoting, alias resolution, live compartment reconfiguration, interaction tests.
4. **E9.5-T4:** shared diagnostic contract, sidecar non-executing EXPLAIN validator, active-project command, official CodeMirror lint integration, revision/debounce tests.
5. **E9.5-T5:** full gates, sidecar rebuild, combined manual checklist, stop in REVIEW.

## UI STATES

- Aggregate verified: specific semantic nodes and plain-language calculation details.
- Aggregate partially verified: generic `Calculate summary` with native details; no guessed column/function wording.
- Group concept selected: disclosure that Group and calculation share one DuckDB operator and measured costs are shown once.
- Multiple aggregates: one `Calculate summaries` node with ordered calculations.
- DISTINCT proven redundant: warning note in inspector, not an execution error.
- Relation completion: table/view/schema rows with type text and safely quoted insertion.
- Alias completion unavailable: fall back to general completion; no unrelated columns claimed.
- Diagnostics editing: old markers removed immediately.
- Diagnostics checking: subtle `Checking SQL` summary; no spinner or modal.
- Diagnostics clean: `No problems detected before execution` available without claiming success.
- Diagnostics problems: count plus first message in toolbar; gutter/underline and hover carry severity and detail.
- Diagnostics unavailable: non-blocking status text; Run remains available.
- Diagnostic without reliable range: summary message only, no arbitrary underline.

## VERDICT

The existing generic aggregate normalization, schema-only CodeMirror setup, and execution-only error path do not satisfy E9.5. The design adds one conservative SQL semantics input to plan normalization, one catalog-aware completion source layered with CodeMirror SQL language support, and one sidecar EXPLAIN-without-ANALYZE validation boundary. The graph remains truthful only if semantic concepts never duplicate physical metrics, ambiguous names/ranges fall back instead of guessing, and mutation-sentinel tests prove validation never executes user statements. No implementation exists yet for these new paths; T2-T4 must match this graph.

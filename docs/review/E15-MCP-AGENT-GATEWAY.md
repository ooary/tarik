# E15 Local Agent Gateway review packet

**Status:** implementation candidate; manual sign-off required. Linux automated evidence is recorded below. Native Windows runtime, host-specific launches, accessibility, DPI, and visual approval remain user-owned gates and are not claimed.

## What is implemented

The packaged native `tarik-mcp` stdio child connects to the visible Tarik desktop through a user-private Unix socket or current-user-restricted Windows named pipe. Tarik owns pairing, project grants, catalog identity, SQL classification, execution, result artifacts, approval, mutation transactions, and audit.

Supported MCP handshake revisions are `2025-06-18` and `2025-11-25`. The public contract is a static allowlist of 20 tools:

- status, granted-project listing, bounded catalog listing, and relation description;
- immutable SQL classification;
- bounded SafeRead start/status/cancel/page/release;
- Estimate or explicit Actual query flow from a SafeRead snapshot;
- approval proposal/status/execute-by-ID, with no MCP approval method;
- bounded Profile start/status/cancel;
- read-only Quality definitions and aggregate run history;
- read-only saved-query listing when Modify workspace is explicitly granted.

Broad project, path, source, export, settings, support, history-clear, check CRUD, saved-query CRUD, editor mutation, and failure-preview commands are not mirrored. Unknown or unsupported operations have no route.

## Security evidence

- New clients have no inferred grants; grants are project- and capability-scoped.
- HMAC challenge authentication binds a fresh connection to a persisted private profile verifier.
- Discovery omits project/source paths and uses revision-bound HMAC cursors.
- `sqlparser` 0.62 recursively classifies one DuckDB-dialect statement with bounded recursion.
- Pinned DuckDB independently parses the same input; SafeRead additionally receives non-executing EXPLAIN bind/plan validation.
- Unknown relations, ordinary views, macros, user-defined functions, external readers/URLs, extensions, secrets, COPY/ATTACH, settings, PRAGMA/CALL, dynamic identifiers, explicit transactions, and parser disagreement are blocked with no approval fallback.
- SafeRead starts only from a one-use server-held snapshot and reclassifies against a stronger catalog/function/registered-source revision.
- Result execution is capped at 5,000 rows without SQL rewriting; capped totals are reported inexact. Pages preserve NULL and truncation markers.
- Mutation proposals bind exact SQL, client, connection, project, risk, affected objects, filter state, and catalog revision into a SHA-256 snapshot hash.
- Approval requires direct Tarik interaction. Critical actions require an exact generated phrase. Pending and approved requests expire after 120 seconds.
- Approved execution accepts only approval ID, atomically marks it used, revalidates capability and revision, and executes the server-held SQL inside one DuckDB transaction.
- Mutation execution is synchronous and exclusive in the sidecar protocol loop and rejects active query/export/profile work; work accepted after commit sees the refreshed catalog.
- Rollback failure has a distinct code and freezes agent mutation for that project until close/reopen.
- SQLite schema 10 retains at most 5,000 audit facts per project; normal audit/log records omit SQL, values, typed phrases, secrets, and paths.
- Disconnect, revoke, project close, disable, and shutdown cancel/release owned queries, results, profiles, snapshots, and approvals.

## Automated gates

Run from repository root:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
npm run typecheck
npm run test:ui
npm test
npm run build
npm run lint
npm run format:check
npm run docs:check
npm run engine:check
npm run release:linux
git diff --check
```

Focused evidence includes:

- MCP subprocess initialize/tool-list/tool-call/EOF tests for both reviewed revisions and fallback from an unreviewed handshake revision;
- private profile permission tests;
- pairing, proof, grant, owner, revoke, approval phrase, and replay tests;
- classifier adversarial statement families and registered-source provenance tests;
- transaction commit and rollback tests;
- engine row-cap smoke: 6,000 generated rows produce exactly 5,000 retained rows with `rowCountExact=false`, NULL fidelity, and 64-KiB cell truncation evidence;
- schema 7→10 migration and bounded SQL-free audit tests;
- Linux portable, DEB, and AppImage package-content/startup checks including `tarik-mcp`.

Current Linux candidate evidence (September 11, 2026):

- `cargo test --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- frontend: 30 files / 216 tests passed.
- Node contract/package tests: 36 passed.
- production TypeScript/Vite build: passed; the existing large-chunk advisory remains non-fatal.
- ESLint: passed with only the two pre-existing TanStack Virtual/Fast Refresh warnings.
- Prettier, documentation checks (42 Markdown files), engine handshake, Cargo all-target build, and diff checks: passed.
- Linux release: passed for portable tarball, DEB, and AppImage. The portable archive and DEB contain `tarik`, `tarik-mcp`, `tarik-engine-duckdb`, and `libduckdb.so`; checksums and clean-start smokes passed.
- Native Windows evidence: not run and not claimed.

## Consolidated manual review

### Pair and grant

1. Run packaged Tarik and open a disposable project.
2. Enable **Agent access**.
3. Configure one MCP host using `docs/user/MCP-AGENT-SETUP.md`.
4. Confirm a pending pairing appears; test Deny once, reconnect, then Pair.
5. Confirm the client initially has no project grant.
6. Grant Inspect + Analyze only. Verify the host lists only this project and no filesystem path.

### Read lifecycle

1. List catalog and describe a wide relation with continuation metadata.
2. Classify and run a safe query larger than 5,000 rows.
3. Verify status, `rowTotalExact=false`, first/later pages, NULL/truncation fidelity, cancellation, and release.
4. Run Estimate Flow and explicitly run Actual Flow; verify Estimate does not run the original query.
5. Revoke Analyze and verify subsequent query/status/page calls fail closed.

### Approval and mutation

1. Restore Modify data.
2. Classify filtered INSERT/UPDATE/DELETE and propose it.
3. Confirm Tarik shows exact SQL, client, project, objects, filter state, snapshot hash, countdown, Deny, and Approve once.
4. Approve, execute by ID, verify one commit and one audit fact; retry the same ID and confirm replay fails.
5. Propose unfiltered UPDATE/DELETE or DROP in a disposable relation. Verify Approve is disabled until the exact generated phrase is typed; deny one critical request.
6. Confirm MCP-host confirmation alone cannot approve and no `tarik_approve` tool exists.

### Data trust and lifecycle

1. Profile selected columns; verify Exact/Approximate/Sampled labels and exact SQL evidence.
2. List Quality definitions and aggregate history; verify no failure rows are retained/exposed.
3. List saved SQL with Modify workspace, then remove that grant and verify access fails.
4. Close the project, disable Agent Access, revoke the client, restart Tarik, and close the host. Verify stale IDs fail and no `tarik-mcp`, engine, result, profile, or approval residue remains.

### Visual/accessibility matrix

Review Agent Access and approval content at minimum supported viewport in light, dark, and system themes; keyboard-only and screen-reader operation; reduced motion; and Windows 100/125/150/200% DPI plus mixed monitors. Check focus trap/restore, non-color-only state, readable exact SQL, countdown, critical phrase, responsive stacking, and action labels.

## Known truthful limitations

- Tarik Desktop must already be running and the project must already be active.
- No remote/HTTP transport, daemon, embedded model, credentials, arbitrary paths, or headless project ownership exists.
- Quality and saved-query mutation workflows are blocked in v1 rather than exposed through broad CRUD commands.
- Native Windows runtime/package evidence must be produced on native x64 MSVC Windows. Cross-compilation is not accepted.
- This security boundary does not isolate Tarik from malware already running as the same OS user.

## Sign-off

- [ ] Pairing and project grant UX approved.
- [ ] SafeRead/catalog/flow/Profile/Quality/saved-query workflow approved.
- [ ] Standard and critical mutation approval UX approved.
- [ ] Light/dark/system, keyboard, screen reader, reduced motion, minimum viewport approved.
- [ ] Native Windows x64 MSVC package, WebView2, DPI, mixed-monitor, and process lifecycle approved.
- [ ] E15 accepted by the user.

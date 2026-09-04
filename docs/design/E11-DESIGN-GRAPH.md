# E11 quality, performance, packaging, and release design graph

PROBLEM: Turn the implemented local workbench into a repeatably tested, measured, least-privilege, distributable, and documented Linux MVP without hiding deferred review gates or platform limits.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: real sidecar, isolated filesystem, process metrics, Tauri bundler, release metadata
│ │ │ └──── E: fixture/setup failure, budget regression, unsafe command/path, bundle/runtime dependency, documentation drift
│ │ └─────── A: golden-workflow report, benchmark report, security audit, release artifacts, user guide
│ │
│ └─ nodes = functions, edges = data flow
│
└─ the problem: a measurable and shippable Tarik MVP

## SHAPES

- `GoldenRoot`: unique temporary directory owning metadata SQLite, managed projects, result cache, source fixtures, exports, and logs for one test run.
- `GoldenProject`: project ID plus managed DuckDB path created through the real project manager and sidecar.
- `GoldenEvidence`: exact imported/linked catalog objects, joined result rows, Explain/Profile nodes, saved-query/history/export records, and restored session tabs.
- `GoldenFailure`: named setup, missing-link, cancellation, persistence, or restart failure with the last completed step.
- `BenchmarkScenario`: `idle | import | large_result | repeated_query | export_cancel` with fixed dataset shape and iteration count.
- `MemorySample`: monotonic timestamp plus desktop/sidecar RSS and cache/output bytes.
- `Budget`: per-scenario peak RSS and allowed post-cycle growth; budgets are regression thresholds, not performance claims.
- `BenchmarkReport`: timestamp, git revision, CPU/RAM/OS/Rust/Node/DuckDB details, scenario inputs, samples, peak, post-cycle delta, and verdict.
- `CommandSurface`: exact Tauri invoke handler names plus required plugin permissions.
- `BoundaryFinding`: severity, boundary, exploit input, observed behavior, required fix, and regression test.
- `ReleaseManifest`: product/version/target, artifact paths, SHA-256 values, required runtime dependencies, and included license files.
- `CompatibilityContract`: SQLite schema range, DuckDB project ownership behavior, downgrade limitation, backup paths, and sidecar protocol version.
- Errors: `GoldenSetup`, `GoldenAssertion`, `MetricUnavailable`, `BudgetExceeded`, `UnsafeCommand`, `UnsafePath`, `BundleMissing`, `ChecksumMismatch`, `DocumentationDrift`.

## GRAPH

### A. End-to-end golden workflows

```text
create_isolated_root (1)
│  R: temporary filesystem
│  E: create failure ☠die(test environment)
↓
start_real_services (1)
│  R: real metadata migrations, real DuckDB sidecar binary
│  E: sidecar/migration failure ☠die(golden test)
↓
create_project → import_csv → link_parquet (1 each)
│  R: ProjectManager, source fixtures
│  E: operation failure ☠die with named step
│  🔒 fixture paths/options → validated source operations
↓
execute_join_query → read_bounded_page (1)
│  R: QueryCoordinator, ResultStore
│  E: timeout ☠die · result mismatch ☠die
↓
capture_explain → capture_profile (1)
│  R: plan adapter, immutable SQL
│  E: unsupported plan ↯escape(assert truthful fallback) · capture failure ☠die
↓
save_query → export_exact_parts (1)
│  R: metadata repositories, ExportCoordinator
│  E: timeout/write mismatch ☠die
↓
flush_session → graceful_close (1)
│  R: session repository, managers
│  E: persistence/close failure ☠die
↓
restart_real_services → reopen_project (1)
│  R: same GoldenRoot, new process/service objects
│  E: reopen/migration failure ☠die
↓
assert_restore_history_saved_catalog (1)
   R: repositories, catalog
   E: invariant mismatch ☠die

fork failure workflows:
  delete_linked_fixture → reopen → assert missing state → relink (1)
  start_long_query/export → cancel → assert terminal + reusable session (1)
```

The golden test uses production managers/coordinators and the real sidecar protocol. It does not drive browser pixels; component accessibility/keyboard behavior remains in the UI suite. One test owns one root and removes it structurally even on assertion failure.

### B. Performance and memory budgets

```text
collect_machine_metadata (1)
│  R: /proc, uname, tool versions
│  E: optional metric missing ↯escape(null + warning)
↓
build_fixed_fixture (1)
│  R: real sidecar, deterministic range/CSV shape
│  E: setup failure ☠die
↓
run_scenario (N)
│  R: protocol driver, monotonic clock
│  E: operation timeout ☠die · cancellation failure ☠die
↓
sample_rss_and_disk (T)
│  R: /proc/<pid>/status, owned roots
│  E: process exits ↯escape(final sample) · metric unavailable ↯escape(warning)
↓
release_cycle_resources (N)
│  R: result.release/export.cancel/session close
│  E: cleanup failure ☠die
↓
calculate_peak_and_growth (1)
│  R: samples, Budget
│  E: no samples ☠die · BudgetExceeded ☠die(CI/stress verdict)
↓
write_json_report (1)
   R: report output path under target/e11
   E: write failure ☠die
```

Budgets are checked against sidecar RSS because DuckDB/Arrow live there; desktop/UI idle RSS is recorded separately during manual release smoke. Dataset dimensions and machine metadata are mandatory in every checked report. Result pages remain 500 rows/~4 MiB, desktop decoded cache remains 12 pages, query/export work remains one FIFO worker per session, and these defaults change only if measurements justify it.

### C. Security and filesystem boundary review

```text
inventory_invoke_surface (1)
│  R: source handler list, frontend command calls
│  E: undocumented command ☠die(audit)
↓
remove_unreachable_mutators (N)
│  R: usage proof, typed command tests
│  E: required workflow regression ☠die
↓
minimize_plugin_permissions_and_CSP (1)
│  R: Tauri capability schema, production asset needs
│  E: build/runtime rejection ☠die
↓
exercise_adversarial_inputs (N)
│  R: temp outside sentinels, unusual identifiers/paths
│  E: boundary accepts traversal/symlink/kind mismatch ☠die
↓
record_boundary_matrix (1)
   R: test evidence
   E: unresolved high finding ☠die(release blocked)
```

The frontend can invoke only commands required by visible workflows. Generic metadata mutation commands that bypass project/session/coordinator ownership are removed from the invoke surface. Native dialogs select source/project/export paths; cleanup/log/shutdown commands accept no caller-selected roots. CSP is non-null and plugin permissions are action-specific. External files are never deleted by forget/remove/cleanup.

### D. Packaging and release artifacts

```text
verify_version_contract (1)
│  R: package.json, Cargo.toml, tauri.conf.json, protocol version
│  E: mismatch ☠die
↓
build_frontend_and_release_sidecar (1)
│  R: pinned Node/Rust, prebuilt DuckDB 1.5.5
│  E: compile/link failure ☠die
↓
stage_sidecar_and_libduckdb (1)
│  R: target triple, release binaries
│  E: missing RPATH/runtime library ☠die
↓
tauri_bundle_linux (1)
│  R: Linux Tauri dependencies, icons, config
│  E: bundle failure ☠die
↓
assemble_portable_linux_archive (1)
│  R: desktop binary, sidecar, libduckdb, README, LICENSE, notices
│  E: missing file ☠die
↓
compute_checksums → verify_checksums (1)
│  R: SHA-256
│  E: mismatch ☠die
↓
clean_root_smoke (1)
│  R: temporary HOME/XDG dirs, built artifact, timeout
│  E: launch/sidecar handshake failure ☠die
↓
write_release_manifest (1)
   R: artifacts and compatibility contract
   E: incomplete manifest ☠die
```

E11 releases Linux x86_64 development-platform artifacts only. Windows portable support remains E12 and must not be implied. The release includes Tarik's MIT license and third-party notices. SQLite migrations are forward-only; users must back up `tarik.sqlite` and user DuckDB files before upgrade, and older Tarik builds may not open newer metadata.

### E. User documentation and ship gate

```text
write_beginner_workflow (1)
│  R: approved UI labels and golden fixture
│  E: stale label/path ☠die(review)
↓
document_concepts_and_limits (1)
│  R: architecture contracts
│  E: overclaim (execution safety, exactness, platform) ☠die
↓
document_data_backup_recovery (1)
│  R: resolved platform paths, ownership rules
│  E: destructive ambiguity ☠die
↓
run_doc_walkthrough (1)
│  R: clean application data, release artifact
│  E: missing prerequisite/step ☠die
↓
evaluate_ship_checklist (1)
   R: E6/E7/E10 deferred gates, E11 evidence
   E: any deferred/manual gate unsigned ↯escape(REVIEW, do not claim release)
```

Documentation uses exact visible labels and distinguishes CSV vs Parquet, import vs link, table vs view, Estimate vs Actual Flow, bounded results, exact export parts, saved queries/history, diagnostics/logs, data locations, backups, and known limitations. E11 can reach REVIEW while E6/E7/E10 remain reopenable, but final release approval cannot be claimed until all deferred gates close.

## CARDINALITY

- `(1)`: root/service setup, each lifecycle operation, restart assertions, metadata collection, fixture build, calculations/report write, surface inventory, permission/CSP hardening, boundary matrix, version check, builds, staging, bundling, archive/checksum/smoke/manifest, documentation, walkthrough, ship verdict.
- `(N)`: scenario iterations, resource-release cycles, unused-command removals, adversarial input cases.
- `(T)`: RSS/disk sampling while a scenario is active.

## BOUNDARIES

- `🔒` Golden fixture paths/options pass through the same production validation as UI-selected paths.
- `🔒` Sidecar newline JSON responses decode into typed protocol values before assertions.
- `🔒` `/proc`, uname, and tool output are optional metric inputs parsed into bounded report fields.
- `🔒` Benchmark report paths are fixed below `target/e11`; no workload SQL or rows are emitted into reports.
- `🔒` Tauri invoke JSON enters only typed commands; active-project and object ownership are rechecked in Rust.
- `🔒` File paths enter through native dialogs or typed test fixtures; generated SQL identifiers use central quoting.
- `🔒` Cleanup roots come only from Tauri's resolver; manifest-selected user files require exact UUID/part/name parsing.
- `🔒` Bundle inputs are exact release outputs; checksums authenticate bytes, not publisher identity.
- `🔒` Documentation paths are platform-specific examples, not runtime inputs.

## BEHAVIOR

- `⛈ timeout` wraps golden query/export/cancel and release smoke operations with explicit bounds.
- `⛈ cleanup-guard` structurally removes GoldenRoot and benchmark artifacts after runs.
- `⛈ metric-sampling` observes scenarios without changing protocol requests or collecting result data.
- `⛈ budget-verdict` compares reports to version-controlled limits; it does not tune production automatically.
- `⛈ checksum` wraps artifact publication and detects byte drift.
- `⛈ documentation-link-check` validates local links and command snippets independently of content flow.

## SCOPE

- GoldenRoot acquire@test start → release@guard drop.
- Sidecar process acquire@service/benchmark start → `engine.shutdown`/kill+wait@guard drop.
- Metadata SQLite and DuckDB connections acquire@service start/project open → checkpoint/close@shutdown or guard drop.
- Result pages acquire@query publication → `result.release`/shutdown/startup cleanup.
- Export stages acquire@writer → publish/drop/recovery manifest reconciliation.
- RSS sampler acquire@scenario start → stop/join@scenario end.
- Release staging root acquire@release script → replace atomically/cleanup on failure.
- Temporary HOME/XDG root acquire@smoke start → release@smoke guard.

## TEST LAYERS

- Golden tests provide real filesystem, real SQLite, real sidecar, deterministic CSV/Parquet fixtures, short poll intervals, and temporary output roots; same production graph, no database mocks.
- Benchmark harness provides fixed workloads, `/proc` sampler, real sidecar, and version-controlled budgets; a `--record` mode writes evidence and a `--check` mode fails regressions.
- Security tests provide traversal names, reserved/quoted identifiers, symlinks, mismatched project IDs/object kinds, malformed manifests, and outside sentinels.
- Packaging tests provide staged target-triple binaries and clean XDG directories; launch smoke uses the produced artifact rather than `cargo run`.
- Documentation tests provide a local-link checker, command existence checks, and a clean-data manual walkthrough.

## VERDICT

The codebase has strong per-feature unit/integration coverage, bounded query/export internals, a real sidecar handshake, icons, pinned toolchains, and atomic migration tests. It lacks one cross-feature restart golden workflow, repeatable memory/RSS evidence and explicit regression budgets, a documented invoke/capability audit, production sidecar/libduckdb bundling, repository license/notices/checksums, an up-to-date user guide, and a release checklist that preserves deferred E6/E7/E10 gates. CI currently tests only default desktop workspace members and uses a `src-tauri` manifest command that does not prove the real sidecar workspace. E11-T1 through T5 must remove those mismatches. Final verdict remains REVIEW, not RELEASED, while deferred manual gates are unsigned.

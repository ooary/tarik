# E12 pre-Windows production reachability and cleanup graph

PROBLEM: Remove accumulated Tarik codebase crust before Windows compilation without changing any production user workflow, persisted-data compatibility, runtime boundary, or release recovery path.

X → DesignGraph<A, E, R>
│ │ │ │ │
│ │ │ │ └─ R: source graph, compilers, tests, real sidecar, Linux packager, compatibility records
│ │ │ └──── E: false-unused finding, indirect runtime edge, regression, package drift
│ │ └─────── A: Baseline, ProductionRoot, Candidate, Evidence, Classification, CleanupDelta
│ │
│ └─ nodes = functions, edges = production reachability and verified removal
│
└─ the problem: production-safe pre-Windows crust removal

SHAPES: Baseline(revision, gates, source/dependency/binary/package measurements), ProductionRoot(frontend|desktop|sidecar|build|release|compatibility), ReachabilityEdge(static|dynamic|macro|wire|persisted|convention), Candidate(path, symbol, reason), Classification(remove|retain|defer), Evidence(references, runtime reason, tests), CleanupDelta(before, after), Regression(failing gate, boundary), Uncertainty(candidate, missing proof)

GRAPH:

```text
capture_baseline (1)
│  R: Git, Cargo, npm, real DuckDB sidecar, Linux Tauri bundler
│  E: baseline gate failure ↯escape(isolate and rerun before cleanup)
│  E: package measurement failure ↯escape(record command/correct invocation; no deletion)
│
├─→ enumerate_production_roots (1)
│   R: main.tsx, tauri::run, generate_handler!, sidecar main, manifests, migrations
│   E: hidden root ↯escape(classify uncertain; retain)
│   └─ 🔒 repository/configuration text → ProductionRoot
│
├─→ trace_reachability (N)
│   R: TypeScript imports/barrels, Rust modules/macros, command strings, Cargo/npm manifests
│   E: static false positive ↯escape(runtime/manual trace)
│   E: dynamic or convention edge ↯escape(retain with reason)
│   └─ 🔒 scanner output → Candidate (lead only, never deletion proof)
│
├─→ classify_candidate (N)
│   R: production roots, source history, protocol/persistence/release contracts
│   E: insufficient evidence ↯escape(defer)
│   ├─ remove → stage_boundary_delta (N)
│   ├─ retain → record_runtime_reason (1)
│   └─ defer → record_missing_proof (1)
│
├─→ verify_boundary_delta (N)
│   R: direct compiler/linter/tests, command parity, real sidecar as applicable
│   E: behavior or contract regression ↯escape(revert boundary delta)
│   └─ accepted atomic cleanup commit
│
└─→ verify_full_production_contract (1)
    R: fresh/upgrade metadata, golden restart, UI gates, sidecar, memory harness, Linux packages
    E: any regression ↯escape(bisect/revert cleanup commit; E12-T1 remains blocked)
    ├─→ measure_cleanup_delta (1)
    │   R: identical measurement method and release profile
    │   E: incomparable artifact ↯escape(report source/dependency delta only)
    └─→ accept_cleanup_gate (1)
        R: reviewable evidence, green direct gates, exact runtime parity
        E: retained orphan or uncertainty ☠die(E12-T1 stays blocked)
```

CARDINALITY: capture_baseline (1) · enumerate_production_roots (1) · trace_reachability (N) · classify_candidate (N) · stage_boundary_delta (N) · record_runtime_reason (1 per retained candidate) · record_missing_proof (1 per deferred candidate) · verify_boundary_delta (N) · verify_full_production_contract (1) · measure_cleanup_delta (1) · accept_cleanup_gate (1)

BOUNDARIES: Repository files and scanner reports are untrusted evidence leads until resolved against a production root; WebView command strings become trusted only when paired exactly with `generate_handler!`; sidecar method strings become trusted only when paired with desktop/client/script callers or a documented protocol compatibility promise; serialized SQLite/protocol shapes and migrations are compatibility boundaries and are retained unless an explicit migration/version decision proves removal safe; Cargo/Tauri platform conventions are runtime edges even without textual imports.

BEHAVIOR: ⛈ Git history explains why candidates exist but does not prove current reachability · ⛈ Clippy/TypeScript/ESLint identify statically unused local code · ⛈ import/reference scanners generate candidates only · ⛈ atomic commits make each accepted removal independently reversible · ⛈ size measurement observes cleanup but never changes the safety verdict.

SCOPE: Baseline artifacts acquire@capture_baseline → retain under `target/e12` through final comparison · staged sidecar files acquire@release script → release@script trap · DuckDB process acquire@real-sidecar/golden gate → release@RAII/shutdown · temporary projects/exports/XDG roots acquire@test → release@test/script · cleanup working tree acquire@boundary delta → commit or revert@boundary verification.

TEST LAYERS: Static layer = TypeScript strict/no-unused, ESLint, Rust Clippy `-D warnings`, explicit import/command/method/dependency scans; component layer = Vitest with Tauri/clipboard/theme substitutions; desktop layer = repository/service tests with temporary SQLite/filesystem and fake engines; integration layer = rebuilt real DuckDB sidecar and golden restart workflow; release layer = clean-XDG portable/AppImage launch, DEB content, checksums, memory/cache harness. The production graph is unchanged; only repositories, process binaries, filesystem roots, and WebView APIs are substituted.

VERDICT: MATCHES after cleanup. Every removed node had no path from any production, compatibility, or release root; all direct/full gates, release memory limits, and Linux package smokes remain green. Failure handling stayed separated at candidate classification and gate joins: uncertain candidates were retained, never guessed away. The cleanup gate is technically complete, but E12-T1 remains blocked until the separate E11.5 manual desktop review is approved.

## Baseline at cleanup start

Baseline revision: `b8ee52fd1709699e3e2122afdc1fe0fdce574316`.

| Measure                             |                          Before cleanup |
| ----------------------------------- | --------------------------------------: |
| Tracked files                       |                                     236 |
| Tracked bytes                       |                               2,518,357 |
| Frontend TypeScript/TSX/CSS         | 73 files / 14,665 lines / 448,020 bytes |
| Rust source                         | 45 files / 16,940 lines / 574,957 bytes |
| Metadata migrations                 |       7 files / 125 lines / 4,838 bytes |
| Scripts                             |      7 files / 830 lines / 32,787 bytes |
| Direct npm runtime/dev dependencies |                                 21 / 16 |
| Cargo workspace packages            |                                       5 |
| Release desktop binary              |                        12,415,832 bytes |
| Release sidecar binary              |                         6,223,320 bytes |
| Bundled `libduckdb.so`              |                        70,529,912 bytes |
| Portable tar.gz                     |                        30,804,329 bytes |
| DEB                                 |                        31,597,960 bytes |
| AppImage                            |                       145,517,048 bytes |

Machine-readable baseline: `target/e12/cleanup-before.json` (generated evidence, intentionally not tracked). Package checksums are verified by `scripts/release-linux.sh` and, after generation, with `(cd target/release-artifacts && sha256sum -c SHA256SUMS)`.

Baseline gates passed: Rust formatting and Clippy, rebuilt sidecar handshake, complete Rust workspace twice after isolating one non-reproducing initial `tarik --lib` harness failure, frontend format/docs/lint/typecheck, 12 command tests, 151 UI tests, production frontend build, Linux portable/DEB/AppImage content and launch smoke, and release checksums. Lint reports zero errors and six known warnings. The non-reproducing initial Rust result is retained as an observation; every cleanup boundary must rerun its affected test layer directly.

## Production roots and required reachability edges

| Root                   | Concrete entry/registration                                              | Reachability that must be preserved                                                                                                        |
| ---------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------ |
| Frontend               | `index.html` → `src/main.tsx` → `App`                                    | 39 runtime modules reached through direct and `components/ui` barrel imports; dialogs/workspaces are state-mounted, not route pages        |
| Desktop                | `src-tauri/src/main.rs` → `tarik_lib::run`                               | setup-managed logger, metadata, cleanup, engine, project/query/export/result/shutdown services; window lifecycle                           |
| Tauri IPC              | `tauri::generate_handler!`                                               | exact pair with typed frontend invoke names; baseline 55/55, then 54/54 after removing the runtime-unreferenced `get_app_directories` pair |
| Sidecar                | `engines/duckdb/src/main.rs`                                             | newline JSON dispatch, session, source/catalog, validation/query/result/export and shutdown methods                                        |
| Engine process         | `EngineManager` / `EngineProcess`                                        | lazy spawn, handshake/protocol check, session ownership, bounded stderr, shutdown/drop                                                     |
| Metadata compatibility | `metadata::migrations::MIGRATIONS`                                       | all seven ordered `include_str!` migrations plus accepted persisted fields and enum spellings                                              |
| Build                  | workspace manifests, both `build.rs`, `.cargo/config.toml`               | Tauri generation, pinned downloaded DuckDB linkage, Linux `$ORIGIN`; platform implementation must be extended—not deleted—in E12           |
| Tauri capability       | `src-tauri/capabilities/default.json`                                    | `core:default` and `dialog:allow-open`; opener reveal remains backend-owned                                                                |
| Release                | `tauri.conf.json`, `tauri.release.conf.json`, `scripts/release-linux.sh` | icons/resources, transient `externalBin`, licenses/notices, portable/DEB/AppImage checks and recovery trap                                 |
| Operational scripts    | `build-engine`, `check-engine`, `dev-reset`, memory/docs/release helpers | documented development, sidecar, memory, documentation, and release workflows                                                              |
| Test fixtures          | plan manifest and JSON fixtures                                          | protocol/normalizer drift and semantic coverage; test-only does not mean crust when it protects production parsing                         |

There is no router and no hidden page registry. The import reconstruction found no unlinked production page/component after resolving the UI barrel; `src/vite-env.d.ts` is a TypeScript/Vite ambient declaration root, not a runtime module.

## Initial candidate classification

| Candidate                                                             | Initial classification                  | Evidence / required action                                                                                                                                                                                             |
| --------------------------------------------------------------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `@radix-ui/react-tabs`                                                | Remove                                  | No import in source, tests, or tooling; Results is the only bottom output and query analysis is not a Radix tab workspace                                                                                              |
| frontend `@tauri-apps/plugin-opener` npm package                      | Remove                                  | No frontend import; reveal operations use the Rust plugin through backend-owned commands. Retain Rust `tauri-plugin-opener` and `.plugin(...)` wiring                                                                  |
| `crates/arrow-page-format`                                            | Remove                                  | Placeholder `PageCursor`/`PageError` crate is referenced only by manifests and its own test; actual bounded Arrow writing/reading lives in DuckDB `pages.rs`, while JSON-safe decoded pages cross Tauri IPC            |
| protocol `PageInfo`                                                   | Remove with Arrow placeholder           | Referenced only by dead `PageCursor`; `ResultInfo` remains live wire state                                                                                                                                             |
| protocol `EngineManifest` and client `check_manifest`                 | Remove                                  | Only self-tested scaffolding; production performs a live handshake and protocol check before session open                                                                                                              |
| protocol `EngineFrame`                                                | Remove                                  | Only self-tested tagged envelope; production wire is untagged `RequestEnvelope` / `ResponseEnvelope` newline JSON                                                                                                      |
| client `build_request`                                                | Remove                                  | Only its own test uses it; `EngineProcess::request` constructs the actual request and ID                                                                                                                               |
| sidecar `engine.ping`                                                 | Remove                                  | No desktop, script, or test caller; handshake is the health check and docs must stop advertising ping                                                                                                                  |
| sidecar `duckdb.source.check_health` and `sources::check_link_health` | Remove                                  | No caller; project reopen health is implemented and persisted by the desktop project manager. Retain `SourceState` wire/persisted variants                                                                             |
| direct unused Rust dependencies                                       | Remove                                  | Protocol `thiserror`; client `serde`/`uuid`; desktop/engine `tarik-arrow-page-format`; engine `serde`, subject to direct compile/test after each manifest boundary                                                     |
| nested `src-tauri/Cargo.lock`                                         | Removed                                 | Stale lock from the pre-workspace crate; root `Cargo.lock` is the workspace/release lock. Cargo metadata invoked from `src-tauri` resolved the root workspace/target and did not recreate it                           |
| generated icon set and Tauri schemas                                  | Retain                                  | Tauri/platform convention and review/configuration roots; Windows icon variants become active in E12                                                                                                                   |
| all seven SQLite migrations                                           | Retain                                  | Forward-only persisted-data compatibility boundary; old migrations remain required for fresh installs/upgrades                                                                                                         |
| legacy optional preference fields / serde defaults                    | Retain                                  | Existing user databases/snapshots may contain them; compatibility is not measured by current UI imports                                                                                                                |
| Unix `cfg`, `$ORIGIN` linker setup, Linux package scripts             | Retain                                  | Active Linux production/release paths; E12 adds Windows handling without breaking Linux                                                                                                                                |
| `bacon.toml`, fast-build installer, Clang/mold wrapper                | Retain                                  | Explicit documented developer workflow; not shipped runtime code, but still linked operational tooling                                                                                                                 |
| source and plan fixtures                                              | Retain                                  | Golden, parser, compatibility, and semantic regression roots                                                                                                                                                           |
| `get_app_directories` Tauri/frontend pair                             | Remove                                  | Wrapper appears only in its own command test and is never called by runtime UI; Rust already owns/resolves paths internally. Remove both IPC ends and obsolete serialization test, retaining internal `AppDirectories` |
| uncertain CSS selectors or externally consumed public protocol shapes | Defer unless direct ownership is proved | Text absence alone is insufficient because classes are composed dynamically and protocol shapes cross process/version boundaries                                                                                       |

## Removal boundaries and evidence plan

1. **Frontend dependencies:** remove only the two unused npm packages, regenerate the lock, then run frontend format/lint/typecheck/tests/build and Tauri capability checks.
2. **Placeholder page crate:** remove its workspace membership/dependencies/files and `PageInfo`; run Cargo metadata/tree, full workspace Clippy/tests, real sidecar paging/golden tests, and update historical docs to state the implemented owner.
3. **Protocol/client scaffolding:** remove self-only manifest/frame/request helpers and dependencies; run protocol/client and full sidecar/desktop tests including handshake mismatch behavior.
4. **Dead sidecar methods:** remove only uncalled ping/health dispatch/function paths; preserve handshake/shutdown and desktop reopen health; run engine protocol, project missing-link/recovery, and golden restart tests.
5. **Repository/tooling artifacts:** remove the stale nested lock only after proving both root and `src-tauri` Cargo commands use the root lock; audit styles/assets/scripts and retain/defer anything without equivalent proof.
6. **Final contract:** rerun all E12-T0 direct gates, release memory/cache harness, and Linux packaging; record `cleanup-after.json` using the same measurement method and explain any package-size variance.

## Completed cleanup ledger

| Boundary commit | Proven removal                                                                                                          | Verification                                                                                       |
| --------------- | ----------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `3b0611d`       | Unused frontend Radix Tabs and frontend opener packages                                                                 | All frontend, docs, and production build gates; Rust opener retained for backend-owned reveal      |
| `f170ea2`       | Placeholder `arrow-page-format` workspace crate and `PageInfo` cursor shape                                             | Cargo metadata, full Clippy/workspace tests, real sidecar paging/golden tests                      |
| `1952a69`       | Bypassed `EngineFrame`, `EngineManifest`, manifest checker, duplicate request builder, unused direct crate dependencies | Live handshake/protocol check and newline envelope tests; full sidecar/desktop suite               |
| `3d0c7cc`       | Uncalled `engine.ping` and duplicate sidecar linked-health method                                                       | Real handshake, 44 sidecar tests, golden restart/missing-link recovery                             |
| `255c0b6`       | Stale member `src-tauri/Cargo.lock` and unused engine `serde` declaration                                               | Cargo from `src-tauri` resolved root workspace/target without recreating the lock; full Rust gates |
| `a93345b`       | Runtime-unreferenced `get_app_directories` frontend/Tauri pair and wire-only serialization                              | Exact 54/54 invoke parity; full Rust/frontend/docs suite                                           |
| `7377da9`       | Two orphan CSS selectors and unnecessary public TypeScript exports                                                      | Full frontend suite; deliberate test seams retained; lint warnings reduced from six to three       |
| `bb3cb3f`       | Three unlinked UI primitive files, their showcase-only test/styles, and Radix Tooltip                                   | Import graph plus full frontend suite; active inline feedback states retained                      |
| `a2607fb`       | Two `cfg(test)` repository wrappers duplicating production APIs                                                         | Tests rewritten through `upsert` and `apply_history_retention`; focused metadata and golden tests  |
| `7fca04f`       | Unreferenced Vite/Tauri starter SVGs copied verbatim into production `dist`                                             | Rebuilt `dist` contains only Tarik HTML and hashed app CSS/JS                                      |

No unlinked production page remains. Tarik has no route registry: all workspaces/dialogs are reachable from `App` state, and every runtime frontend module is rooted through direct or UI-barrel imports. All remaining npm runtime dependencies have direct production imports. The 22 remaining sidecar methods are either called by the desktop/scripts or are structural lifecycle methods (`engine.handshake`, `engine.shutdown`).

## Retained and deferred findings

- Retained all seven forward-only SQLite migrations, persisted enum spellings/defaults, and compatibility fields. Historical migrations are required for fresh and upgraded metadata stores even when later code does not mention each column directly.
- Retained Rust `tauri-plugin-opener`; backend-owned log/export reveal commands actively use it. Only the unused frontend npm package was removed.
- Retained all Tauri icon variants, generated schemas, capability files, both `build.rs` files, `src/vite-env.d.ts`, and `src-tauri/binaries/.gitkeep` because Tauri/Vite/platform/release conventions consume them. Windows icon variants become active in E12.
- Retained Linux `$ORIGIN`, Unix cleanup tests, AppImage/DEB scripts, Clang/mold development configuration, `bacon.toml`, and fast-build tooling because Linux remains a supported production/development platform.
- Retained CodeMirror, XYFlow, history-status, estimate-accuracy, button-tone, and engine-state CSS selectors that scanners cannot associate through plain static text because libraries or template interpolation generate the class names.
- Retained public Rust service/repository types used within the Tauri composition crate. `unreachable_pub` produced hundreds of expected false positives because the library exports only `run()` while Tauri macros and internal modules form the actual app; it was not used as deletion evidence.
- Retained test-only pure-function seams and plan/source fixtures when they exercise production parsing, SQL safety, lifecycle, or compatibility. A test-only consumer is not crust when the test protects active behavior.

## Final production evidence and size delta

Machine-readable generated evidence: `target/e12/cleanup-before.json` and `target/e12/cleanup-after.json` (not tracked). The before revision is `b8ee52f`; after measurements were taken from production code at `7fca04f` before this evidence-only documentation commit.

| Measure                         |                  Before |                   After |                 Delta |
| ------------------------------- | ----------------------: | ----------------------: | --------------------: |
| Tracked files                   |                     236 |                     229 |                    −7 |
| Tracked bytes                   |               2,518,357 |               2,384,851 |              −133,506 |
| Frontend TypeScript/TSX/CSS     | 73 files / 14,665 lines | 70 files / 14,457 lines | −3 files / −208 lines |
| Rust source                     | 45 files / 16,940 lines | 44 files / 16,721 lines |  −1 file / −219 lines |
| Direct npm runtime dependencies |                      21 |                      18 |                    −3 |
| Cargo workspace packages        |                       5 |                       4 |                    −1 |
| Release desktop binary          |            12,415,832 B |            12,410,072 B |              −5,760 B |
| Release sidecar binary          |             6,223,320 B |             6,222,744 B |                −576 B |
| Bundled `libduckdb.so`          |            70,529,912 B |            70,529,912 B |                   0 B |
| Portable tar.gz                 |            30,804,329 B |            30,796,528 B |              −7,801 B |
| DEB                             |            31,597,960 B |            31,590,804 B |              −7,156 B |
| AppImage                        |           145,517,048 B |           145,517,048 B |                   0 B |

The AppImage's unchanged byte count is expected: its NO_STRIP runtime/container payload and bundled DuckDB dominate these small source removals. Size is observational, not the acceptance criterion.

Final release memory workload: 111,056 KiB (108.5 MiB) peak sidecar RSS/HWM, 3,540 KiB post-cycle growth, zero residual result-cache bytes, and zero hidden export stages; all existing budgets pass. Linux portable, DEB, and AppImage content/clean-XDG launch smokes and `sha256sum -c` pass.

Final direct gates: Rust format; Clippy `-D warnings`; rebuilt real sidecar handshake; all workspace tests including 75 desktop and 44 sidecar tests; frontend format/docs/lint/typecheck; 12 typed-command tests; 150 UI tests across 25 files; production build; exact 54/54 Tauri invoke parity. Lint has zero errors and three reviewed warnings.

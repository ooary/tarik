# E11 quality, performance, packaging, and release review

**Status:** REVIEW — final publication blocked by deferred E6, E7, and E10 manual gates  
**Scope:** Real-sidecar golden workflow, release memory evidence, local security boundaries, Linux x86_64 packaging, user documentation, and ship evidence.

## Build under review

Source commit used for the clean artifact generation:

```text
fb0c71dcbba8b1047a4dd44c4775a74aad10fd37
```

Artifacts under `target/release-artifacts/`:

| Artifact                          |      Size | SHA-256                                                            |
| --------------------------------- | --------: | ------------------------------------------------------------------ |
| `Tarik_0.1.0_amd64.AppImage`      | 138.7 MiB | `4e317b89e5cb0ed85216ed82632285fe88ba8b163e05523918e3c7a00e998501` |
| `Tarik_0.1.0_amd64.deb`           |  30.1 MiB | `8e2c365c12e8187fb4062e6629b6c41fb68805f1bff13ab510bd8d630fa1f879` |
| `Tarik-0.1.0-linux-x86_64.tar.gz` |  29.4 MiB | `b1a25c0e05b08339597e7c2d2a70b53926527a662a2c1f5b16722d1cd5b956c3` |

These artifacts are unsigned. Verify with:

```bash
cd target/release-artifacts
sha256sum -c SHA256SUMS
```

## 1. Golden workflow

The automated golden test already drives production Rust services and the real DuckDB sidecar over an isolated root. For manual confirmation, use only [`../user/USER-GUIDE.md`](../user/USER-GUIDE.md):

1. Start from clean XDG data or a clean machine.
2. Create a managed project.
3. Import a CSV table and link a Parquet view.
4. Run a grouped join, browse multiple result pages, and verify column/value rendering.
5. Open Estimate and Actual Flow; compare immutable SQL and truthful metrics.
6. Save the SQL with folder/tags.
7. Export one-row CSV or Parquet parts and reveal output.
8. Cancel a long query and export, then run another query.
9. Close/restart Tarik; reopen the project and verify tabs, saved SQL, history, catalog, and export files.
10. Move the linked Parquet file, reopen, verify Missing state, and repair it.

Acceptance requires a beginner to complete this without developer help or hidden steps.

## 2. Memory and bounded resources

Review [`../performance/E11-MEMORY.md`](../performance/E11-MEMORY.md) and the checked report.

Latest release-candidate fixed workload (`fb0c71d` source):

- peak sidecar RSS: **105.0 MiB** (budget: 512 MiB);
- retained RSS growth after 12 run/page/release cycles: **1.4 MiB** (budget: 64 MiB);
- result cache after release: **0 bytes**;
- hidden export stages after cancellation: **0**.

1. Run `npm run benchmark:memory -- --report target/e11/review-memory.json` twice.
2. Confirm both verdicts pass and every report includes machine/tool/dataset metadata.
3. Confirm query/result/export cycles do not accumulate directories under the harness root.
4. During a release desktop session, inspect Tauri/WebKit and sidecar RSS separately; do not combine them into an unexplained number.
5. Confirm current defaults remain: 500 rows/~4 MiB sidecar pages, 12 decoded desktop pages, one FIFO query and export worker per session, 4 MiB Parquet row groups.

## 3. Security and filesystem boundaries

Review [`../security/E11-BOUNDARY-REVIEW.md`](../security/E11-BOUNDARY-REVIEW.md).

1. Confirm `src-tauri/capabilities/default.json` has only `core:default` and `dialog:allow-open`; there is no broad opener permission.
2. Confirm the non-null CSP allows local application/IPC/assets and inline geometry styles but no remote/inline scripts or arbitrary network origins.
3. Confirm all 55 registered handlers have a typed frontend counterpart and no unregistered invoke exists (E11.5 adds the reviewed `create_table` pair).
4. In Settings, **Reveal logs** opens only Tarik's resolved log file; no path crosses WebView IPC.
5. **Reveal output** works for a completed tracked export. Forge export ID/part number in DevTools and confirm Rust rejects it; no arbitrary path parameter exists.
6. Re-run managed/external project deletion and cleanup outside-sentinel/symlink cases. User-owned files must survive.
7. Re-run quoted/path traversal/malformed manifest/collision cases from the boundary matrix.
8. Confirm validation remains EXPLAIN-only while Run, Actual Flow, and Export remain the only explicit user-SQL execution actions.

## 4. Linux packages

### Portable tarball

1. Extract `Tarik-0.1.0-linux-x86_64.tar.gz` into an empty directory.
2. Confirm it includes `tarik`, `tarik-engine-duckdb`, `libduckdb.so`, README, LICENSE, compatibility, notices, and dependency inventories.
3. Run `./tarik-engine-duckdb` with an engine handshake or use `scripts/check-engine.sh` logic. It must resolve sibling `libduckdb.so` through `$ORIGIN`.
4. Run `./tarik`; create/open/query/export under clean XDG directories.

### AppImage

1. On a second glibc-compatible Linux x86_64 machine with WebKitGTK/GTK runtime support:

   ```bash
   chmod +x Tarik_0.1.0_amd64.AppImage
   ./Tarik_0.1.0_amd64.AppImage
   ```

2. Complete the short golden workflow and restart.
3. Confirm the bundled sidecar starts when a project opens and no system DuckDB installation is required.

### DEB

1. Install `Tarik_0.1.0_amd64.deb` on a clean supported Debian/Ubuntu VM.
2. Confirm package contents include `/usr/bin/tarik`, `/usr/bin/tarik-engine-duckdb`, `/usr/bin/libduckdb.so`, compatibility, and notices.
3. Run create/open/query/export/restart smoke.
4. Remove the package and confirm XDG data, external DuckDB/source files, and exports are preserved.

Windows is not part of E11; it remains E12.

## 5. Upgrade, backup, notices, and documentation

1. Confirm `package.json`, `src-tauri/Cargo.toml`, and Tauri config all report `0.1.0`.
2. Confirm `release-manifest.json` records exact Git revision, metadata schema 7, protocol 1, DuckDB 1.5.5, unsigned state, runtime requirements, artifact hashes, and deferred release gates.
3. Follow [`../release/COMPATIBILITY.md`](../release/COMPATIBILITY.md) to back up metadata and managed/external projects; verify no text implies live SQLite copying is safe.
4. Confirm MIT `LICENSE`, DuckDB notice, and generated Rust/npm inventories are present.
5. Walk through README and user guide; every button/state label must match the release build.
6. Confirm docs distinguish:
   - CSV text/inference versus typed columnar Parquet;
   - import/copy/table versus link/reference/view;
   - Run/Results versus non-executing Estimate versus executing Actual Flow;
   - result paging versus one-pass export;
   - saved queries versus history versus drafts;
   - logs/cache versus durable project/user files.
7. Confirm limitations clearly disclose unsigned artifacts, Linux-only E11 packaging, runtime-only SQL errors, mutation execution, missing linked files, ephemeral result pages, and deferred reviews.

## Automated evidence recorded

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Real sidecar rebuild/handshake
- `cargo test --workspace --no-fail-fast`: desktop includes 74 tests with one real restart golden workflow; all protocol/sidecar/fixture suites pass
- `npm run format:check`
- `npm run docs:check`: 26 Markdown files, local links/npm commands/version/topics/deferred gates pass
- `npm run lint`: 0 errors; four pre-existing warnings
- `npm run typecheck`
- `npm test`: 12 typed command tests
- `npm run test:ui`: 136 tests across 22 files
- `npm run build`
- release sidecar benchmark: 105.0 MiB peak, 1.4 MiB growth, zero residual result/stage bytes; budget passes
- `npm run release:linux`: DEB/AppImage/portable build, target-triple staging, RPATH, sidecar handshake, DEB content, clean-XDG portable/AppImage smoke, SHA-256 verification, manifest generation pass

## E11 sign-off

- [ ] Golden workflow is understandable and passes manually from the user guide
- [ ] Restart preserves project/catalog/draft/saved/history/export state without rerunning SQL
- [ ] Missing link and query/export cancellation recovery approved
- [ ] Memory report is repeatable and budgets/defaults approved
- [ ] Invoke/CSP/path/identifier/cleanup security boundaries approved
- [ ] Portable tarball works on a clean extraction
- [ ] AppImage works on a second compatible Linux x86_64 machine
- [ ] DEB install/use/remove works on a clean Debian/Ubuntu VM and preserves data
- [ ] Checksums, unsigned state, version/compatibility, licenses/notices approved
- [ ] Beginner user guide and known limitations approved

## Deferred blockers before final publication

- [ ] E6 final result review approved
- [ ] E7 final query-flow review approved
- [ ] E10 diagnostics/recovery review approved
- [ ] Final artifacts regenerated from the fully approved/tagged commit

Reply **“E11 reviewed”** only after E11-specific items pass. Do not approve or publish Tarik 0.1.0 until the deferred blockers also pass.

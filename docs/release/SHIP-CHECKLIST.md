# Tarik 0.1.0 Linux MVP ship checklist

**Current verdict: NOT READY FOR FINAL RELEASE**  
E11 implementation can enter REVIEW, but E6 final result review, E7 final query-flow review, and E10 diagnostics/recovery review remain deferred and unsigned.

## 1. Source and versions

- [x] `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` are version `0.1.0`.
- [x] Rust is pinned to 1.91.0 and Node uses `.node-version`.
- [x] DuckDB Rust binding is pinned to `1.10505.0` / DuckDB runtime 1.5.5.
- [x] Engine protocol is version 1; metadata schema is version 8.
- [x] Working tree is clean before final artifact generation.

## 2. Automated gates

Run directly and check each exit code; do not pipe gates through `tail`.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/build-engine.sh
./scripts/check-engine.sh
cargo test --workspace --no-fail-fast
npm run format:check
npm run lint
npm run typecheck
npm test
npm run test:ui
npm run build
```

- [x] Real-sidecar golden restart workflow passes.
- [x] Mutation-sentinel validation tests pass.
- [x] Exact export boundary/cancel/collision/recovery tests pass.
- [x] Cleanup outside-sentinel/symlink tests pass.
- [x] Invoke handler/frontend command parity is exact.
- [x] Final full-gate run recorded against source commit `fb0c71d` (artifact source; review docs follow).

## 3. Memory evidence

```bash
CARGO_BUILD_PROFILE=release ./scripts/build-engine.sh
npm run benchmark:memory -- --record
```

- [x] Report records machine, toolchain, DuckDB, dataset, defaults, and samples.
- [x] Fixed-workload sidecar peak remains below 512 MiB.
- [x] Twelve query-release cycles retain less than 64 MiB.
- [x] Result cache is zero bytes after release.
- [x] Export cancellation leaves no hidden stage.
- [x] Release-candidate report passed at 105.0 MiB peak / 1.4 MiB post-cycle growth with zero residual result/stage bytes.

## 4. Security and filesystem boundaries

- [x] `docs/security/E11-BOUNDARY-REVIEW.md` has no unresolved high finding.
- [x] Tauri capability is local/main-window only with native open dialog and no broad opener permission.
- [x] CSP disallows remote/inline scripts and arbitrary network origins.
- [x] Log/export reveal paths are backend-owned.
- [x] External project forget preserves the user file.
- [x] Cleanup cannot traverse outside owned cache roots or follow symlinks.
- [x] Export recovery matches exact manifest UUID/part/name and preserves canonical files.
- [ ] Re-audit after any command, capability, CSP, path, or packaging change.

## 5. Artifacts

```bash
npm run release:linux
```

Expected under `target/release-artifacts/`:

- `Tarik_0.1.0_amd64.AppImage`
- `Tarik_0.1.0_amd64.deb`
- `Tarik-0.1.0-linux-x86_64.tar.gz`
- `SHA256SUMS`
- `release-manifest.json`
- generated Rust/npm dependency inventories

- [x] Sidecar and `libduckdb.so` are bundled under stable sibling names.
- [x] Sidecar `$ORIGIN` resolution and protocol handshake pass.
- [x] DEB contains desktop, sidecar, library, compatibility, and notices.
- [x] Portable desktop and AppImage launch under clean XDG directories.
- [x] All three SHA-256 values verify.
- [x] Artifacts explicitly state `signed: false`.
- [ ] Install DEB on a clean supported Debian/Ubuntu VM; create/open/query/export/uninstall smoke.
- [ ] Run AppImage on a second glibc-compatible Linux x86_64 machine.
- [ ] Confirm package removal preserves XDG data and external files.

## 6. Documentation walkthrough

- [x] README points to user, compatibility, performance, security, and release docs.
- [x] User guide covers CSV vs Parquet, import vs link, tables/views, joins, bounded Results, Estimate vs Actual Flow, saved SQL/history, exact export, logs/cache, data locations, backups, and limitations.
- [x] Compatibility guide states forward-only SQLite migration and DuckDB/source backup behavior.
- [x] Platform/signing limitations are explicit.
- [ ] A new user completes New project → Import CSV → Link Parquet → joined Run → Estimate → Actual Flow → Save → Export → restart → reopen from only the user guide.
- [ ] Every visible label in the walkthrough matches the release build.

## 7. Deferred manual gates — release blockers

These cannot be inferred from automated tests or E11 completion.

- [ ] **E6 final review:** bounded result browsing, rendering, resizing, paging, cancellation, and catalog refresh accepted.
- [ ] **E7 final review:** Estimate/Actual Flow shared workspace, truthful interpreted graph, immutable SQL snapshots, SQL mapping, and measured metrics accepted.
- [ ] **E10 review:** log redaction/rotation, incident surfaces, cleanup/export recovery, draft-first shutdown, forced reopen, and no-data-loss behavior accepted.
- [ ] E11 combined release review accepted.

## 8. Final publication

Only after every item above is checked:

- [ ] Regenerate artifacts from the exact tagged commit.
- [ ] Verify `SHA256SUMS` after upload/download.
- [ ] Tag `v0.1.0` and publish release notes with known limitations.
- [ ] Retain the release manifest, checksums, source commit, and memory evidence.
- [ ] Mark E11 approved and final release accepted in `TASK.md`.

Do not call the build released while any deferred manual gate remains open.

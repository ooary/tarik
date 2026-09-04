# Tarik

Tarik is a local-first desktop SQL workbench for beginner data engineers. It opens local DuckDB projects, imports CSV/Parquet into tables, links Parquet as views, runs cancellable SQL with bounded result browsing, explains estimated and actual query flows, saves SQL, keeps local history, and streams exact-row CSV/Parquet export parts.

No external service, account, or credential store is required. SQL and analytical data remain local to files you choose; operational metadata is stored in local SQLite.

> **Release status:** E11 is in progress. Linux x86_64 artifacts can be built and smoke-tested, but final release acceptance remains blocked by deferred E6, E7, and E10 manual reviews. Windows portable support is E12.

## Start here

- Beginner setup and full workflow: [`docs/user/USER-GUIDE.md`](docs/user/USER-GUIDE.md)
- Upgrade and backup contract: [`docs/release/COMPATIBILITY.md`](docs/release/COMPATIBILITY.md)
- Release checklist: [`docs/release/SHIP-CHECKLIST.md`](docs/release/SHIP-CHECKLIST.md)
- Delivery status and manual gates: [`TASK.md`](TASK.md)

## Development

Requirements: Node from `.node-version` and Rust 1.91.0. Linux additionally needs the Tauri/WebKitGTK development libraries and Clang/mold; Python 3 is used by release tooling. Windows uses the native MSVC Rust toolchain and the WebView2 runtime included with supported Windows 10/11 systems.

```bash
npm ci
npm run engine:build
npm run engine:check
npm run tauri dev
```

These npm commands run natively on Linux and Windows without Git Bash or WSL. The existing shell wrappers delegate to the same Node tooling for Linux documentation and release compatibility.

The desktop does not link DuckDB or Arrow. All analytical work runs in the long-lived `tarik-engine-duckdb` sidecar, which links pinned DuckDB 1.5.5 (`libduckdb.so` on Linux, `duckdb.dll` on Windows). Build the sidecar before desktop development and after removing `target/debug`.

If another development process owns port 1420:

```bash
npm run tauri:dev:clean
```

Do not use `cargo clean` as a routine blank-screen fix. See [`docs/development/FAST-RUST-BUILDS.md`](docs/development/FAST-RUST-BUILDS.md).

## Verification

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
npm run engine:build
npm run engine:check
cargo test --workspace --no-fail-fast
npm run format:check
npm run lint
npm run typecheck
npm test
npm run test:ui
npm run build
```

The Rust workspace includes a real-sidecar golden workflow covering project creation, CSV import, Parquet link, joined result paging, Estimate/Actual Flow, saved SQL, export, cancellation, full service restart, missing-link recovery, and session/history restore.

## Memory evidence

```bash
CARGO_BUILD_PROFILE=release ./scripts/build-engine.sh
npm run benchmark:memory -- --record
```

See [`docs/performance/E11-MEMORY.md`](docs/performance/E11-MEMORY.md) for fixed workloads, machine metadata, recorded release measurements, bounded defaults, and regression budgets.

## Linux release artifacts

```bash
npm run release:linux
```

This builds and verifies Linux x86_64 DEB, AppImage, and portable tar artifacts under `target/release-artifacts/`, including the desktop, DuckDB sidecar, `libduckdb.so`, MIT/third-party notices, compatibility guidance, SHA-256 checksums, and a release manifest. Artifacts are currently unsigned. Windows packaging is not part of E11.

## Local data locations (Linux)

```text
~/.local/share/com.tarik.desktop/tarik.sqlite   operational metadata and editor drafts
~/.local/share/com.tarik.desktop/projects/      Tarik-managed DuckDB projects
~/.local/share/com.tarik.desktop/logs/          bounded diagnostic JSONL logs
~/.cache/com.tarik.desktop/                     ephemeral results and recovery manifests
```

External DuckDB, CSV, Parquet, and completed export files remain where you choose. Tarik does not move or delete an external DuckDB file when you forget its project registration.

## License

Tarik is available under the [MIT License](LICENSE). See [third-party notices](THIRD_PARTY_NOTICES.md).

# Tarik third-party notices

Tarik is distributed under the MIT License. It includes or links open-source dependencies under their respective licenses.

## DuckDB

Tarik's analytical sidecar links DuckDB 1.5.5. DuckDB is available under the MIT License.

- Project: https://duckdb.org/
- Source: https://github.com/duckdb/duckdb
- License: https://github.com/duckdb/duckdb/blob/main/LICENSE

Copyright (c) 2018-2026 Stichting DuckDB Foundation.

## Tauri and Rust dependencies

The desktop uses Tauri 2, its dialog/opener plugins, rusqlite/SQLite, Arrow, Parquet, and other Rust crates recorded exactly in `Cargo.lock`. Their SPDX expressions and source metadata are generated into `target/release-artifacts/THIRD-PARTY-RUST.txt` by `scripts/release-linux.sh` from Cargo metadata.

## React and JavaScript dependencies

The WebView uses React, CodeMirror, Radix primitives, TanStack Virtual, XYFlow, Phosphor Icons, Tauri JavaScript bindings, and transitive packages recorded exactly in `package-lock.json`. Package versions and declared licenses are generated into `target/release-artifacts/THIRD-PARTY-NPM.txt` by `scripts/release-linux.sh`.

The generated inventories are notices, not replacements for upstream license texts. Source distributions and downstream repackagers should preserve the applicable license files supplied by each dependency.

#!/usr/bin/env bash
set -euo pipefail

# Build the DuckDB engine adapter. It links the official prebuilt libduckdb
# (DUCKDB_DOWNLOAD_LIB=1 is pinned in .cargo/config.toml), so no bundled C++
# compilation happens and the desktop app does not need to rebuild for this.

cargo build -p tarik-engine-duckdb

echo
echo "Engine built: target/debug/tarik-engine-duckdb"
echo "Run the desktop with: npm run tauri dev"

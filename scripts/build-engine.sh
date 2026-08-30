#!/usr/bin/env bash
set -euo pipefail

# Build the DuckDB engine adapter and place libduckdb next to the binary so it
# can find its runtime library via $ORIGIN without LD_LIBRARY_PATH.

cargo build -p tarik-engine-duckdb

PROFILE="${CARGO_BUILD_PROFILE:-debug}"
BIN_DIR="target/$PROFILE"

# Locate the prebuilt dynamic library downloaded by libduckdb-sys.
LIB=$(find target/duckdb-download -name 'libduckdb.so' -o -name 'duckdb.dll' 2>/dev/null | head -n 1 || true)
if [[ -n "$LIB" && -f "$LIB" ]]; then
  cp "$LIB" "$BIN_DIR/"
  echo "Copied $(basename "$LIB") -> $BIN_DIR/"
else
  echo "Warning: prebuilt libduckdb not found under target/duckdb-download; build may need DUCKDB_DOWNLOAD_LIB=1." >&2
fi

echo
echo "Engine built: $BIN_DIR/tarik-engine-duckdb"
echo "Run the desktop with: npm run tauri dev"

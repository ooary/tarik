#!/usr/bin/env bash
set -euo pipefail

# Spawn the engine exactly like the desktop app does and verify the handshake.
# Prints a clear reason if the engine binary or its DuckDB runtime library is
# missing or cannot load.

BIN="target/debug/tarik-engine-duckdb"

if [[ ! -x "$BIN" ]]; then
  echo "Engine binary not found: $BIN"
  echo "Build it with: ./scripts/build-engine.sh"
  exit 1
fi

if ! ldd "$BIN" 2>/dev/null | grep -q libduckdb; then
  echo "Engine binary does not link libduckdb."
  exit 1
fi

if ! ldd "$BIN" 2>/dev/null | grep -q 'libduckdb.so =>' ; then
  echo "libduckdb.so is not resolvable at runtime."
  echo "Expected it beside the binary (target/debug/libduckdb.so)."
  echo "Fix with: ./scripts/build-engine.sh"
  exit 1
fi

printf '%s\n' '{"id":"r1","method":"engine.handshake","params":{}}' | "$BIN" | python3 -c 'import sys,json
d=json.loads(sys.stdin.readline())
if d.get("ok"):
    print("Engine OK:", d["result"]["engineId"], d["result"]["engineVersion"], "protocol", d["result"]["protocolVersion"])
else:
    print("Engine failed:", d.get("error")); sys.exit(1)'

echo "Ready. Run the app with: npm run tauri dev"

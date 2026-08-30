#!/usr/bin/env bash
set -euo pipefail

PROJECT_ROOT="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"

patterns=(
  "^npm run tauri dev$"
  "^node ${PROJECT_ROOT}/node_modules/.bin/tauri dev$"
  "^node ${PROJECT_ROOT}/node_modules/.bin/vite$"
  "^${PROJECT_ROOT}/src-tauri/target/debug/tarik$"
  "^target/debug/tarik$"
  "^target/debug/tarik-engine-duckdb$"
)

for pattern in "${patterns[@]}"; do
  while IFS= read -r pid; do
    [[ -n "${pid}" ]] || continue
    kill "${pid}" 2>/dev/null || true
  done < <(pgrep -f "${pattern}" || true)
done

for _ in {1..20}; do
  if ! ss -ltn 2>/dev/null | grep -qE '[:.]1420[[:space:]]'; then
    exit 0
  fi
  sleep 0.1
done

printf 'Tarik dev port 1420 is still occupied by another process.\n' >&2
exit 1

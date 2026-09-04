#!/usr/bin/env bash
set -euo pipefail

# Linux/release compatibility wrapper. Native Windows development invokes the
# same Node implementation through `npm run engine:check`.
exec node "$(dirname "$0")/check-engine.mjs"

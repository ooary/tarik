#!/usr/bin/env bash
set -euo pipefail

# Linux/release compatibility wrapper. Native Windows development invokes the
# same Node implementation through `npm run engine:build`.
exec node "$(dirname "$0")/build-engine.mjs"

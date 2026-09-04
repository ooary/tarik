#!/usr/bin/env bash
set -euo pipefail

# Compatibility wrapper for existing Linux documentation/bookmarks.
exec node "$(dirname "$0")/dev-reset.mjs"

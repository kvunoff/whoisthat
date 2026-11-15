#!/usr/bin/env bash
# Fails when a state write is deliberately discarded (plan 2026-11-07, item 1).
#
# Usage: check-swallowed-errors.sh [dir]   (default: core/core)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="${1:-$ROOT/core/core}"

if grep -rn --include='*.go' --exclude='*_test.go' \
    -e '_ = proxy_manager.ReloadRouting' \
    -e '_ = cmd.DB.SetActiveRoutingProfile' \
    -e '_ = database.SetActiveRoutingProfile' \
    -e '_ = cmd.DB.SaveRoutingProfile' \
    -e '_ = database.SaveRoutingProfile' \
    -e '_ = db.updateGroup' \
    -e '_ = db.saveDBConfig' \
    -e '_ = os.Chmod(' \
    -e '_ = os.WriteFile(' \
    "$TARGET"; then
    echo 'error: swallowed state error (plan 2026-11-07, item 1)' >&2
    exit 1
fi

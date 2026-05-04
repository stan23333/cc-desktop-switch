#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

ARGS=(package macos --skip-pkg)
if [[ $# -ge 1 && -n "${1:-}" ]]; then
  ARGS+=(--version "$1")
elif [[ -n "${CCDS_VERSION:-}" ]]; then
  ARGS+=(--version "$CCDS_VERSION")
fi
if [[ $# -ge 2 && -n "${2:-}" ]]; then
  ARGS+=(--app "$2")
fi
if [[ $# -ge 3 && -n "${3:-}" ]]; then
  ARGS+=(--dmg "$3")
fi

cargo run -p xtask -- "${ARGS[@]}"

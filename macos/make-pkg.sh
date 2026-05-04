#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_BIN="${NODE_BIN:-node}"
detect_version() {
  "$NODE_BIN" -e 'const fs = require("fs"); const pkg = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); if (!pkg.version) throw new Error("version not found in package.json"); console.log(pkg.version);' "$ROOT/package.json"
}

VERSION="${1:-${CCDS_VERSION:-$(detect_version)}}"
APP_PATH="${2:-dist/mac/CC Desktop Switch.app}"
OUTPUT_PKG="${3:-dist/mac/CC-Desktop-Switch-v${VERSION}-macOS.pkg}"

if [[ ! -d "$APP_PATH" ]]; then
  echo "App bundle not found: $APP_PATH" >&2
  exit 1
fi

if ! command -v pkgbuild >/dev/null 2>&1; then
  echo "pkgbuild is required to create a macOS installer package." >&2
  exit 1
fi

PKG_ROOT="$ROOT/.tmp/pkg-root"
SCRIPTS_DIR="$ROOT/macos/pkg-scripts"
APP_NAME="$(basename "$APP_PATH")"

rm -rf "$PKG_ROOT"
mkdir -p "$PKG_ROOT/Applications"
mkdir -p "$(dirname "$OUTPUT_PKG")"

ditto "$APP_PATH" "$PKG_ROOT/Applications/$APP_NAME"

rm -f "$OUTPUT_PKG"
pkgbuild \
  --root "$PKG_ROOT" \
  --install-location "/" \
  --identifier "io.github.lonr6.ccdesktopswitch" \
  --version "$VERSION" \
  --scripts "$SCRIPTS_DIR" \
  "$OUTPUT_PKG"

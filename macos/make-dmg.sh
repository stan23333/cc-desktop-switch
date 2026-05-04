#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_BIN="${NODE_BIN:-node}"
detect_version() {
  "$NODE_BIN" -e 'const fs = require("fs"); const pkg = JSON.parse(fs.readFileSync(process.argv[1], "utf8")); if (!pkg.version) throw new Error("version not found in package.json"); console.log(pkg.version);' "$ROOT/package.json"
}

VERSION="${1:-${CCDS_VERSION:-$(detect_version)}}"
SOURCE_PATH="${2:-dist/mac/CC Desktop Switch.app}"
OUTPUT_DMG="${3:-dist/mac/CC-Desktop-Switch-v${VERSION}-macOS.dmg}"

if [[ ! -e "$SOURCE_PATH" ]]; then
  echo "DMG source not found: $SOURCE_PATH" >&2
  exit 1
fi

if ! command -v hdiutil >/dev/null 2>&1; then
  echo "hdiutil is required to create a DMG." >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT_DMG")"
STAGING="$(mktemp -d)"
cleanup() {
  rm -rf "$STAGING"
}
trap cleanup EXIT

cp -R "$SOURCE_PATH" "$STAGING/"
if [[ "$SOURCE_PATH" == *.app ]]; then
  ln -s /Applications "$STAGING/Applications"
fi

rm -f "$OUTPUT_DMG"
hdiutil create \
  -volname "CC Desktop Switch ${VERSION}" \
  -srcfolder "$STAGING" \
  -ov \
  -format UDZO \
  "$OUTPUT_DMG"

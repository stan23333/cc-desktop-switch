# UI Layout Cleanup After Backport

## Goal

Clean the layout after the `codex-app-transfer` visual backport by comparing code-level structure with the source project instead of guessing from the screenshot.

## Status

Completed.

## Findings From Code Comparison

- `codex-app-transfer/frontend/index.html` uses `.header-actions-left` as the first direct child of `.app-header`; it does not keep a brand block in the header.
- Current `codex-app-transfer/frontend/index.html` keeps only the clear-config button in the header left action area, so the CC Switch import shortcut should not stay in that crowded header slot.
- `codex-app-transfer/frontend/index.html` does not render `global-health-banner`.
- `codex-app-transfer/frontend/index.html` removes the dashboard `switch-board-head`; the dashboard starts with the provider cards.
- Current `cc-desktop-switch` kept all three structures, which creates the crowded header and extra dashboard title region shown in the screenshot.

## Checklist

- [x] Align header DOM with `codex-app-transfer` while preserving CC Desktop Switch clear-config wording and action.
- [x] Remove the dashboard title block that the source visual scheme does not use.
- [x] Remove the full-width global health banner and rely on the page-level desktop warning used by the source visual scheme.
- [x] Validate syntax and rebuild a local `.app` for testing.

## Validation Results

- `node --check frontend/js/api.js`
- `node --check frontend/js/app.js`
- `node --check frontend/js/i18n.js`
- `git diff --check`
- `python3 -m compileall -q backend main.py tests macos/build-macos.spec macos/prepare-icon.py`
- `MACOS_SKIP_DMG=1 PYTHON_BIN=.tmp/test-venv/bin/python ./macos/build-macos.sh`
- `codesign --verify --deep --strict --verbose=2 "dist/mac/CC Desktop Switch.app"`

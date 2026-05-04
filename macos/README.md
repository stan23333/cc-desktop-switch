# macOS Packaging

This directory owns only the macOS installer wrapper layer for CC Desktop Switch.

As of v1.1.0, the macOS application bundle is built by Tauri. The Python/PyInstaller macOS app-build path has been removed. The remaining scripts package an existing Tauri `.app` bundle into the release artifacts:

- `make-pkg.sh`
- `make-dmg.sh`

Do not copy shared runtime code into this directory. Runtime behavior now lives in `src-tauri/`, `frontend/`, and the Tauri bridge; this directory should stay limited to macOS packaging, signing, notarization, and installer layout.

## Local Build

Run from the repository root on macOS.

First build the Tauri app bundle:

```bash
env PATH=/Users/alysechen/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/bin:$PATH pnpm tauri build --bundles app --no-sign
mkdir -p dist/mac
ditto src-tauri/target/release/bundle/macos/CC\ Desktop\ Switch.app dist/mac/CC\ Desktop\ Switch.app
```

Then create the installer package and drag-and-drop disk image:

```bash
./macos/make-pkg.sh 1.1.0 "dist/mac/CC Desktop Switch.app" "dist/mac/CC-Desktop-Switch-v1.1.0-macOS-arm64.pkg"
./macos/make-dmg.sh 1.1.0 "dist/mac/CC Desktop Switch.app" "dist/mac/CC-Desktop-Switch-v1.1.0-macOS-arm64.dmg"
```

All macOS release outputs stay under:

```text
dist/mac/
```

Expected outputs:

- `CC Desktop Switch.app`
- `CC-Desktop-Switch-v<version>-macOS-arm64.pkg`
- `CC-Desktop-Switch-v<version>-macOS-arm64.dmg`

The DMG keeps the standard drag-and-drop layout: the `.app` bundle plus an Applications shortcut. The PKG installer installs to `/Applications/CC Desktop Switch.app` and removes the previous app at that official install location before writing the new one.

## Update Assets

The in-app updater uses platform keys such as `macos-arm64` and `macos-x64` from `latest.json`. For macOS install actions, the app prefers a `.pkg` asset and falls back to a `.dmg` asset.

Only alter `latest.json` when the release signing private key is available. If the key is unavailable, upload macOS installer artifacts plus `.sha256` sidecars without changing `latest.json` or `latest.json.sig`.

## Runtime Behavior

The macOS runtime is the Tauri app bundle. Closing the window hides it, and clicking the tray/Dock entry restores the main window.

For Anthropic-compatible providers, the app writes the selected provider URL, API key, auth scheme, extra gateway headers, and model list directly into Claude Desktop's macOS 3P configuration. After applying the configuration and fully restarting Claude Desktop, Claude Desktop can keep using that provider even if CC Desktop Switch is quit.

The local proxy is still used for experimental OpenAI, new-api, and reverse-proxy style providers that need request conversion. In that mode, keep CC Desktop Switch running; closing the window only hides the app so the proxy can continue serving Claude Desktop through `127.0.0.1:<proxyPort>`.

## Optional Signing

For local unsigned testing, no environment variables are required.

For Developer ID signing, signing should be applied to the Tauri `.app` bundle before creating PKG/DMG artifacts.

For installer package signing, set:

```bash
export MACOS_INSTALLER_SIGN_IDENTITY="Developer ID Installer: Your Name (TEAMID)"
```

Then sign the generated PKG with `productsign` before upload.

## Manual Smoke Test

After building the Tauri app:

```bash
./dist/mac/CC\ Desktop\ Switch.app/Contents/MacOS/cc-desktop-switch
```

Then verify:

- The desktop window opens with URL `tauri://localhost#dashboard`.
- Provider and preset icons render from `./assets/providers/...`.
- Applying an Anthropic-compatible provider writes the macOS plist, root Claude-3p JSON config, and active `configLibrary` entry.
- Anthropic-compatible providers report direct-provider mode and do not require the proxy after Claude Desktop restarts.
- Experimental OpenAI/new-api providers start the proxy on port `18080` and keep working when the window is hidden.

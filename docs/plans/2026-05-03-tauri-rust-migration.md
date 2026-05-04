# Tauri Rust Migration Implementation Plan

> For future agents: implement this plan only after the user explicitly approves the migration. Do not commit or push unless the user explicitly asks for commits in the current conversation.

**Goal:** Move CC Desktop Switch from the former Python + pywebview desktop runtime to a faster, smaller, more native Tauri application while preserving the existing user-facing behavior and release compatibility.

**Architecture:** Use Tauri v2 as the desktop shell. Move local config, Claude Desktop integration, update handling, single-instance behavior, provider management, and local forwarding into Rust modules. Rebuild the frontend as TypeScript with a component framework, while keeping the current UI workflow and terminology intact.

**Tech Stack:** Tauri v2, Rust, Tokio, Axum or Tauri commands, reqwest, serde, serde_json, React or Solid with TypeScript, Vite, NSIS or Tauri Bundler on Windows, Tauri Bundler plus existing PKG/DMG scripts on macOS.

---

## Current Code Shape

- Retired Python runtime entry and desktop shell: `main.py`.
- Retired FastAPI management API and static frontend server: `backend/main.py`.
- Retired Python provider config, desktop integration, local proxy, API adapter, provider-tool, CC-Switch import, and update modules under `backend/`.
- Frontend: `frontend/index.html`, `frontend/js/app.js`, `frontend/js/tauri-api.js`, `frontend/js/i18n.js`, `frontend/css/style.css`.
- Current packaging: Windows release artifacts are built from Tauri/Rust output through `.github/workflows/release.yml` and `cargo run -p xtask -- release windows`; `windows/build.bat` is only a local helper. macOS release packaging builds the `.app` through Tauri and wraps it through `cargo run -p xtask -- package macos`, with `macos/make-pkg.sh` / `macos/make-dmg.sh` retained as thin wrappers. The pre-Tauri macOS Python/PyInstaller app-build path was removed after v1.1.0 parity.
- Tests: Rust unit tests under `src-tauri/src/` plus `cargo run -p xtask -- frontend check-contracts` for frontend bridge and release-chain static contracts.

## Recommended Target

The recommended target is Tauri + Rust backend + TypeScript frontend.

Do not aim for "Rust only" in the first migration. A pure Rust UI through Dioxus, Leptos, Slint, or egui would unify the language more aggressively, but it would make the UI migration higher risk and would slow down visual iteration. This project already has a browser-style UI, many stateful forms, routing, i18n, dark mode, and provider workflows. TypeScript is the practical language for that surface.

The final steady state should be:

- Rust owns runtime logic, platform integration, local proxy, update flow, config storage, and packaging entrypoints.
- TypeScript owns the visual interface and state management.
- JSON schemas shared through generated TypeScript types keep the boundary strict.
- Python has been removed from the shipping runtime after parity was reached. Small one-off maintenance scripts can stay only if they are not part of app startup or packaging.

## Migration Strategy

### Phase 1: Create a parallel Tauri app without changing the shipping Python app

**Files:**
- Create: `package.json`
- Create: `vite.config.ts`
- Create: `tsconfig.json`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/state.rs`
- Create: `src/App.tsx`
- Create: `src/main.tsx`
- Create: `src/styles.css`
- Keep: existing `main.py`, `backend/`, and `frontend/` until Python runtime retirement is explicitly started.

**Steps:**
1. Scaffold Tauri v2 in parallel so existing releases can continue from the Python app.
2. Copy the current UI text and screen structure into `src/` without redesigning workflows.
3. Add a `tauri:dev` command for local validation.
4. Do not remove the Python runtime in this phase.

**Validation:**
- `npm run tauri dev` opens the new app shell.
- Existing Python validation still passes.

### Phase 2: Port the data model and config storage to Rust

**Files:**
- Create: `src-tauri/src/config.rs`
- Create: `src-tauri/src/models.rs`
- Create: `src-tauri/src/model_alias.rs`
- Test: `src-tauri/src/config_tests.rs`

**Steps:**
1. Define `Provider`, `Settings`, `ModelMapping`, and `AppConfig` with `serde`.
2. Preserve the current config file path: `~/.cc-desktop-switch/config.json`.
3. Preserve import, backup, export, provider ordering, saved secret retention, and built-in preset behavior from `backend/config.py`.
4. Port model alias fallback behavior from `backend/model_alias.py`.
5. Write Rust unit tests matching the existing Python tests before deleting any Python logic.

**Validation:**
- `cargo test config model_alias`
- A config created by the Python version can be read by the Rust version without manual migration.

### Phase 3: Replace the FastAPI management API with Tauri commands

**Files:**
- Create: `src-tauri/src/commands/providers.rs`
- Create: `src-tauri/src/commands/settings.rs`
- Create: `src-tauri/src/commands/desktop.rs`
- Create: `src-tauri/src/commands/update.rs`
- Modify: `src/App.tsx`
- Modify: frontend API client under `src/lib/api.ts`

**Steps:**
1. Convert `/api/status`, `/api/providers`, `/api/settings`, `/api/desktop/*`, `/api/update/*`, and `/api/config/*` into Tauri commands.
2. Keep command payloads compatible with the current frontend response shape where reasonable.
3. Remove the local admin HTTP server from the normal desktop path.
4. Keep an optional local HTTP compatibility layer only for Claude Desktop proxy traffic, not for UI rendering.

**Validation:**
- Provider add, edit, delete, reorder, default selection, backup, import, export, and desktop status all work from the Tauri UI.
- There is no `127.0.0.1:18081` dependency for normal UI startup.

### Phase 4: Port Claude Desktop platform integration

**Files:**
- Create: `src-tauri/src/desktop/windows.rs`
- Create: `src-tauri/src/desktop/macos.rs`
- Create: `src-tauri/src/desktop/mod.rs`
- Test: `src-tauri/src/desktop/tests.rs`

**Steps:**
1. Port Windows HKCU policy reads and writes from `backend/registry.py`.
2. Port macOS plist, JSON config, and configLibrary writes from `backend/registry.py`.
3. Preserve masking rules so API keys and headers are never returned to the frontend in plaintext except explicit export or secret-read flows.
4. Preserve direct-provider behavior for Anthropic-compatible providers and proxy behavior for OpenAI / new-api experimental providers.
5. Preserve the Claude Desktop restart helper.

**Validation:**
- Windows: current-user config writes without admin for normal HKCU paths.
- macOS: plist, JSON config, and configLibrary write/readback behavior match current Python implementation.
- Existing user configs still show the correct active provider after migration.

### Phase 5: Port the local proxy and streaming adapter

**Files:**
- Create: `src-tauri/src/proxy/mod.rs`
- Create: `src-tauri/src/proxy/adapters.rs`
- Create: `src-tauri/src/proxy/logs.rs`
- Create: `src-tauri/src/proxy/routes.rs`
- Test: `src-tauri/src/proxy/tests.rs`

**Steps:**
1. Use Axum or Hyper on `127.0.0.1:<proxyPort>` for `/v1/models`, `/claude/v1/models`, `/v1/messages`, and `/claude/v1/messages`.
2. Port Anthropic-to-OpenAI conversion from `backend/api_adapters.py`.
3. Port model mapping, upstream headers, upstream HTTP proxy support, error normalization, usage normalization, and SSE forwarding.
4. Keep log buffer and proxy stats available through Tauri commands.
5. Keep the proxy optional for stable direct Anthropic-compatible providers.

**Validation:**
- Non-streaming Anthropic-compatible requests work.
- Streaming SSE requests work.
- OpenAI Chat experimental conversion still returns Anthropic-shaped responses.
- Proxy auth rejects missing or wrong gateway API keys.

### Phase 6: Replace packaging and update metadata

**Files:**
- Modify: `.github/workflows/release.yml` if present in the target branch.
- Replace: `scripts/New-Release.ps1` with `cargo run -p xtask -- release windows`
- Removed: hand-written `windows/installer.nsi`, because Windows NSIS output is now owned by Tauri Bundler.
- Remove after parity: pre-Tauri macOS Python/PyInstaller app-build path.
- Replace orchestration in `macos/make-pkg.sh` and `macos/make-dmg.sh` with thin `xtask` wrappers.
- Modify: `release/latest.json` generation flow if present locally or in CI.

**Steps:**
1. Let Tauri Bundler own the Windows NSIS output.
2. Preserve current release asset names so existing release documentation and update checks do not break.
3. Keep Windows current-user installation behavior and shortcuts.
4. Keep macOS PKG and DMG assets, and keep `latest.json` platform keys compatible with the current updater.
5. Do not commit generated `dist/`, `.dmg`, `.pkg`, `.exe`, or `.zip` artifacts.

**Validation:**
- Windows setup installer installs under `%LOCALAPPDATA%\Programs\CC-Desktop-Switch`.
- macOS app launches as `CC Desktop Switch.app`.
- In-app update selects the intended installer asset.

### Phase 7: Retire Python only after parity

**Files:**
- Removed after Windows compatibility path replacement: `main.py`
- Removed after Windows compatibility path replacement: `backend/`
- Removed after Windows release moved to Tauri artifacts: `windows/build.spec`
- Removed after Windows compatibility path replacement: `requirements.txt`
- Removed after parity: `macos/build-macos.sh`, `macos/build-macos.spec`, `macos/prepare-icon.py`, and `macos/entitlements.plist`
- Replaced tests: `tests/test_provider_config_and_proxy.py` with Rust tests and `cargo run -p xtask -- frontend check-contracts`
- Update: `README.md`
- Update: `docs/USAGE.md`
- Update: `docs/QUICK_START.md`
- Update local-only project-management notes when useful.

**Steps:**
1. Keep Python release path until Rust app passes parity checks on both Windows and macOS.
2. Update documentation only after the new build pipeline is actually verified.
3. Remove Python runtime files in a dedicated cleanup phase.
4. Keep a rollback tag or branch that can still build the last Python release.

**Validation:**
- Full Rust unit tests pass.
- Frontend typecheck and build pass.
- Manual smoke tests pass on Windows and macOS.
- Update, provider import/export, direct provider apply, proxy mode, Claude restart, and single-instance behavior all match current user-visible behavior.

## Acceptance Checklist

- App startup is faster than the PyInstaller build on the same machine.
- Installed app size is materially smaller than the Python bundle.
- No duplicate instance opens multiple backend processes.
- Normal UI no longer depends on a local admin HTTP port.
- Existing `~/.cc-desktop-switch/config.json` remains compatible.
- Direct Anthropic-compatible provider mode still works after the app exits.
- Experimental proxy mode still works when required.
- Windows installation remains current-user and shortcut-friendly.
- macOS packaging still produces a working app and installer assets.
- Release notes and README describe the new stack only after the new release path is verified.

## Main Risks

- SSE streaming behavior is easy to regress during the proxy rewrite.
- macOS Claude Desktop config paths have changed before; keep readback tests and manual verification.
- Tauri command security is simpler than local HTTP, but proxy endpoints still need strict local-only binding and gateway auth.
- A visual rewrite can drift away from the existing app. Reuse the current workflows and copy before polishing the component structure.
- A single-language Rust UI would reduce language count but increase migration risk and visual iteration cost.

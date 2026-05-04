# Language Unification Active Task

## Goal

Unify non-runtime project tooling around Rust while preserving the existing Tauri WebView product UI.

## Current Status

- Active phase: P5 in progress.
- Completed phases: P1, P2, P3, P4.
- Execution adjustment: added a root Cargo workspace for `xtask` only and explicitly excluded `src-tauri`. This keeps `src-tauri` as its existing standalone Tauri package so the migration does not change the runtime lockfile ownership during tooling cleanup.

## Stable Task Tree

- [x] P1. Move build and static validation tooling from JavaScript to Rust `xtask`.
  - [x] P1.1. Add an `xtask` Cargo package and workspace wiring.
  - [x] P1.2. Port `scripts/build-app-bundle.mjs` to `xtask frontend bundle-app`.
  - [x] P1.3. Port `scripts/build-style-bundle.mjs` to `xtask frontend bundle-style`.
  - [x] P1.4. Port `scripts/check-static-contracts.mjs` to `xtask frontend check-contracts`.
  - [x] P1.5. Update `package.json` so `pnpm build` and `pnpm check:static` call the Rust `xtask` commands.
  - [x] P1.6. Remove the retired JavaScript build/check scripts after parity validation.
- [x] P2. Move release and packaging orchestration from platform scripts to Rust `xtask`.
  - [x] P2.1. Port release asset assembly and `latest.json` generation from `scripts/New-Release.ps1` to `xtask release windows`.
  - [x] P2.2. Port release signature verification from `scripts/Test-ReleaseSignature.ps1` to `xtask release verify`.
  - [x] P2.3. Keep optional Windows Authenticode signing as a platform call, but make Rust own the release flow around it.
  - [x] P2.4. Port macOS PKG/DMG orchestration from `macos/make-pkg.sh` and `macos/make-dmg.sh` to `xtask package macos`.
  - [x] P2.5. Remove or reduce `windows/build.bat`, macOS shell scripts, and PowerShell scripts to thin wrappers only if users still need double-click/manual entrypoints.
  - [x] P2.6. Update `.github/workflows/release.yml` to call Rust `xtask` instead of the PowerShell release script.
- [x] P3. Make Rust the source of truth for shared frontend/runtime contracts.
  - [x] P3.1. Move provider/model slot metadata that exists in both Rust and frontend JavaScript into Rust-owned generated JSON.
  - [x] P3.2. Generate frontend-readable constants during `xtask frontend build` without changing the rendered UI.
  - [x] P3.3. Keep `frontend/js/tauri-api.js` as the small Tauri bridge, but reduce duplicated method lists by validating against Rust command metadata or a Rust-maintained contract file.
  - [x] P3.4. Move non-DOM business helpers from `frontend/js/app/*.js` into Rust commands only when that reduces duplication and does not make UI interactions slower.
- [x] P4. Reassess the frontend language boundary after tooling is Rust-owned.
  - [x] P4.1. Re-measure editable language composition.
  - [x] P4.2. If JavaScript remains mostly DOM wiring, keep it as the Web UI layer.
  - [x] P4.3. If JavaScript business logic is still large, create a side-by-side prototype for moving one workflow's logic to Rust commands.
  - [x] P4.4. Consider TypeScript only as a type-safety decision, not as a language-unification decision.
  - [x] P4.5. Consider pure Rust UI only after an explicitly approved prototype preserves the current UI and workflows.
- [ ] P5. Update documentation and release validation.
  - [x] P5.1. Update `agent/operations.md` with verified `xtask` commands.
  - [x] P5.2. Update `docs/plans/2026-05-03-tauri-rust-migration.md` or replace its completed sections with the new Rust-tooling direction.
  - [x] P5.3. Update README build instructions only after the old scripts are actually removed.
  - [ ] P5.4. Run full validation on macOS locally and Windows through GitHub Actions before treating the language cleanup as complete.

## Execution Record

- 2026-05-05: Started P1. Read global/project instructions, the language unification plan, project rules, technical notes, operations notes, and current plan.
- 2026-05-05: Added root `Cargo.toml` with `xtask` as the only workspace member and `src-tauri` excluded to preserve the Tauri runtime package boundary.
- 2026-05-05: Added Rust `xtask` commands for `frontend bundle-app`, `frontend bundle-style`, `frontend build`, and `frontend check-contracts`.
- 2026-05-05: Updated `package.json` so `pnpm build` and `pnpm check:static` call Rust `xtask`.
- 2026-05-05: Removed retired JavaScript scripts: `scripts/build-app-bundle.mjs`, `scripts/build-style-bundle.mjs`, `scripts/build-tauri-frontend.mjs`, and `scripts/check-static-contracts.mjs`.
- 2026-05-05: Updated concise project notes for the new Rust frontend tooling entrypoints and marked P5.1 complete because the verified `xtask` commands are now recorded.
- 2026-05-05: Completed P2 implementation. Added Rust `xtask` commands for Windows release assembly, `latest.json` generation, release asset signing, signature verification, and macOS PKG/DMG orchestration.
- 2026-05-05: Kept Windows Authenticode signing as a platform call through `scripts/Invoke-CodeSigning.ps1`, while Rust now owns the release flow around it.
- 2026-05-05: Updated `.github/workflows/release.yml` to call `cargo run -p xtask -- release windows` and `cargo run -p xtask -- release verify`.
- 2026-05-05: Reduced `macos/make-pkg.sh` and `macos/make-dmg.sh` to thin `xtask` wrappers; updated `windows/build.bat` release mode to call `xtask`.
- 2026-05-05: Removed retired PowerShell release scripts: `scripts/New-Release.ps1` and `scripts/Test-ReleaseSignature.ps1`.
- 2026-05-05: Updated README, macOS packaging README, migration plan, technical notes, and operations notes for the Rust-owned release/package tooling. Marked P5.2 and P5.3 complete.
- 2026-05-05: Completed P3 implementation. Added Rust-owned contract generation in `xtask/src/contracts.rs` and generated `frontend/js/generated/contracts.js` plus `src-tauri/src/generated/model_contracts.rs`.
- 2026-05-05: Moved shared model metadata, provider form model slots, default mapping rows, legacy model-slot candidates, and Claude model ID to slot mapping out of handwritten frontend/runtime duplicates.
- 2026-05-05: Updated `frontend/index.html` to load generated contracts before `tauri-api.js` and `app.js`; updated `frontend/js/app/00-state.js` and regenerated `frontend/js/app.js` so the UI reads contract data without changing the rendered layout.
- 2026-05-05: Updated `frontend/js/tauri-api.js` so model mapping normalization uses generated slot and legacy-candidate contracts instead of a second hardcoded mapping table.
- 2026-05-05: Updated `src-tauri/src/model_alias.rs` to use generated Rust constants while preserving legacy `sonnet` / `opus` / `haiku` fallback behavior.
- 2026-05-05: Extended `xtask frontend check-contracts` so it validates the Rust-maintained `CCApi` method contract and checks Tauri commands called by `frontend/js/tauri-api.js` against `tauri::generate_handler!` registration in `src-tauri/src/lib.rs`.
- 2026-05-05: Reviewed remaining non-DOM-looking helpers in `frontend/js/app/*.js`. The remaining functions are primarily synchronous UI state, DOM rendering, form/menu state, Bootstrap modal/toast wiring, drag sorting, and immediate feedback helpers. Moving them behind Rust commands would add asynchronous WebView-to-Rust round trips without reducing meaningful duplication, so P3.4 is complete with no additional migration.
- 2026-05-05: Completed P4 measurement after P3. Active editable source, excluding generated files, lockfiles, binary assets, vendor files, `agent/`, `codex_history/`, `dist/`, `target/`, and the unrelated user-added `docs/copilot-api-caozhiyuan/`, is now 54 files / 19,736 lines: Rust 26 files / 11,752 lines / 59.5%; JavaScript 9 files / 3,492 lines / 17.7%; CSS 7 files / 3,479 lines / 17.6%; HTML 1 file / 544 lines / 2.8%; YAML 1 file / 131 lines / 0.7%; Shell 2 files / 42 lines / 0.2%; Batch 1 file / 90 lines / 0.5%; PowerShell 1 file / 70 lines / 0.4%; JSON/TOML/config 6 files / 136 lines / 0.7%.
- 2026-05-05: Reviewed remaining JavaScript distribution. `frontend/js/app/*.js` and `frontend/js/i18n.js` are dominated by DOM rendering, route rendering, form/menu state, Bootstrap modal/toast behavior, drag-and-drop ordering, local UI storage, feedback file handling, and event binding. `frontend/js/tauri-api.js` is a 411-line bridge that adapts UI payloads to Rust Tauri commands.
- 2026-05-05: P4 decision: keep plain JavaScript as the bounded WebView UI layer. Do not create a Rust-command prototype now because no large standalone JavaScript business workflow remains; provider diagnostics, model discovery, config import/export, updates, proxying, feedback submission, Desktop integration, and local proxy detection are already Rust-owned.
- 2026-05-05: P4 TypeScript decision: do not add TypeScript for language unification. It would add a compile-time frontend language and dependency surface while the current bridge and generated-contract checks already cover the active contract risk.
- 2026-05-05: P4 pure Rust UI decision: do not pursue egui, Slint, Dioxus, Leptos, or Yew without a separately approved prototype. A pure Rust UI would reduce language count on paper but would risk another UI drift from the preserved WebView interface.
- 2026-05-05: Started P5.4 validation. Ran local static checks, syntax checks, Rust formatting, `xtask` build/test, Tauri Rust build/test, frontend build, and macOS unsigned `.app` bundle build.
- 2026-05-05: Confirmed macOS unsigned `.app` build output exists at `src-tauri/target/release/bundle/macos/CC Desktop Switch.app`.
- 2026-05-05: Windows GitHub Actions validation is blocked until the current local changes are committed and pushed. The current `.github/workflows/release.yml`, root Cargo workspace, `xtask`, generated contract files, and package command changes are still local, so a GitHub Actions run on the remote branch would not validate this worktree. Per commit policy, no commit or push was performed without an explicit user request.

## Validation Results

- `cargo fmt --check`: passed.
- `pnpm check:static`: passed.
- `pnpm build`: passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed.
- `cargo test --manifest-path src-tauri/Cargo.toml`: failed in sandbox because tests bind local loopback ports; passed outside the sandbox with 62 tests.
- `git diff --check`: passed.
- `cargo check -p xtask`: passed after P2 implementation.
- `cargo test -p xtask`: passed; includes release signing and verification round-trip coverage.
- `bash -n macos/make-pkg.sh macos/make-dmg.sh`: passed.
- `pnpm check:static`: passed after P2 implementation.
- `pnpm build`: passed after P2 implementation.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed after P2 implementation.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed after P2 implementation.
- `cargo test --manifest-path src-tauri/Cargo.toml`: failed in sandbox because tests bind local loopback ports; passed outside the sandbox with 62 tests after P2 implementation.
- `git diff --check`: passed after P2 implementation.
- `cargo fmt -p xtask`: passed after P3 implementation.
- `cargo fmt --check`: passed after P3 implementation.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed after P3 implementation.
- `node --check frontend/js/tauri-api.js`: passed after P3 implementation.
- `node --check frontend/js/generated/contracts.js`: passed after P3 implementation.
- `node --check frontend/js/app.js`: passed after P3 implementation.
- `node --check frontend/js/i18n.js`: passed after P3 implementation.
- `cargo check -p xtask`: passed after P3 implementation.
- `cargo test -p xtask`: passed after P3 implementation.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed after P3 implementation.
- `pnpm build`: passed after P3 implementation.
- `cargo test --manifest-path src-tauri/Cargo.toml`: failed in sandbox because tests bind local loopback ports; passed outside the sandbox with 62 tests after P3 implementation.
- `pnpm check:static`: passed after P3 implementation; includes generated contract freshness, `CCApi` method coverage, and Tauri command registration coverage.
- `git diff --check`: passed after P3 implementation.
- P4 language composition measurement: completed with hidden `.github/` included and unrelated `docs/copilot-api-caozhiyuan/` excluded.
- P4 JavaScript boundary scan: completed; no code migration prototype required.
- `git diff --check`: passed after P4 documentation updates.
- `pnpm check:static`: passed during P5.4 local validation.
- `node --check frontend/js/app.js`: passed during P5.4 local validation.
- `node --check frontend/js/i18n.js`: passed during P5.4 local validation.
- `node --check frontend/js/tauri-api.js`: passed during P5.4 local validation.
- `cargo fmt --check`: passed during P5.4 local validation.
- `cargo check -p xtask`: passed during P5.4 local validation.
- `cargo test -p xtask`: passed during P5.4 local validation.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed during P5.4 local validation.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed during P5.4 local validation.
- `cargo test --manifest-path src-tauri/Cargo.toml`: passed outside the sandbox with 62 tests during P5.4 local validation; this test suite binds local loopback ports.
- `pnpm build`: passed during P5.4 local validation.
- `pnpm tauri build --bundles app --no-sign`: passed during P5.4 local macOS app validation.
- `test -d src-tauri/target/release/bundle/macos/CC\ Desktop\ Switch.app`: passed during P5.4 local macOS app validation.
- `git diff --check`: passed after P5.4 local validation and task-document update.

## Blockers

- Windows GitHub Actions validation cannot be completed against the current worktree until the local changes are committed and pushed. No commit or push has been performed because the current conversation has not explicitly requested it.

## Next Step

- To finish P5.4, commit and push the current source changes, then run or observe the Windows release workflow on GitHub Actions against that pushed commit.

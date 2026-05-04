# Language Unification Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Further unify CC Desktop Switch around Rust after Python removal, while preserving the existing Tauri WebView UI and avoiding another frontend redesign.

**Architecture:** Keep Rust as the application runtime, business logic, config, proxy, update, provider, feedback, and release-tooling language. Keep HTML/CSS/JavaScript only as the product UI layer loaded by the Tauri WebView. Move build, static checks, release assembly, signing verification, and platform packaging orchestration into a Rust `xtask` workspace instead of spreading that logic across JavaScript, PowerShell, Shell, and Batch.

**Tech Stack:** Rust 2024, Tauri 2, plain HTML/CSS/JavaScript frontend, Cargo workspace `xtask`, existing platform tools (`pkgbuild`, `hdiutil`, NSIS) invoked from Rust where needed.

---

## Current Language Baseline

Measured on 2026-05-05 after P3 from active editable source files, excluding generated `frontend/js/app.js`, generated `frontend/css/style.css`, generated contract outputs, lockfiles, binary assets, vendor bundles, `agent/`, `codex_history/`, `dist/`, `src-tauri/target/`, and the unrelated user-added `docs/copilot-api-caozhiyuan/` directory.

| Language | Files | Lines | Share |
| --- | ---: | ---: | ---: |
| Rust | 26 | 11,752 | 59.5% |
| JavaScript | 9 | 3,492 | 17.7% |
| CSS | 7 | 3,479 | 17.6% |
| HTML | 1 | 544 | 2.8% |
| YAML | 1 | 131 | 0.7% |
| Shell | 2 | 42 | 0.2% |
| Batch | 1 | 90 | 0.5% |
| PowerShell | 1 | 70 | 0.4% |
| JSON/TOML/other config | 6 | 136 | 0.7% |

Python is now absent from active tracked source.

P4 result: JavaScript remains a bounded WebView UI layer. The remaining JavaScript is concentrated in route rendering, DOM updates, form state, Bootstrap modal/toast behavior, drag-and-drop ordering, local UI storage, feedback attachment handling, localization, and the thin Tauri bridge. The application runtime and business workflows are already Rust-owned, so no side-by-side Rust-command prototype is needed for P4.

## Recommended Target

The best next target is **Rust-first with a bounded Web UI layer**, not a pure Rust UI rewrite.

Keep:

- Rust: runtime, commands, provider logic, proxy, update, config, diagnostics, feedback, import, release metadata, packaging orchestration, static contract checks.
- Web UI assets: existing `frontend/index.html`, `frontend/css/style/*.css`, `frontend/js/app/*.js`, `frontend/js/i18n.js`, and `frontend/js/tauri-api.js`.
- Declarative config only where required: `package.json`, `tauri.conf.json`, GitHub Actions YAML, Cargo manifests.

Do not pursue now:

- Pure Rust UI rewrite with egui, Slint, Dioxus, Leptos, or Yew. It would reduce language count on paper, but this app is form-heavy and WebView-shaped. The likely cost is UI drift and regression risk, and startup speed has already been addressed by the direct Tauri frontend path.
- TypeScript migration as the main unification step. It improves frontend type safety but adds a language rather than reducing language surface.
- React or another SPA framework. Project rules explicitly keep React out of the active product UI unless a redesign is approved.

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

## Expected Result

After P1 and P2, the remaining non-Rust executable logic should be limited to the browser UI itself and thin declarative CI/config files. The practical target is:

- Rust becomes the only runtime and tooling language.
- JavaScript remains only for DOM events, Bootstrap modal/toast integration, Tauri bridge calls, and immediate UI state.
- CSS and HTML remain because they are the actual visual product surface.
- PowerShell, Shell, and Batch are either removed or reduced to optional wrappers.
- YAML/JSON/TOML remain only as required declarative config.

This is the highest-value unification route because it improves maintainability without repeating the earlier UI drift problem.

## Validation Strategy

Run after each phase:

```bash
pnpm check:static
pnpm build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
git diff --check
```

Run after release-tooling changes:

```bash
pnpm tauri build --bundles app --no-sign
./dist/mac/CC\ Desktop\ Switch.app/Contents/MacOS/cc-desktop-switch
```

For Windows release parity, use GitHub Actions or a Windows environment to verify that Setup, Portable ZIP, Windows x64 EXE, `latest.json`, checksums, and release signatures are still generated and uploaded with the existing public asset names.

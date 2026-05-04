# Post 1.1 Cleanup Active Task

## Goal

Continue cleaning the codebase after the v1.1.0 Tauri/Rust migration by reducing legacy language/runtime surface while preserving release correctness and the existing frontend experience.

## Current Status

- Started: 2026-05-04.
- Trigger: post-cleanup code scan showed Rust is now the largest language, but Python still accounts for about one third of current self-authored source.
- Current focus: Priority 1, moving the Windows release chain from Python/PyInstaller artifacts to Tauri/Rust artifacts.
- Boundary: do not remove the Python runtime path until the release workflow no longer depends on it and parity-critical behavior remains covered by Rust/Tauri tests.

## Stable Task Tree

- [x] P1. Move Windows release packaging to Tauri/Rust artifacts.
  - [x] P1.1. Audit the existing Windows release workflow and packaging scripts.
  - [x] P1.2. Change the GitHub Release workflow to install Node/Rust/Tauri dependencies instead of Python packaging dependencies.
  - [x] P1.3. Change `scripts/New-Release.ps1` to build and collect Tauri Windows artifacts.
  - [x] P1.4. Preserve existing release asset names and `latest.json` update metadata shape.
  - [x] P1.5. Update tests and docs so the release chain no longer treats PyInstaller/NSIS scripts as the active Windows path.
  - [x] P1.6. Run focused local validation.
  - [x] P1.7. Verify the Windows Setup artifact on GitHub Actions or another Windows environment after the changes are committed and pushed.
- [ ] P2. Remove Python compatibility runtime after Windows release no longer depends on it.
  - [ ] P2.1. Confirm Rust/Tauri covers all runtime behavior still used by `backend/` and `main.py`.
  - [ ] P2.2. Remove `backend/`, `main.py`, `requirements.txt`, and Python-specific tests only after parity confirmation.
  - [ ] P2.3. Update docs, validation commands, and release notes to remove Python fallback language.
- [ ] P3. Split large source files without changing behavior.
  - [ ] P3.1. Split `src-tauri/src/proxy/mod.rs` by conversion, listener, streaming, and telemetry responsibilities.
  - [ ] P3.2. Split `frontend/js/app.js` into route/render/action modules only if the original UI contract stays intact.
  - [ ] P3.3. Split `frontend/css/style.css` by layout, components, and pages without visual redesign.
- [ ] P4. Split the large Python test file and clean repository management artifacts.
  - [ ] P4.1. Split `tests/test_provider_config_and_proxy.py` by behavior area while tests still exist.
  - [ ] P4.2. Decide whether tracked `codex_history/` archives should stay in the public repository.
  - [ ] P4.3. Keep only current, useful project-management notes in tracked documentation.

## Execution Record

- 2026-05-04: Created this active task document from the four cleanup priorities requested by the user. Priority 1 is the active implementation target.
- 2026-05-04: Audited `.github/workflows/release.yml`, `scripts/New-Release.ps1`, Tauri config, Cargo metadata, package scripts, and current Python packaging tests. Current release flow still installs Python requirements, runs PyInstaller through `windows/build.spec`, optionally runs `windows/installer.nsi`, and then generates the existing Windows Setup, Portable ZIP, Windows x64 EXE, signatures, checksums, and `latest.json`.
- 2026-05-04: Updated `.github/workflows/release.yml` to install Node/pnpm, Rust, and NSIS instead of Python packaging dependencies. Updated `scripts/New-Release.ps1` so `-Build -TryInstaller` runs Tauri builds, collects `src-tauri/target/release/cc-desktop-switch.exe`, collects the Tauri NSIS installer under `src-tauri/target/release/bundle/nsis`, and preserves the public release asset names for Windows Setup, Portable ZIP, Windows x64 EXE, and `latest.json`.
- 2026-05-04: Removed obsolete active Windows packaging inputs `windows/build.spec` and `windows/installer.nsi`. Rewrote `windows/build.bat` as a Tauri helper for local Windows exe, NSIS, and release asset generation. Updated static packaging tests and project docs to point at the Tauri Windows release chain.
- 2026-05-04: Local validation passed. Full Windows installer generation still needs a Windows environment or GitHub Actions run after these changes are committed and pushed.
- 2026-05-05: Completed P1.7. Committed the Tauri Windows packaging change as `cb8bfa2` and pushed it to `origin/main`. Triggered Release workflow run `#22` for `v1.1.0`; the Windows release job completed successfully at `https://github.com/lonr-6/cc-desktop-switch/actions/runs/25329471028`. The workflow uploaded refreshed Windows assets to the `v1.1.0` Release, including Setup, Portable ZIP, Windows x64 EXE, signatures, checksums, `latest.json`, and the public release key. Downloaded and parsed `latest.json`; `platforms.windows-x64.assets` contains `CC-Desktop-Switch-v1.1.0-Windows-Portable.zip`, `CC-Desktop-Switch-v1.1.0-Windows-x64.exe`, and `CC-Desktop-Switch-v1.1.0-Windows-Setup.exe`.

## Validation Results

- `python3 -m compileall -q backend main.py tests`: passed.
- Node syntax checks passed for `frontend/js/api.js`, `frontend/js/app.js`, `frontend/js/i18n.js`, and `src/originalApiBridge.js`.
- `pnpm build`: passed.
- `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`: passed.
- `cargo check --manifest-path src-tauri/Cargo.toml`: passed.
- `.tmp/test-venv/bin/python -m unittest discover -s tests -v`: passed, 110 tests.
- `git diff --check`: passed.
- GitHub Actions Release workflow run `#22`: passed for Windows release artifact generation and upload.
- Published `latest.json` metadata check: passed; `windows-x64` lists Portable ZIP, Windows x64 EXE, and Windows Setup.

## Remaining Work

- P1 is complete. Next priority is P2: remove the Python compatibility runtime only after confirming Rust/Tauri covers all behavior still used by `backend/` and `main.py`.

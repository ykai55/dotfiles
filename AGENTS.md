# AGENTS

This repository is a personal dotfiles collection. Most tooling lives in
standalone scripts under `bin/` (bash, Python, perl), with a few focused
subprojects such as `ha_helper/`, `rproxy/`, and `opencode/`. There is no
centralized build system or lint configuration.

## Build / Lint / Test

No global build step.

Lint
- No lint tooling configured.
- Keep shell and Python changes consistent with existing style.

Tests (Python)
- After any code change, run the relevant unittest(s).
- Run all `bin/` Python tests:
  `python -m unittest discover -s bin/tests -p 'test_*.py'`
- Run a single test:
  `python -m unittest bin.tests.test_tmux_load.TmuxLoadWindowRestoreTests.test_restore_new_session_creates_all_windows`
  (Use dotted module path with `python -m unittest`; avoid running test files
  directly as scripts since they may fail on module resolution.)

Tests (Rust)
- `ha_helper/` is an independent Rust crate. After Rust changes, run:
  `cargo test --manifest-path ha_helper/Cargo.toml`
- `rproxy/` is an independent Rust crate. After Rust changes, run:
  `cargo test --manifest-path rproxy/Cargo.toml`

Subdirectory instructions
- More specific `AGENTS.md` files exist under some subdirectories. Follow the
  nearest one for work in that subtree.

## Repository Layout

- `bin/`: executable scripts (bash, Python, perl). Most tooling lives here.
- `bin/tests/`: unittest coverage for scripts and modules.
- `tbox/`: Python package (`tbox.cli`, `tbox.core`). `bin/tbox` is a thin
  launcher that adds the repo root to `sys.path` and calls `tbox.cli.main()`.
  Tests import from `tbox.core` directly.
- `fish/`, `nvim/`, `tmux/`: shell/editor/terminal configs.
- `kitty.conf`, `gitconfig`: app configs.
- `dotfiles-map.json`: manifest mapping repo files to install targets;
  consumed by `bin/dotfiles-apply`.
- `downloads.json`, `downloads.schema.json`: downloaded binary/tool manifest
  and schema used by `bin/dotfiles-apply`.
- `opencode/`: opencode config, plugins, skills, and service/container helpers.
- `ha_helper/`: independent Rust crate for OpenWrt WiFi presence to MQTT.
- `rproxy/`: independent Rust crate for an HTTP/TCP reverse proxy CLI.
- `scripts/`: helper scripts for building static tools.
- `tmux-box.md`: tbox usage and workflow documentation.

## Code Style Guidelines

General
- Prefer minimal, direct changes; these scripts are small and focused.
- Keep changes ASCII-only unless the file already contains non-ASCII.
- Use descriptive variable names; avoid unnecessary abbreviations.
- Do not run broad content searches from large scopes such as the home directory
  or filesystem root by default. Filename searches are fine; content/string
  matching over those scopes requires either an explicit user request or prior
  user approval.

Python (tmux-dump, tmux-load)
- Shebang: `#!/usr/bin/env python3`.
- Keep `from __future__ import annotations` at the top.
- Imports: standard library only; group and order stdlib imports alphabetically.
- Typing: prefer type hints in function signatures; use `typing` aliases for
  complex structures (e.g., `Dict[str, Any]`).
- Formatting: 4-space indentation; blank line between top-level defs.
- Exceptions:
  - Use `RuntimeError` for operational failures.
  - Print user-facing errors to stderr and return non-zero exit codes in `main`.
  - Use `raise SystemExit(main())` guard in `__main__`.
- Subprocess:
  - Use `subprocess.run([...], stdout=PIPE, stderr=PIPE, text=True)`.
  - Keep helper wrappers (`run_tmux`, `tmux_out`) to centralize error handling.
- Data handling:
  - Normalize paths (strip `file://` and host prefixes) when consuming tmux output.
  - Preserve output schema; do not rename keys unless updating both scripts and
    tests.

Bash scripts
- Shebang: `#!/usr/bin/env bash` or `/bin/bash` (keep existing style).
- Use `set -euo pipefail` when starting new scripts unless compatibility
  requires otherwise.
- Quote variables; use `[[ ... ]]` for tests and `command -v` for detection.
- Functions are lower_snake_case; constants are UPPER_SNAKE_CASE.
- Prefer `exec` for thin wrapper scripts (`env-exec` pattern).

Perl script (`bin/vidir`)
- This is vendored; keep edits minimal and preserve upstream formatting and
  license comments.

Config files (fish, nvim, tmux, kitty)
- Match the file's existing conventions and indentation.
- Avoid reformatting unrelated blocks.

## Naming Conventions

- Executable scripts use kebab-case in `bin/` (e.g., `tmux-dump`).
- Functions: snake_case in Python and bash.
- Test classes: `CamelCase` with `Tests` suffix; test methods start with
  `test_`.

## Error Handling Expectations

- Prefer explicit error messages and non-zero exit codes.
- For tmux scripts, surface tmux command failures verbosely.
- Avoid silent failures; handle missing dependencies with clear messages.

## Testing Notes

- `bin/tests/test_tmux_load.py` uses `unittest` and `unittest.mock`.
- Keep tests fast and isolated; mock tmux calls instead of running tmux.
- `bin/tests/test_utils.py` provides `CapturingTestCase` base class (captures
  stdout/stderr during tests). Used by `test_tbox`, `test_tmux_load`,
  `test_dotfiles_apply`, and `test_apply_env`.
- `bin/tests/test_dotfiles_apply.py` loads the script via `importlib` because
  `dotfiles-apply` has no `.py` extension; tests must pass `-m unittest` style
  to resolve the `test_utils` import.
- `bin/tests/test_tbox_integration.py` runs `tbox` as a subprocess and
  requires the `tbox` package to be importable from the repo root.
- `bin/tests/test_apply_env.py` calls the `apply_env.fish` fish function via
  `subprocess` with `fish -N -c`.

## tmux Tools Notes

- `tmux-load` runs pane commands by default; use `--no-run-commands` to skip.
- Pane commands are derived from `processes` in the dump, not tmux start/current command fields.
- `tmux-dump` schema reference: `tmux-dump.d.ts`.
- When changing tbox commands, update `tmux-box.md`.

## clip Wrapper Notes

- `bin/clip` is a Bash wrapper around the downloaded `clip` binary under
  `bin/.downloads/clip/current/<platform>/`.
- Supported targets are macOS arm64 (`macos-aarch64`), Linux x86_64
  (`linux-x86_64-musl`), and Windows x86_64 (`windows-x86_64-gnu`).
- On macOS arm64, the wrapper also exports `CLIP_MACOS_HELPER` pointing at the
  downloaded `clip-macos-helper` executable.
- If the binary or helper is missing, the user-facing recovery is to run
  `bin/dotfiles-apply`.
- Keep `bin/clip` as a thin Bash wrapper: preserve `set -euo pipefail`, quote
  variables, and prefer explicit platform/architecture errors.

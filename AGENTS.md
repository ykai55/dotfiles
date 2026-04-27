# AGENTS

This repository is a personal dotfiles collection with a small set of scripts
in `bin/` (bash, Python, perl). There is no centralized build system or lint
configuration; most scripts are standalone.

## Build / Lint / Test

No global build step.

Lint
- No lint tooling configured.
- Keep shell and Python changes consistent with existing style.

Tests (Python)
- After any code change, run the relevant unittest(s).
- Run all main tests (one command):
  `python -m unittest bin/tests/test_tmux_load.py bin/tests/test_tmux_dump.py bin/tests/test_tbox.py bin/tests/test_tbox_integration.py`
- Additional test files (run separately):
  - `bin/tests/test_dotfiles_apply.py` — tests for `dotfiles-apply` script
  - `bin/tests/test_apply_env.py` — tests for fish `apply_env.fish` function
- Run a single test:
  `python -m unittest bin.tests.test_tmux_load.TmuxLoadWindowRestoreTests.test_restore_new_session_creates_all_windows`
  (Use dotted module path with `python -m unittest`; avoid running test files
  directly as scripts since they may fail on module resolution.)

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
- `tmux-box.md`: tbox usage and workflow documentation.


## Code Style Guidelines

General
- Prefer minimal, direct changes; these scripts are small and focused.
- Keep changes ASCII-only unless the file already contains non-ASCII.
- Use descriptive variable names; avoid unnecessary abbreviations.

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



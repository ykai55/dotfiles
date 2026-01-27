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
- Run all tests (unittest):
  - `python bin/tests/test_tmux_load.py`
  - `python bin/tests/test_tmux_dump.py`
- Run a single test (unittest):
  - `python -m unittest bin/tests/test_tmux_load.py TmuxLoadWindowRestoreTests.test_restore_new_session_creates_all_windows`
  - If that fails due to module path, run the file and temporarily narrow the
    test name inside `bin/tests/test_tmux_load.py`.

## Repository Layout

- `bin/`: executable scripts (bash, Python, perl). Most tooling lives here.
- `bin/tests/`: unittest coverage for `tmux-load` and `tmux-dump`.
- `fish/`, `nvim/`, `tmux/`: shell/editor/terminal configs.
- `kitty.conf`, `gitconfig`: app configs.

## Script Inventory (bin/)

tmux-dump
- Purpose: dump tmux topology as JSON to stdout.
- Usage: `tmux-dump > tmux.json`, `tmux-dump --pretty > tmux.pretty.json`,
  `tmux-dump --session name > tmux.json`.
- Behavior: when inside tmux, dumps current session; otherwise dumps attached
  session or first session.
- Session path: uses a temporary background window probe to capture
  `pane_current_path` for the session.
- Output: session -> windows -> panes -> processes.
- Notable fields: `name`, `windows[].name`, `windows[].panes[].path`,
  `windows[].panes[].start_command`, `windows[].panes[].current_command`,
  `windows[].panes[].processes[].command` (array of tokens).

tmux-load
- Purpose: restore tmux topology from a tmux-dump JSON file.
- Usage:
  - `tmux-load path/to/tmux.json`
  - `tmux-load --session name path/to/tmux.json`
  - `tmux-load -f path/to/tmux.json`
  - `tmux-load -a path/to/tmux.json`
  - `tmux-load --run-commands path/to/tmux.json`
- Behavior:
  - Restores windows, panes, titles, layouts, and working directories.
  - Default target is current tmux session; outside tmux uses dump session name
    and creates a unique new session if that name exists.
  - Use `-f` to clear target session, `-a` to append.
  - `--run-commands` executes pane `start_command`.
  - After restore, switches/attaches to the target session when not restoring
    in place.
- Input: supports single-session object or legacy `{ "sessions": [...] }`.
- Note: `start_command` may be string or list; list is joined into shell line.

Important cross-script note
- When changing tmux dump/load behavior, keep `tmux-dump` and `tmux-load`
  aligned and update tests in `bin/tests/test_tmux_load.py`.

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

## Cursor / Copilot Rules

- No `.cursor/rules/`, `.cursorrules`, or `.github/copilot-instructions.md`
  files were found in this repository at the time of writing.

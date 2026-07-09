# AGENTS

When making modifications, remember to update both scripts and the unit test simultaneously.

## Build / Lint / Test

There is no dedicated lint tooling for `bin/`. After Python changes, run the
relevant unittest(s), or all `bin/` Python tests with:
`python -m unittest discover -s bin/tests -p 'test_*.py'`.

Use dotted module paths for single tests, for example:
`python -m unittest bin.tests.test_tmux_load.TmuxLoadWindowRestoreTests.test_restore_new_session_creates_all_windows`.
Avoid running test files directly as scripts because imports may fail.

Keep tests fast and isolated. `bin/tests/test_utils.py` provides
`CapturingTestCase` for stdout/stderr capture, and tmux tests should mock tmux
calls instead of running tmux.

`bin/tests/test_dotfiles_apply.py` loads `dotfiles-apply` via `importlib`
because the script has no `.py` extension. `bin/tests/test_apply_env.py` calls
the `apply_env.fish` fish function via `subprocess` with `fish -N -c`.

## tmux-dump

Purpose
- Dump tmux topology as JSON to stdout.

Usage
- tmux-dump path/to/tmux.json
- tmux-dump --pretty path/to/tmux.pretty.json
- tmux-dump --session name path/to/tmux.json
- tmux-dump --session name > tmux.json

Behavior
- If running inside tmux, only the current session is dumped.
- If not in tmux, the attached session is dumped (or the first session if none are attached).
- Writes JSON to the provided output path.

Output structure (high level)
- session
  - windows: list of windows
    - panes: list of panes
      - processes: list of processes on the pane TTY

Important fields
- name
- windows[].name
- windows[].panes[].path
- windows[].panes[].processes[].command (array of tokens)

Notes
- processes[].command is an array of strings (tokenized via shell-like splitting).
- processes[].command reflects the command tokens as reported by ps.
- Schema reference: `tmux-dump.d.ts`.

## tmux-load

Purpose
- Restore tmux topology from a tmux-dump JSON file.

Usage
- tmux-load path/to/tmux.json
- tmux-load --session name path/to/tmux.json
- tmux-load -f path/to/tmux.json
- tmux-load -a path/to/tmux.json
- tmux-load -c /path/to/dir path/to/tmux.json
- tmux-load --no-run-commands path/to/tmux.json

Behavior
- Restores windows, panes, titles, layouts, and working directories into a target session.
- Default target is the current tmux session; outside tmux uses the dump session name.
- If not inside tmux and the dump has no session name, --session is required.
- If target session is not empty, use -f to clear or -a to append.
- -f clears the target session before restore.
- -a appends windows to the target session.
- -c sets the base directory for new sessions, overriding dump paths for the first window only.
- By default, pane commands are restored based on processes.
- --no-run-commands skips starting any pane commands.

Restore combinations
- Inside tmux + restoring into current session (in-place): does not change directory, -c is rejected.
- Inside tmux + restoring into a different session: creates/updates that session and switches client after restore.
- Outside tmux + target session derived from dump name: creates/updates that session and attaches after restore.
- Outside tmux + --session provided: creates/updates that session and attaches after restore.
- Outside tmux + dump has no session name: requires --session.

Input expectations
- Dump can be a single session object (current output) or legacy {"sessions": [...]}.

Notes
- Pane commands are derived from processes, preferring a direct tmux command when
  the first process is not a shell; when the first process is a shell and there is
  a second process, the second command is sent to the shell.
- -c cannot be used when restoring in the current session.
- Runs pane commands by default; use `--no-run-commands` to skip.

## clip

`bin/clip` is a thin Bash wrapper around the downloaded `clip` binary under
`bin/.downloads/clip/current/<platform>/`. Keep it as a wrapper: preserve
`set -euo pipefail`, quote variables, and prefer explicit platform/architecture
errors.

Supported targets are macOS arm64 (`macos-aarch64`), Linux x86_64
(`linux-x86_64-musl`), and Windows x86_64 (`windows-x86_64-gnu`). On macOS
arm64, the wrapper also exports `CLIP_MACOS_HELPER` pointing at the downloaded
`clip-macos-helper` executable.

If the binary or helper is missing, the user-facing recovery is to run
`bin/dotfiles-apply`.

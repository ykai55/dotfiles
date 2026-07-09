# AGENTS

This repository is a personal dotfiles collection. Most tooling lives in
standalone scripts under `bin/` (bash, Python, perl), with a few focused
subprojects such as `ha_helper/`, `rproxy/`, and `opencode/`. There is no
centralized build system or lint configuration.

## Subdirectory Instructions

- More specific `AGENTS.md` files exist under some subdirectories. Follow the
  nearest one for work in that subtree.

## dotfiles-apply

`bin/dotfiles-apply` applies mappings from `dotfiles-map.json` and downloads
tools or managed repositories from `downloads.json`. It is the user-facing
recovery path when linked configs, downloaded binaries, or `.managed/` content
are missing.

## Repository Layout

- `bin/`: executable scripts (bash, Python, perl). Most tooling lives here.
- `bin/tests/`: unittest coverage for scripts and modules.
- `.github/`: GitHub Actions workflows for releases and static tool builds.
- `.managed/`: generated or downloaded content managed by `bin/dotfiles-apply`.
- `clip/`: independent Rust workspace for the cross-platform clipboard CLI and
  history tools.
- `docs/`: design notes, plans, and other project documentation.
- `github-codebase-sync/`: Docker and shell helpers for syncing this repository
  into codebase-memory tooling.
- `tbox/`: Python package backing the `bin/tbox` tmux workflow helper.
- `fish/`: fish shell configuration, functions, completions, and plugin state.
- `nvim/`: Neovim/LazyVim configuration and related editor settings.
- `tmux/`: tmux configuration files linked into the user environment.
- `kitty.conf`, `gitconfig`: app configs.
- `niri/`, `ironbar/`, `mako/`, `keyd/`: Linux desktop, bar, notification, and
  keyboard configuration.
- `systemd/`: systemd unit files, currently reserved for user services.
- `dotfiles-map.json`: manifest mapping repo files to install targets;
  consumed by `bin/dotfiles-apply`.
- `downloads.json`, `downloads.schema.json`: downloaded binary/tool manifest
  and schema used by `bin/dotfiles-apply`.
- `opencode/`: opencode config, plugins, skills, and service/container helpers.
- `ha_helper/`: independent Rust crate for OpenWrt WiFi presence to MQTT.
- `rproxy/`: independent Rust crate for an HTTP/TCP reverse proxy CLI.
- `scripts/`: helper scripts for building static tools.
- `tmux-box.md`: tbox usage and workflow documentation.
- When adding a new top-level directory, add a short entry here describing its
  purpose and any important maintenance expectations.

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

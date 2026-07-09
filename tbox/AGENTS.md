# AGENTS

This directory contains the `tbox` Python package. `bin/tbox` is a thin launcher
that adds the repository root to `sys.path` and calls `tbox.cli.main()`.

Tests import from `tbox.core` directly. When changing tbox commands or behavior,
update relevant tests and `tmux-box.md` together.

`bin/tests/test_tbox_integration.py` runs `tbox` as a subprocess and requires
the `tbox` package to be importable from the repository root.

#!/usr/bin/env python3
from __future__ import annotations

import argparse
from typing import Optional

from . import core


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="tmux session persistence + switcher (tbox)")
    subparsers = parser.add_subparsers(dest="command")

    save_parser = subparsers.add_parser("save", help="Save a tmux session dump")
    save_parser.add_argument("name", nargs="?", help="Target tmux session name")

    autosave_parser = subparsers.add_parser("autosave", help="Autosave all named tmux sessions")
    autosave_parser.add_argument("--quiet", action="store_true", help="Suppress output")
    autosave_parser.add_argument(
        "--throttle-seconds",
        type=float,
        default=3.0,
        help="Skip autosave if run within this many seconds (default: 3)",
    )

    select_parser = subparsers.add_parser("select", help="Select a session (live + archived)")
    select_parser.add_argument("name", nargs="?", help="Session name")
    select_parser.add_argument(
        "--no-run-commands",
        action="store_false",
        dest="run_commands",
        help="Do not run pane commands during restore",
    )
    select_parser.add_argument(
        "-n",
        "--new",
        action="store_true",
        help="Restore archived sessions into a new tmux session",
    )
    select_parser.set_defaults(run_commands=True, new=False)

    drop_parser = subparsers.add_parser("drop", help="Drop a stored session archive")
    drop_parser.add_argument("name", nargs="?", help="Stored session name")

    list_parser = subparsers.add_parser("list", help="List sessions")
    list_parser.add_argument("-v", "--verbose", action="store_true", help="Show archive path")
    list_parser.add_argument("--all", action="store_true", help="Include live sessions")

    preview_parser = subparsers.add_parser("preview", help="Preview an archived session")
    preview_parser.add_argument("name", help="Session name")

    inspect_parser = subparsers.add_parser(
        "inspect", help="Inspect archive directory and JSON contents"
    )
    inspect_parser.add_argument("name", nargs="?", help="Session name")

    snippet_parser = subparsers.add_parser("tmux-snippet", help="Print a tmux.conf snippet")
    snippet_parser.add_argument(
        "--throttle-seconds",
        type=float,
        default=3.0,
        help="Throttle used in autosave snippet (default: 3)",
    )
    snippet_parser.add_argument(
        "--tbox-command",
        dest="tbox_command",
        default=None,
        help="Command to run in tmux hooks (example: tbox). Defaults to a resolved path.",
    )

    return parser


def main(argv: Optional[list[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command == "save":
        return core.cmd_save(args.name)
    if args.command == "autosave":
        return core.cmd_autosave(args.throttle_seconds, args.quiet)
    if args.command == "select":
        return core.cmd_select(args.run_commands, args.new, args.name)
    if args.command == "drop":
        return core.cmd_drop(args.name)
    if args.command == "list":
        return core.cmd_list(args.verbose, args.all)
    if args.command == "preview":
        return core.cmd_preview(args.name)
    if args.command == "inspect":
        return core.cmd_inspect(args.name)
    if args.command == "tmux-snippet":
        return core.cmd_tmux_snippet(args.throttle_seconds, args.tbox_command)
    parser.print_help()
    return 2

import importlib.machinery
import importlib.util
import json
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

TESTS_DIR = pathlib.Path(__file__).resolve().parent
if str(TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(TESTS_DIR))
from test_utils import CapturingTestCase


def load_tmux_load_module():
    tmux_load_path = pathlib.Path(__file__).resolve().parents[1] / "tmux-load"
    spec = importlib.util.spec_from_file_location(
        "tmux_load",
        tmux_load_path,
        loader=importlib.machinery.SourceFileLoader("tmux_load", str(tmux_load_path)),
    )
    if not spec or not spec.loader:
        raise RuntimeError(f"Failed to load spec for {tmux_load_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class TmuxLoadWindowRestoreTests(CapturingTestCase):
    def setUp(self):
        super().setUp()
        self.tmux_load = load_tmux_load_module()

    def test_restore_new_session_creates_all_windows(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 1, "name": "w1", "panes": [{"index": 0, "path": "/"}]},
                {"index": 2, "name": "w2", "panes": [{"index": 0, "path": "/"}]},
                {"index": 3, "name": "w3", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []
        win_counter = {"n": 0}

        def tmux_out(args):
            calls.append(args)
            if args and args[0] in {"new-session", "new-window"}:
                win_counter["n"] += 1
                return f"@{win_counter['n']}"
            return ""

        ensure_calls = []

        def ensure_window_panes(target, panes, run_commands, sort_by_index=True):
            ensure_calls.append((target, len(panes)))

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "ensure_window_panes", side_effect=ensure_window_panes), \
            mock.patch.object(self.tmux_load, "unique_session_name", side_effect=lambda name: name), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        new_session_calls = [c for c in calls if c and c[0] == "new-session"]
        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        self.assertEqual(len(new_session_calls), 1)
        self.assertEqual(len(new_window_calls), 3)
        self.assertEqual(len(ensure_calls), 3)
        self.assertIn("-n", new_session_calls[0])
        self.assertIn("__tmux_load__", new_session_calls[0])
        self.assertIn("-c", new_session_calls[0])
        self.assertIn("/", new_session_calls[0])
        self.assertIn("-n", new_window_calls[0])
        self.assertIn("w1", new_window_calls[0])
        self.assertIn("-c", new_window_calls[0])
        self.assertIn("/", new_window_calls[0])
        self.assertIn("-n", new_window_calls[1])
        self.assertIn("w2", new_window_calls[1])
        self.assertIn("-c", new_window_calls[1])
        self.assertIn("/", new_window_calls[1])
        self.assertIn("-n", new_window_calls[2])
        self.assertIn("w3", new_window_calls[2])
        self.assertIn("-c", new_window_calls[2])
        self.assertIn("/", new_window_calls[2])
        kill_window_calls = [c for c in calls if c and c[0] == "kill-window"]
        self.assertEqual(len(kill_window_calls), 1)
        self.assertIn("@1", kill_window_calls[0])

    def test_restore_applies_window_automatic_rename(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "automatic_rename": "on", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] == "new-session":
                return "@1"
            if args and args[0] == "new-window":
                return "@2"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        set_calls = [c for c in queue_calls if c and c[:4] == ["set-window-option", "-t", "@2", "automatic-rename"]]
        self.assertEqual(len(set_calls), 1)
        self.assertEqual(set_calls[0], ["set-window-option", "-t", "@2", "automatic-rename", "on"])

    def test_restore_skips_empty_automatic_rename(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "automatic_rename": "", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] == "new-session":
                return "@1"
            if args and args[0] == "new-window":
                return "@2"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        set_calls = [c for c in queue_calls if c and "automatic-rename" in c]
        self.assertEqual(len(set_calls), 0)

    def test_restore_applies_zoomed_window(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "zoomed": True, "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] == "new-session":
                return "@1"
            if args and args[0] == "new-window":
                return "@2"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        resize_calls = [c for c in queue_calls if c and c[0] == "resize-pane"]
        self.assertEqual(len(resize_calls), 1)
        self.assertEqual(resize_calls[0], ["resize-pane", "-Z", "-t", "@2"])

    def test_ensure_window_panes_sets_titles_and_runs_commands(self):
        panes = [
            {"index": 0, "title": "t0", "path": "/", "processes": [{"command": ["fish"]}, {"command": ["nvim", "Cargo.toml"]}]},
            {"index": 1, "title": "t1", "path": "/", "processes": [{"command": ["fish"]}, {"command": ["ls"]}]},
        ]
        calls = []

        with mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "list_pane_ids_by_index", return_value={0: "%1", 1: "%2"}):
            self.tmux_load.ensure_window_panes("@1", panes, run_commands=True)

        self.assertIn(["select-pane", "-t", "%1", "-T", "t0"], calls)
        self.assertIn(["select-pane", "-t", "%2", "-T", "t1"], calls)
        split_calls = [c for c in calls if c and c[0] == "split-window"]
        self.assertEqual(len(split_calls), 1)
        self.assertNotIn("--", split_calls[0])
        self.assertIn(["send-keys", "-t", "%1", "-l", "nvim Cargo.toml"], calls)
        self.assertIn(["send-keys", "-t", "%1", "Enter"], calls)
        self.assertIn(["send-keys", "-t", "%2", "-l", "ls"], calls)
        self.assertIn(["send-keys", "-t", "%2", "Enter"], calls)

    def test_ensure_window_panes_selects_active_pane(self):
        panes = [
            {"index": 0, "path": "/"},
            {"index": 1, "active": True, "path": "/"},
        ]
        calls = []

        with mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "list_pane_ids_by_index", return_value={0: "%1", 1: "%2"}):
            self.tmux_load.ensure_window_panes("@1", panes, run_commands=False)

        self.assertIn(["select-pane", "-t", "%2"], calls)

    def test_restore_reuse_current_appends_all_windows(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 1, "name": "w1", "active": True, "panes": [{"index": 0, "path": "/"}]},
                {"index": 2, "name": "w2", "panes": [{"index": 0, "path": "/"}]},
                {"index": 3, "name": "w3", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []
        win_counter = {"n": 0}

        def tmux_out(args):
            calls.append(args)
            if args == ["display-message", "-p", "#{window_id}"]:
                return "@orig"
            if args and args[0] == "new-window":
                win_counter["n"] += 1
                return f"@{win_counter['n']}"
            return ""

        ensure_calls = []

        def ensure_window_panes(target, panes, run_commands, sort_by_index=True):
            ensure_calls.append((target, len(panes)))

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "ensure_window_panes", side_effect=ensure_window_panes), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=True), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="cur",
                base_dir="",
                fallback_dir="/",
            )

        new_session_calls = [c for c in calls if c and c[0] == "new-session"]
        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        cleanup_calls = [c for c in calls if c and c[0] == "run-shell"]
        select_window_calls = [c for c in queue_calls if c and c[0] == "select-window"]
        self.assertEqual(len(new_session_calls), 0)
        self.assertEqual(len(new_window_calls), 3)
        self.assertEqual(len(ensure_calls), 3)
        self.assertEqual(len(cleanup_calls), 1)
        self.assertEqual(len(select_window_calls), 1)
        self.assertIn("@orig", cleanup_calls[0][-1])
        self.assertIn("-n", new_window_calls[0])
        self.assertIn("w1", new_window_calls[0])
        self.assertIn("-c", new_window_calls[0])
        self.assertIn("/", new_window_calls[0])
        self.assertIn("-n", new_window_calls[1])
        self.assertIn("w2", new_window_calls[1])
        self.assertIn("-c", new_window_calls[1])
        self.assertIn("/", new_window_calls[1])
        self.assertIn("-n", new_window_calls[2])
        self.assertIn("w3", new_window_calls[2])
        self.assertIn("-c", new_window_calls[2])
        self.assertIn("/", new_window_calls[2])

    def test_restore_reuse_current_with_multiple_panes(self):
        data = {
            "name": "sess",
            "windows": [
                {
                    "index": 1,
                    "name": "w1",
                    "panes": [
                        {"index": 0, "path": "/"},
                        {"index": 1, "path": "/"},
                    ],
                },
                {
                    "index": 2,
                    "name": "w2",
                    "panes": [
                        {"index": 0, "path": "/"},
                        {"index": 1, "path": "/"},
                        {"index": 2, "path": "/"},
                    ],
                },
            ],
        }
        calls = []
        queue_calls = []
        win_counter = {"n": 0}

        def tmux_out(args):
            calls.append(args)
            if args == ["display-message", "-p", "#{window_id}"]:
                return "@orig"
            if args and args[0] == "new-window":
                win_counter["n"] += 1
                return f"@{win_counter['n']}"
            return ""

        ensure_calls = []

        def ensure_window_panes(target, panes, run_commands, sort_by_index=True):
            ensure_calls.append((target, len(panes)))

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "ensure_window_panes", side_effect=ensure_window_panes), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=True), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="cur",
                base_dir="",
                fallback_dir="/",
            )

        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        cleanup_calls = [c for c in calls if c and c[0] == "run-shell"]
        self.assertEqual(len(new_window_calls), 2)
        self.assertEqual(ensure_calls, [("@1", 2), ("@2", 3)])
        self.assertEqual(len(cleanup_calls), 1)
        self.assertIn("@orig", cleanup_calls[0][-1])
        self.assertIn("-n", new_window_calls[0])
        self.assertIn("w1", new_window_calls[0])
        self.assertIn("-c", new_window_calls[0])
        self.assertIn("/", new_window_calls[0])
        self.assertIn("-n", new_window_calls[1])
        self.assertIn("w2", new_window_calls[1])
        self.assertIn("-c", new_window_calls[1])
        self.assertIn("/", new_window_calls[1])

    def test_new_window_format_before_command(self):
        data = {
            "name": "sess",
            "windows": [
                {
                    "index": 0,
                    "name": "w1",
                    "panes": [
                        {
                            "index": 0,
                            "path": "/",
                            "processes": [{"command": ["nvim", "README.md"]}],
                        }
                    ],
                }
            ],
        }
        calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] in {"new-session", "new-window"}:
                return "@1"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "ensure_window_panes"), \
            mock.patch.object(self.tmux_load, "apply_window_options"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=True,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        self.assertEqual(len(new_window_calls), 1)
        cmd = new_window_calls[0]
        self.assertIn("-F", cmd)
        self.assertIn("--", cmd)
        self.assertLess(cmd.index("-F"), cmd.index("--"))

    def test_restore_new_session_uses_base_dir_only_for_first_window(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "panes": [{"index": 0, "path": "/fromdump1"}]},
                {"index": 1, "name": "w2", "panes": [{"index": 0, "path": "/fromdump2"}]},
            ],
        }
        calls = []
        queue_calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] in {"new-session", "new-window"}:
                return "@1"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "ensure_window_panes"), \
            mock.patch.object(self.tmux_load, "apply_window_options"), \
            mock.patch.object(self.tmux_load, "restore_directory", side_effect=lambda path, fallback: path), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="sess",
                base_dir="/base",
                fallback_dir="/cwd",
            )

        new_session_calls = [c for c in calls if c and c[0] == "new-session"]
        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        self.assertEqual(len(new_session_calls), 1)
        self.assertEqual(len(new_window_calls), 2)
        self.assertIn("-c", new_session_calls[0])
        self.assertIn("/base", new_session_calls[0])
        self.assertIn("-c", new_window_calls[0])
        self.assertIn("/base", new_window_calls[0])
        self.assertIn("-c", new_window_calls[1])
        self.assertIn("/fromdump2", new_window_calls[1])
        kill_window_calls = [c for c in queue_calls if c and c[0] == "kill-window"]
        self.assertEqual(len(kill_window_calls), 1)
        self.assertIn("@1", kill_window_calls[0])

    def test_numeric_session_target_uses_colon_suffix(self):
        data = {
            "name": "123",
            "windows": [
                {"index": 0, "name": "w1", "panes": [{"index": 0, "path": "/"}]},
                {"index": 1, "name": "w2", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        win_counter = {"n": 0}

        def tmux_out(args):
            calls.append(args)
            if args and args[0] == "new-window":
                win_counter["n"] += 1
                return f"@{win_counter['n']}"
            if args and args[0] == "list-windows":
                return "0"
            if args and args[0] == "list-panes":
                return "%1\t0\tfish\t0"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=False,
                run_commands=False,
                target_session="123",
                base_dir="",
                fallback_dir="/",
            )

        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        self.assertIn("123:", new_window_calls[0])

    def test_non_empty_session_requires_flag(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "panes": [{"index": 0, "path": "/"}]},
            ],
        }

        with mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=False):
            with self.assertRaises(RuntimeError) as ctx:
                self.tmux_load.restore_from_dump(
                    data,
                    force=False,
                    append=False,
                    run_commands=False,
                    target_session="sess",
                    base_dir="",
                    fallback_dir="/",
                )
        self.assertIn("not empty", str(ctx.exception))

    def test_force_clears_target_session(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] == "list-windows":
                return "@orig"
            if args and args[0] in {"new-window", "new-session"}:
                return "@1"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="sess"), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True):
            self.tmux_load.restore_from_dump(
                data,
                force=True,
                append=False,
                run_commands=False,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        clear_calls = [c for c in queue_calls if c and c[0] == "kill-window"]
        self.assertEqual(clear_calls, [])
        cleanup_calls = [c for c in calls if c and c[0] == "run-shell"]
        self.assertEqual(len(cleanup_calls), 1)
        self.assertIn("kill-window -t @orig", cleanup_calls[0][-1])

    def test_append_keeps_target_session(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
        queue_calls = []

        def tmux_out(args):
            calls.append(args)
            if args and args[0] in {"new-window", "new-session"}:
                return "@1"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=True,
                run_commands=False,
                target_session="sess",
                base_dir="",
                fallback_dir="/",
            )

        kill_calls = [c for c in queue_calls if c and c[0] == "kill-session"]
        self.assertEqual(len(kill_calls), 0)

    def test_append_keeps_empty_target_window(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "restored", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        queue_calls = []

        def tmux_out(args):
            if args and args[0] == "new-window":
                return "@new"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=queue_calls.append), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=True), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value=None):
            self.tmux_load.restore_from_dump(
                data, False, True, False, "sess", "", "/"
            )

        self.assertFalse(any(call[0] == "kill-window" for call in queue_calls))

    def test_first_pane_shell_command_runs_once(self):
        data = {
            "name": "sess",
            "windows": [
                {
                    "index": 0,
                    "name": "w",
                    "panes": [
                        {
                            "index": 0,
                            "path": "/",
                            "processes": [
                                {"pid": 1, "ppid": 0, "command": ["fish"]},
                                {"pid": 2, "ppid": 1, "command": ["nvim", "README.md"]},
                            ],
                        }
                    ],
                }
            ],
        }
        queue_calls = []

        def tmux_out(args):
            if args and args[0] == "new-session":
                return "@placeholder"
            if args and args[0] == "new-window":
                return "@new"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=queue_calls.append), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value=None), \
            mock.patch.object(self.tmux_load, "list_pane_ids_by_index", return_value={0: "%1"}):
            self.tmux_load.restore_from_dump(
                data, False, False, True, "sess", "", "/"
            )

        literal_sends = [call for call in queue_calls if call[:4] == ["send-keys", "-t", "%1", "-l"]]
        self.assertEqual(literal_sends, [["send-keys", "-t", "%1", "-l", "nvim README.md"]])

    def test_base_dir_overrides_first_restored_window(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 0, "name": "w1", "panes": [{"index": 0, "path": "/dump1"}]},
                {"index": 1, "name": "w2", "panes": [{"index": 0, "path": "/dump2"}]},
            ],
        }
        direct_calls = []

        def tmux_out(args):
            direct_calls.append(args)
            if args and args[0] == "new-session":
                return "@placeholder"
            if args and args[0] == "new-window":
                return f"@{len(direct_calls)}"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue"), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value=None), \
            mock.patch.object(self.tmux_load, "restore_directory", side_effect=lambda path, fallback: path), \
            mock.patch.object(self.tmux_load, "ensure_window_panes"):
            self.tmux_load.restore_from_dump(
                data, False, False, False, "sess", "/override", "/fallback"
            )

        new_windows = [call for call in direct_calls if call[0] == "new-window"]
        first_cwd = new_windows[0][new_windows[0].index("-c") + 1]
        second_cwd = new_windows[1][new_windows[1].index("-c") + 1]
        self.assertEqual(first_cwd, "/override")
        self.assertEqual(second_cwd, "/dump2")


class TmuxLoadMainTests(unittest.TestCase):
    def setUp(self):
        self.tmux_load = load_tmux_load_module()

    def write_dump(self, data):
        tmp = tempfile.NamedTemporaryFile(mode="w", delete=False)
        json.dump(data, tmp)
        tmp.flush()
        tmp.close()
        return tmp.name

    def valid_dump(self, name="dump"):
        data = {"windows": [{"index": 0, "panes": [{"index": 0, "path": "/"}]}]}
        if name is not None:
            data["name"] = name
        return data

    def test_main_uses_dump_session_outside_tmux(self):
        data = self.valid_dump()
        dump_path = self.write_dump(data)
        restore_calls = []
        tmux_calls = []

        def restore_from_dump(*args, **kwargs):
            restore_calls.append((args, kwargs))

        def tmux_out(args):
            tmux_calls.append(args)
            return ""

        argv = ["tmux-load", dump_path]
        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=restore_from_dump), \
            mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False), \
            mock.patch.object(self.tmux_load.sys.stdin, "isatty", return_value=True), \
            mock.patch.object(self.tmux_load.sys.stdout, "isatty", return_value=True), \
            mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "dump")
        self.assertIn(["attach-session", "-t", "dump"], tmux_calls)

    def test_main_outside_tmux_uses_unique_session(self):
        data = self.valid_dump()
        dump_path = self.write_dump(data)
        restore_calls = []
        tmux_calls = []

        def restore_from_dump(*args, **kwargs):
            restore_calls.append((args, kwargs))

        def tmux_out(args):
            tmux_calls.append(args)
            return ""

        argv = ["tmux-load", dump_path]
        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=restore_from_dump), \
            mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "unique_session_name", return_value="dump(1)"), \
            mock.patch.object(self.tmux_load.sys.stdin, "isatty", return_value=True), \
            mock.patch.object(self.tmux_load.sys.stdout, "isatty", return_value=True), \
            mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "dump(1)")
        self.assertIn(["attach-session", "-t", "dump(1)"], tmux_calls)

    def test_main_force_outside_tmux_targets_existing_dump_session(self):
        data = {"name": "dump", "windows": [{"index": 0, "panes": [{"index": 0, "path": "/"}]}]}
        dump_path = self.write_dump(data)
        restore_calls = []
        argv = ["tmux-load", "-f", dump_path]

        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=lambda *args: restore_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_out", return_value=""), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "unique_session_name", return_value="dump(1)"), \
            mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(restore_calls[0][4], "dump")

    def test_main_rejects_non_object_dump_without_traceback(self):
        dump_path = self.write_dump(None)
        argv = ["tmux-load", "--session", "target", dump_path]

        with mock.patch.object(self.tmux_load.sys, "argv", argv), \
            mock.patch.object(self.tmux_load.sys, "stderr"):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 2)

    def test_main_in_tmux_renames_to_dump_session(self):
        data = self.valid_dump()
        dump_path = self.write_dump(data)
        restore_calls = []
        tmux_calls = []

        def restore_from_dump(*args, **kwargs):
            restore_calls.append((args, kwargs))

        def tmux_out(args):
            tmux_calls.append(args)
            return ""

        argv = ["tmux-load", dump_path]
        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=restore_from_dump), \
            mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertIn(["rename-session", "-t", "cur", "dump"], tmux_calls)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "cur")
        self.assertNotIn(["switch-client", "-t", "dump"], tmux_calls)

    def test_main_in_tmux_renames_with_unique_name(self):
        data = self.valid_dump()
        dump_path = self.write_dump(data)
        restore_calls = []
        tmux_calls = []

        def restore_from_dump(*args, **kwargs):
            restore_calls.append((args, kwargs))

        def tmux_out(args):
            tmux_calls.append(args)
            return ""

        argv = ["tmux-load", dump_path]
        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=restore_from_dump), \
            mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "unique_session_name", return_value="dump(1)"), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertIn(["rename-session", "-t", "cur", "dump(1)"], tmux_calls)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "cur")
        self.assertNotIn(["switch-client", "-t", "dump(1)"], tmux_calls)

    def test_main_in_tmux_without_dump_name_uses_current_session(self):
        data = self.valid_dump(name=None)
        dump_path = self.write_dump(data)
        restore_calls = []
        tmux_calls = []

        def restore_from_dump(*args, **kwargs):
            restore_calls.append((args, kwargs))

        def tmux_out(args):
            tmux_calls.append(args)
            return ""

        argv = ["tmux-load", dump_path]
        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=restore_from_dump), \
            mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "cur")
        self.assertNotIn(["rename-session", "-t", "cur", "cur"], tmux_calls)
        self.assertNotIn(["switch-client", "-t", "cur"], tmux_calls)

    def test_main_requires_session_when_no_name_outside_tmux(self):
        data = self.valid_dump(name=None)
        dump_path = self.write_dump(data)
        argv = ["tmux-load", dump_path]
        with mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv), \
            mock.patch.object(self.tmux_load.sys, "stderr"):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 2)

    def test_main_in_tmux_switches_when_target_differs(self):
        data = self.valid_dump()
        dump_path = self.write_dump(data)
        restore_calls = []
        tmux_calls = []

        def restore_from_dump(*args, **kwargs):
            restore_calls.append((args, kwargs))

        def tmux_out(args):
            tmux_calls.append(args)
            return ""

        argv = ["tmux-load", "--session", "dump", dump_path]
        with mock.patch.object(self.tmux_load, "restore_from_dump", side_effect=restore_from_dump), \
            mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.dict(self.tmux_load.os.environ, {"TMUX": "1"}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "dump")
        self.assertIn(["switch-client", "-t", "dump"], tmux_calls)

    def test_session_name_from_dump_variants(self):
        self.assertEqual(self.tmux_load.session_name_from_dump({"name": "s"}), "s")
        self.assertEqual(self.tmux_load.session_name_from_dump({"session_name": "s2"}), "s2")
        self.assertEqual(self.tmux_load.session_name_from_dump({"windows": []}), None)
        self.assertEqual(self.tmux_load.session_name_from_dump({"sessions": [{"name": "s3"}]}), "s3")


class TmuxLoadHelpersTests(unittest.TestCase):
    def setUp(self):
        self.tmux_load = load_tmux_load_module()

    def test_normalize_start_command(self):
        self.assertEqual(self.tmux_load.normalize_start_command(None), "")
        self.assertEqual(self.tmux_load.normalize_start_command(["echo", "hi"]), "echo hi")
        self.assertEqual(self.tmux_load.normalize_start_command("ls"), "ls")

    def test_command_from_processes_prefers_child_of_shell(self):
        processes = [
            {"command": ["/bin/bash"]},
            {"command": ["nvim", "Cargo.toml"]},
        ]
        self.assertEqual(self.tmux_load.run_plan_from_processes(processes), ([], "nvim Cargo.toml"))

    def test_command_from_processes_falls_back_to_shell(self):
        processes = [
            {"command": ["fish"]},
        ]
        self.assertEqual(self.tmux_load.run_plan_from_processes(processes), ([], ""))

    def test_command_from_processes_uses_first_non_shell(self):
        processes = [
            {"command": ["vim", "README.md"]},
            {"command": ["sleep", "1"]},
        ]
        self.assertEqual(self.tmux_load.run_plan_from_processes(processes), (["vim", "README.md"], ""))

    def test_command_from_processes_uses_foreground_process(self):
        processes = [
            {"pid": 3, "ppid": 1, "command": ["nvim", "README.md"], "foreground": True, "pgid": 3},
        ]
        self.assertEqual(self.tmux_load.run_plan_from_processes(processes), (["nvim", "README.md"], ""))

    def test_command_from_processes_runs_shell_child_inside_shell(self):
        processes = [
            {"pid": 10, "ppid": 1, "command": ["fish"], "foreground": False, "pgid": 10},
            {"pid": 20, "ppid": 10, "command": ["vim", "README.md"], "foreground": True, "pgid": 20},
        ]
        self.assertEqual(self.tmux_load.run_plan_from_processes(processes), ([], "vim README.md"))

    def test_command_from_processes_skips_foreground_pipeline(self):
        processes = [
            {"command": ["producer"], "foreground": True, "pgid": 3},
            {"command": ["consumer"], "foreground": True, "pgid": 3},
        ]
        self.assertEqual(self.tmux_load.run_plan_from_processes(processes), ([], ""))

    def test_run_plan_prefers_running_tracked_shell_command(self):
        pane = {
            "shell_command": {
                "command": "FOO='a b' vim README.md | tee output.log",
                "running": True,
                "source": "fish",
            },
            "processes": [{"command": ["tee", "output.log"], "foreground": True, "pgid": 3}],
        }
        self.assertEqual(
            self.tmux_load.run_plan_from_pane(pane),
            ([], "FOO='a b' vim README.md | tee output.log"),
        )

    def test_run_plan_ignores_idle_tracked_shell_command(self):
        pane = {
            "shell_command": {"command": "vim README.md", "running": False, "source": "fish"},
            "processes": [{"command": ["fish"]}],
        }
        self.assertEqual(self.tmux_load.run_plan_from_pane(pane), ([], ""))

    def test_normalize_path_strips_file_scheme(self):
        self.assertEqual(self.tmux_load.normalize_path("file:///tmp"), "/tmp")

    def test_normalize_path_preserves_relative_path(self):
        self.assertEqual(self.tmux_load.normalize_path("project/src"), "project/src")

    def test_restore_directory_falls_back_when_dump_path_is_missing(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            self.assertEqual(
                self.tmux_load.restore_directory("/definitely/missing/tmux-load-test", tmpdir),
                tmpdir,
            )

    def test_list_pane_ids_by_index_parses(self):
        out = "0\t%1\n1\t%2\n"
        with mock.patch.object(self.tmux_load, "tmux_out", return_value=out):
            res = self.tmux_load.list_pane_ids_by_index("@1")

        self.assertEqual(res, {0: "%1", 1: "%2"})

    def test_is_empty_session_with_shell(self):
        windows_out = "0\n"
        panes_out = "%1\t0\tbash\t0\n"
        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=[windows_out, panes_out]):
            self.assertTrue(self.tmux_load.is_empty_session("sess", allow_current_pane=False))


if __name__ == "__main__":
    unittest.main()

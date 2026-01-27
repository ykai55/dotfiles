import importlib.machinery
import importlib.util
import json
import pathlib
import tempfile
import unittest
from unittest import mock


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


class TmuxLoadWindowRestoreTests(unittest.TestCase):
    def setUp(self):
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
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="sess")

        new_session_calls = [c for c in calls if c and c[0] == "new-session"]
        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        self.assertEqual(len(new_session_calls), 1)
        self.assertEqual(len(new_window_calls), 2)
        self.assertEqual(len(ensure_calls), 3)
        self.assertIn("-n", new_session_calls[0])
        self.assertIn("w1", new_session_calls[0])
        self.assertIn("-c", new_session_calls[0])
        self.assertIn("/", new_session_calls[0])
        self.assertIn("-n", new_window_calls[0])
        self.assertIn("w2", new_window_calls[0])
        self.assertIn("-c", new_window_calls[0])
        self.assertIn("/", new_window_calls[0])
        self.assertIn("-n", new_window_calls[1])
        self.assertIn("w3", new_window_calls[1])
        self.assertIn("-c", new_window_calls[1])
        self.assertIn("/", new_window_calls[1])

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
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="sess")

        set_calls = [c for c in queue_calls if c and c[0] == "set-window-option"]
        self.assertEqual(len(set_calls), 1)
        self.assertEqual(set_calls[0], ["set-window-option", "-t", "@1", "automatic-rename", "on"])

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
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: queue_calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="sess")

        set_calls = [c for c in queue_calls if c and c[0] == "set-window-option"]
        self.assertEqual(len(set_calls), 0)

    def test_ensure_window_panes_sets_titles_and_runs_commands(self):
        panes = [
            {"index": 0, "title": "t0", "start_command": ["echo", "hi"], "path": "/"},
            {"index": 1, "title": "t1", "start_command": "ls", "path": "/"},
        ]
        calls = []

        with mock.patch.object(self.tmux_load, "tmux_queue", side_effect=lambda args: calls.append(args)), \
            mock.patch.object(self.tmux_load, "tmux_flush"), \
            mock.patch.object(self.tmux_load, "list_pane_ids_by_index", return_value={0: "%1", 1: "%2"}):
            self.tmux_load.ensure_window_panes("@1", panes, run_commands=True)

        self.assertIn(["select-pane", "-t", "%1", "-T", "t0"], calls)
        self.assertIn(["select-pane", "-t", "%2", "-T", "t1"], calls)
        self.assertIn(["send-keys", "-t", "%1", "echo hi", "Enter"], calls)
        self.assertIn(["send-keys", "-t", "%2", "ls", "Enter"], calls)

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

        self.assertIn(["select-pane", "-t", "@1.1"], calls)

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
            mock.patch.object(self.tmux_load, "session_exists", return_value=True):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="cur")

        new_session_calls = [c for c in calls if c and c[0] == "new-session"]
        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        kill_window_calls = [c for c in queue_calls if c and c[0] == "kill-window"]
        select_window_calls = [c for c in queue_calls if c and c[0] == "select-window"]
        self.assertEqual(len(new_session_calls), 0)
        self.assertEqual(len(new_window_calls), 3)
        self.assertEqual(len(ensure_calls), 3)
        self.assertEqual(len(kill_window_calls), 1)
        self.assertEqual(len(select_window_calls), 1)
        self.assertIn("@orig", kill_window_calls[0])
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
            mock.patch.object(self.tmux_load, "session_exists", return_value=True):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="cur")

        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        kill_window_calls = [c for c in queue_calls if c and c[0] == "kill-window"]
        self.assertEqual(len(new_window_calls), 2)
        self.assertEqual(ensure_calls, [("@1", 2), ("@2", 3)])
        self.assertEqual(len(kill_window_calls), 1)
        self.assertIn("@orig", kill_window_calls[0])
        self.assertIn("-n", new_window_calls[0])
        self.assertIn("w1", new_window_calls[0])
        self.assertIn("-c", new_window_calls[0])
        self.assertIn("/", new_window_calls[0])
        self.assertIn("-n", new_window_calls[1])
        self.assertIn("w2", new_window_calls[1])
        self.assertIn("-c", new_window_calls[1])
        self.assertIn("/", new_window_calls[1])

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
            mock.patch.object(self.tmux_load, "session_exists", return_value=False):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="123")

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
            )

        clear_calls = [c for c in queue_calls if c and c[0] == "kill-window"]
        self.assertEqual(len(clear_calls), 1)
        self.assertIn("-a", clear_calls[0])

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
            )

        kill_calls = [c for c in queue_calls if c and c[0] == "kill-session"]
        self.assertEqual(len(kill_calls), 0)


class TmuxLoadMainTests(unittest.TestCase):
    def setUp(self):
        self.tmux_load = load_tmux_load_module()

    def write_dump(self, data):
        tmp = tempfile.NamedTemporaryFile(mode="w", delete=False)
        json.dump(data, tmp)
        tmp.flush()
        tmp.close()
        return tmp.name

    def test_main_uses_dump_session_outside_tmux(self):
        data = {"name": "dump", "windows": []}
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
            mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "dump")
        self.assertIn(["attach-session", "-t", "dump"], tmux_calls)

    def test_main_outside_tmux_uses_unique_session(self):
        data = {"name": "dump", "windows": []}
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
            mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 0)
        self.assertEqual(len(restore_calls), 1)
        self.assertEqual(restore_calls[0][0][4], "dump(1)")
        self.assertIn(["attach-session", "-t", "dump(1)"], tmux_calls)

    def test_main_in_tmux_renames_to_dump_session(self):
        data = {"name": "dump", "windows": []}
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
        self.assertEqual(restore_calls[0][0][4], "dump")
        self.assertNotIn(["switch-client", "-t", "dump"], tmux_calls)

    def test_main_in_tmux_renames_with_unique_name(self):
        data = {"name": "dump", "windows": []}
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
        self.assertEqual(restore_calls[0][0][4], "dump(1)")
        self.assertNotIn(["switch-client", "-t", "dump(1)"], tmux_calls)

    def test_main_in_tmux_without_dump_name_uses_current_session(self):
        data = {"windows": []}
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
        data = {"windows": []}
        dump_path = self.write_dump(data)
        argv = ["tmux-load", dump_path]
        with mock.patch.dict(self.tmux_load.os.environ, {}, clear=True), \
            mock.patch.object(self.tmux_load.sys, "argv", argv), \
            mock.patch.object(self.tmux_load.sys, "stderr"):
            rc = self.tmux_load.main()

        self.assertEqual(rc, 2)

    def test_main_in_tmux_switches_when_target_differs(self):
        data = {"name": "dump", "windows": []}
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

    def test_normalize_path_strips_file_scheme(self):
        self.assertEqual(self.tmux_load.normalize_path("file:///tmp"), "/tmp")

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

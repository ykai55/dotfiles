import importlib.machinery
import importlib.util
import pathlib
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

    def test_restore_reuse_current_appends_all_windows(self):
        data = {
            "name": "sess",
            "windows": [
                {"index": 1, "name": "w1", "panes": [{"index": 0, "path": "/"}]},
                {"index": 2, "name": "w2", "panes": [{"index": 0, "path": "/"}]},
                {"index": 3, "name": "w3", "panes": [{"index": 0, "path": "/"}]},
            ],
        }
        calls = []
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
            mock.patch.object(self.tmux_load, "ensure_window_panes", side_effect=ensure_window_panes), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=True), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="cur")

        new_session_calls = [c for c in calls if c and c[0] == "new-session"]
        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        kill_window_calls = [c for c in calls if c and c[0] == "kill-window"]
        self.assertEqual(len(new_session_calls), 0)
        self.assertEqual(len(new_window_calls), 3)
        self.assertEqual(len(ensure_calls), 3)
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
            mock.patch.object(self.tmux_load, "ensure_window_panes", side_effect=ensure_window_panes), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=True), \
            mock.patch.object(self.tmux_load, "current_session_name", return_value="cur"), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True):
            self.tmux_load.restore_from_dump(data, force=False, append=False, run_commands=False, target_session="cur")

        new_window_calls = [c for c in calls if c and c[0] == "new-window"]
        kill_window_calls = [c for c in calls if c and c[0] == "kill-window"]
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

        def tmux_out(args):
            calls.append(args)
            if args and args[0] == "list-windows":
                return "@orig"
            if args and args[0] in {"new-window", "new-session"}:
                return "@1"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
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

        clear_calls = [c for c in calls if c and c[0] == "kill-window"]
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

        def tmux_out(args):
            calls.append(args)
            if args and args[0] in {"new-window", "new-session"}:
                return "@1"
            return ""

        with mock.patch.object(self.tmux_load, "tmux_out", side_effect=tmux_out), \
            mock.patch.object(self.tmux_load, "session_exists", return_value=True), \
            mock.patch.object(self.tmux_load, "is_empty_session", return_value=False):
            self.tmux_load.restore_from_dump(
                data,
                force=False,
                append=True,
                run_commands=False,
                target_session="sess",
            )

        kill_calls = [c for c in calls if c and c[0] == "kill-session"]
        self.assertEqual(len(kill_calls), 0)


if __name__ == "__main__":
    unittest.main()

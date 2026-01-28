import importlib.util
import importlib.machinery
import json
import os
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

TESTS_DIR = pathlib.Path(__file__).resolve().parent
if str(TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(TESTS_DIR))
from test_utils import CapturingTestCase


def load_tbox_module():
    tbox_path = pathlib.Path(__file__).resolve().parents[1] / "tbox"
    spec = importlib.util.spec_from_file_location(
        "tbox",
        tbox_path,
        loader=importlib.machinery.SourceFileLoader("tbox", str(tbox_path)),
    )
    if not spec or not spec.loader:
        raise RuntimeError(f"Failed to load spec for {tbox_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class TboxTests(CapturingTestCase):
    def setUp(self):
        super().setUp()
        self.tbox = load_tbox_module()

    def test_cmd_select_uses_tmux_load_without_session(self):
        entry = {"name": "work", "path": "/tmp/dump.json", "mtime": 0.0, "windows_count": 2}
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[entry]), \
            mock.patch.object(self.tbox, "choose_entry", return_value=entry), \
            mock.patch.object(self.tbox, "tool_path", return_value="tmux-load"), \
            mock.patch.object(self.tbox.subprocess, "run") as run_mock:
            run_mock.return_value = mock.Mock(returncode=0)
            rc = self.tbox.cmd_select()

        self.assertEqual(rc, 0)
        run_mock.assert_called_once()
        args = run_mock.call_args[0][0]
        self.assertEqual(args, ["tmux-load", "/tmp/dump.json"])

    def test_cmd_select_requires_entries(self):
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[]), \
            mock.patch.object(self.tbox, "data_dir", return_value="/tmp"):
            rc = self.tbox.cmd_select()
        self.assertEqual(rc, 1)

    def test_cmd_save_writes_dump_file(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            def run_cmd(argv):
                tmp_path = argv[-1]
                with open(tmp_path, "w", encoding="utf-8") as f:
                    json.dump({"name": "work", "windows": []}, f)
                return 0, "", ""

            with mock.patch.object(self.tbox, "data_dir", return_value=tmpdir), \
                mock.patch.object(self.tbox, "current_session_name", return_value="work"), \
                mock.patch.object(self.tbox, "load_saved_sessions", return_value=[]), \
                mock.patch.object(self.tbox, "tool_path", return_value="tmux-dump"), \
                mock.patch.object(self.tbox, "run_cmd", side_effect=run_cmd), \
                mock.patch("builtins.print") as print_mock:
                rc = self.tbox.cmd_save(None)

            self.assertEqual(rc, 0)
            expected = os.path.join(tmpdir, self.tbox.safe_filename("work"))
            self.assertTrue(os.path.exists(expected))
            with open(expected, "r", encoding="utf-8") as f:
                data = json.load(f)
            self.assertEqual(data.get("name"), "work")
            print_mock.assert_called_with("Saved session 'work'")

    def test_cmd_save_requires_session_outside_tmux(self):
        with mock.patch.dict(os.environ, {"TMUX": ""}, clear=True), \
            mock.patch.object(self.tbox, "current_session_name", return_value=None):
            rc = self.tbox.cmd_save(None)
        self.assertEqual(rc, 2)

    def test_cmd_save_reuses_existing_entry_path(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            existing = os.path.join(tmpdir, "existing.json")
            entry = {"name": "work", "path": existing, "mtime": 0.0, "windows_count": 0}

            def run_cmd(argv):
                tmp_path = argv[-1]
                with open(tmp_path, "w", encoding="utf-8") as f:
                    json.dump({"name": "work", "windows": []}, f)
                return 0, "", ""

            with mock.patch.object(self.tbox, "data_dir", return_value=tmpdir), \
                mock.patch.object(self.tbox, "current_session_name", return_value="work"), \
                mock.patch.object(self.tbox, "load_saved_sessions", return_value=[entry]), \
                mock.patch.object(self.tbox, "tool_path", return_value="tmux-dump"), \
                mock.patch.object(self.tbox, "run_cmd", side_effect=run_cmd), \
                mock.patch("builtins.print") as print_mock:
                rc = self.tbox.cmd_save(None)

            self.assertEqual(rc, 0)
            self.assertTrue(os.path.exists(existing))
            print_mock.assert_called_with("Saved session 'work'")

    def test_cmd_drop_removes_named_entry(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = os.path.join(tmpdir, "sess.json")
            with open(path, "w", encoding="utf-8") as f:
                json.dump({"name": "sess", "windows": []}, f)
            entry = {"name": "sess", "path": path, "mtime": 0.0, "windows_count": 0}

            with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[entry]), \
                mock.patch("builtins.print") as print_mock:
                rc = self.tbox.cmd_drop("sess")

            self.assertEqual(rc, 0)
            self.assertFalse(os.path.exists(path))
            print_mock.assert_called_with("Removed session 'sess'")

    def test_cmd_drop_requires_entries(self):
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[]):
            rc = self.tbox.cmd_drop("sess")
        self.assertEqual(rc, 1)

    def test_cmd_drop_requires_name(self):
        with mock.patch.dict(os.environ, {"TMUX": ""}, clear=True), \
            mock.patch.object(self.tbox, "current_session_name", return_value=None):
            rc = self.tbox.cmd_drop(None)
        self.assertEqual(rc, 2)

    def test_cmd_drop_uses_current_session(self):
        entry = {"name": "sess", "path": "/tmp/sess.json", "mtime": 0.0, "windows_count": 0}
        with mock.patch.dict(os.environ, {"TMUX": "1"}, clear=True), \
            mock.patch.object(self.tbox, "current_session_name", return_value="sess"), \
            mock.patch.object(self.tbox, "load_saved_sessions", return_value=[entry]), \
            mock.patch.object(self.tbox.os, "remove") as remove_mock, \
            mock.patch("builtins.print") as print_mock:
            rc = self.tbox.cmd_drop(None)

        self.assertEqual(rc, 0)
        remove_mock.assert_called_once_with("/tmp/sess.json")
        print_mock.assert_called_with("Removed session 'sess'")

    def test_cmd_drop_handles_missing_entry(self):
        entry = {"name": "sess", "path": "/tmp/sess.json", "mtime": 0.0, "windows_count": 0}
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[entry]):
            rc = self.tbox.cmd_drop("other")
        self.assertEqual(rc, 1)

    def test_cmd_list_reports_empty(self):
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[]), \
            mock.patch.object(self.tbox, "data_dir", return_value="/tmp"), \
            mock.patch("builtins.print") as print_mock:
            rc = self.tbox.cmd_list(False)

        self.assertEqual(rc, 1)
        print_mock.assert_called_with("No stored sessions")

    def test_cmd_list_prints_entries(self):
        entries = [
            {"name": "a", "path": "/tmp/a.json", "mtime": 0.0, "windows_count": 1},
            {"name": "b", "path": "/tmp/b.json", "mtime": 0.0, "windows_count": None},
        ]
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=entries), \
            mock.patch.object(self.tbox, "data_dir", return_value="/tmp"), \
            mock.patch("builtins.print") as print_mock:
            rc = self.tbox.cmd_list(False)

        self.assertEqual(rc, 0)
        self.assertEqual(print_mock.call_count, 2)
        printed = [call.args[0] for call in print_mock.call_args_list]
        self.assertTrue(all("/tmp/" not in line for line in printed))

    def test_cmd_list_verbose_includes_path(self):
        entries = [{"name": "a", "path": "/tmp/a.json", "mtime": 0.0, "windows_count": 1}]
        with mock.patch.object(self.tbox, "load_saved_sessions", return_value=entries), \
            mock.patch.object(self.tbox, "data_dir", return_value="/tmp"), \
            mock.patch("builtins.print") as print_mock:
            rc = self.tbox.cmd_list(True)

        self.assertEqual(rc, 0)
        printed = print_mock.call_args[0][0]
        self.assertIn("/tmp/a.json", printed)

    def test_load_saved_sessions_orders_by_mtime(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path1 = os.path.join(tmpdir, "a.json")
            path2 = os.path.join(tmpdir, "b.json")
            with open(path1, "w", encoding="utf-8") as f:
                json.dump({"name": "a", "windows": [{"index": 0}]}, f)
            with open(path2, "w", encoding="utf-8") as f:
                json.dump({"name": "b", "windows": [{"index": 0}, {"index": 1}]}, f)
            os.utime(path1, (1, 1))
            os.utime(path2, (2, 2))

            entries = self.tbox.load_saved_sessions(tmpdir)

            self.assertEqual([e["name"] for e in entries], ["b", "a"])
            self.assertEqual(entries[0]["windows_count"], 2)
            self.assertEqual(entries[1]["windows_count"], 1)

    def test_session_name_and_windows_count_helpers(self):
        data = {"name": "sess", "windows": [{"index": 0}]}
        self.assertEqual(self.tbox.session_name_from_dump(data), "sess")
        self.assertEqual(self.tbox.windows_count_from_dump(data), 1)

        data2 = {"sessions": [{"name": "main", "windows": [{"index": 0}, {"index": 1}]}]}
        self.assertEqual(self.tbox.session_name_from_dump(data2), "main")
        self.assertEqual(self.tbox.windows_count_from_dump(data2), 2)

    def test_safe_filename_is_stable_and_sanitized(self):
        name = "weird/name"
        fname = self.tbox.safe_filename(name)
        self.assertTrue(fname.endswith(".json"))
        self.assertNotIn("/", fname)
        self.assertEqual(fname, self.tbox.safe_filename(name))

    def test_find_entry_by_name(self):
        entries = [{"name": "a"}, {"name": "b"}]
        self.assertIsNotNone(self.tbox.find_entry_by_name(entries, "b"))
        self.assertIsNone(self.tbox.find_entry_by_name(entries, "c"))

    def test_choose_entry_falls_back_to_prompt(self):
        entries = [
            {"name": "one", "path": "/tmp/one.json", "mtime": 0.0, "windows_count": 1},
            {"name": "two", "path": "/tmp/two.json", "mtime": 0.0, "windows_count": 2},
        ]
        with mock.patch.object(self.tbox.shutil, "which", return_value=None), \
            mock.patch("builtins.input", return_value="2"):
            selected = self.tbox.choose_entry(entries, "Select")
        self.assertEqual(selected["name"], "two")

    def test_choose_entry_invalid_selection(self):
        entries = [{"name": "one", "path": "/tmp/one.json", "mtime": 0.0, "windows_count": 1}]
        with mock.patch.object(self.tbox.shutil, "which", return_value=None), \
            mock.patch("builtins.input", return_value="x"):
            selected = self.tbox.choose_entry(entries, "Select")
        self.assertIsNone(selected)

    def test_data_dir_prefers_env(self):
        with mock.patch.dict(os.environ, {"TBOX_DIR": "/tmp/tbox"}, clear=True):
            self.assertEqual(self.tbox.data_dir(), "/tmp/tbox")

    def test_tool_path_prefers_local_executable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            local = os.path.join(tmpdir, "tmux-load")
            with open(local, "w", encoding="utf-8") as f:
                f.write("#!/usr/bin/env bash\n")
            os.chmod(local, 0o755)
            with mock.patch.object(self.tbox, "__file__", os.path.join(tmpdir, "tbox")):
                self.assertEqual(self.tbox.tool_path("tmux-load"), local)

    def test_run_cmd_captures_output(self):
        rc, out, err = self.tbox.run_cmd(["/bin/echo", "hi"])
        self.assertEqual(rc, 0)
        self.assertEqual(out.strip(), "hi")
        self.assertEqual(err, "")

    def test_data_dir_falls_back_to_xdg(self):
        with mock.patch.dict(os.environ, {"XDG_DATA_HOME": "/tmp/xdg"}, clear=True):
            self.assertEqual(self.tbox.data_dir(), "/tmp/xdg/tmux-box")

    def test_current_session_name_returns_none_on_failure(self):
        with mock.patch.object(self.tbox, "run_cmd", return_value=(1, "", "err")):
            self.assertIsNone(self.tbox.current_session_name())

    def test_choose_entry_uses_selector(self):
        entries = [{"name": "one", "path": "/tmp/one.json", "mtime": 0.0, "windows_count": 1}]
        fake_proc = mock.Mock(returncode=0, stdout="one\t1w\t\t/tmp/one.json\n")
        with mock.patch.object(self.tbox.shutil, "which", side_effect=["/usr/bin/fzf", None]), \
            mock.patch.object(self.tbox.subprocess, "run", return_value=fake_proc):
            selected = self.tbox.choose_entry(entries, "Select")
        self.assertEqual(selected["path"], "/tmp/one.json")

    def test_cmd_save_reports_tmux_dump_error(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            with mock.patch.object(self.tbox, "data_dir", return_value=tmpdir), \
                mock.patch.object(self.tbox, "current_session_name", return_value="work"), \
                mock.patch.object(self.tbox, "load_saved_sessions", return_value=[]), \
                mock.patch.object(self.tbox, "tool_path", return_value="tmux-dump"), \
                mock.patch.object(self.tbox, "run_cmd", return_value=(1, "", "boom")):
                rc = self.tbox.cmd_save(None)

            self.assertEqual(rc, 1)



if __name__ == "__main__":
    unittest.main()

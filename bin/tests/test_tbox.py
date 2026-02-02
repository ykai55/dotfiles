import json
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


class TboxTests(CapturingTestCase):
    def setUp(self):
        super().setUp()
        repo_root = pathlib.Path(__file__).resolve().parents[2]
        if str(repo_root) not in sys.path:
            sys.path.insert(0, str(repo_root))
        import tbox.core as tbox_core

        self.core = tbox_core

    def test_data_dir_prefers_tbox_dir(self):
        with mock.patch.dict(os.environ, {"TBOX_DIR": "/tmp/tbox"}, clear=True):
            self.assertEqual(self.core.data_dir(), "/tmp/tbox")

    def test_autosave_saves_named_sessions_only(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            live = [
                self.core.Entry(name="0", live=True),
                self.core.Entry(name="work", live=True),
            ]
            with mock.patch.dict(os.environ, {"TBOX_DIR": tmpdir}, clear=True), \
                mock.patch.object(self.core, "list_live_sessions", return_value=live), \
                mock.patch.object(self.core, "save_session_dump", return_value=os.path.join(tmpdir, "x.json")) as save_mock:
                rc = self.core.cmd_autosave(throttle_seconds=0, quiet=True)
            self.assertEqual(rc, 0)
            save_mock.assert_called_once()
            self.assertEqual(save_mock.call_args[0][0], "work")

    def test_select_live_switches_or_attaches(self):
        entries = [self.core.Entry(name="work", live=True)]
        with mock.patch.object(self.core, "load_saved_sessions", return_value=[]), \
            mock.patch.object(self.core, "list_live_sessions", return_value=entries), \
            mock.patch.object(self.core, "merge_sessions", return_value=entries), \
            mock.patch.object(self.core.subprocess, "run") as run_mock:
            run_mock.return_value = mock.Mock(returncode=0)
            with mock.patch.dict(os.environ, {"TMUX": "1"}, clear=True):
                rc = self.core.cmd_select(True, False, "work")
        self.assertEqual(rc, 0)
        args = run_mock.call_args[0][0]
        self.assertEqual(args, ["tmux", "switch-client", "-t", "work"])

    def test_select_archived_restores_with_tmux_load(self):
        entry = self.core.Entry(name="work", live=False, archive_path="/tmp/dump.json")
        entries = [entry]
        with mock.patch.object(self.core, "load_saved_sessions", return_value=[entry]), \
            mock.patch.object(self.core, "list_live_sessions", return_value=[]), \
            mock.patch.object(self.core, "merge_sessions", return_value=entries), \
            mock.patch.object(self.core, "tool_path", return_value="tmux-load"), \
            mock.patch.object(self.core.subprocess, "run") as run_mock:
            run_mock.return_value = mock.Mock(returncode=0)
            rc = self.core.cmd_select(True, False, "work")
        self.assertEqual(rc, 0)
        args = run_mock.call_args[0][0]
        self.assertEqual(args, ["tmux-load", "/tmp/dump.json", "--session", "work"])

    def test_preview_reports_missing_archive(self):
        with mock.patch.object(self.core, "load_saved_sessions", return_value=[]), \
            mock.patch("builtins.print") as print_mock:
            rc = self.core.cmd_preview("work")
        self.assertEqual(rc, 0)
        print_mock.assert_called_with("No archive for session: work")

    def test_inspect_prints_store_and_content(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            payload = {"name": "work", "windows": []}
            path = os.path.join(tmpdir, self.core.safe_filename("work"))
            with open(path, "w", encoding="utf-8") as f:
                json.dump(payload, f)
            with mock.patch.dict(os.environ, {"TBOX_DIR": tmpdir}, clear=True):
                rc = self.core.cmd_inspect("work")
            self.assertEqual(rc, 0)
            out = self._stdout_buffer.getvalue()
            self.assertIn(f"Store: {tmpdir}", out)
            self.assertIn("Session: work", out)
            self.assertIn(f"Archive: {path}", out)
            self.assertIn('"name": "work"', out)
            self.assertIn('"windows": []', out)

    def test_inspect_without_name_lists_archives(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            payloads = [
                {"name": "work", "windows": []},
                {"name": "play", "windows": []},
            ]
            for payload in payloads:
                path = os.path.join(tmpdir, self.core.safe_filename(payload["name"]))
                with open(path, "w", encoding="utf-8") as f:
                    json.dump(payload, f)
            with mock.patch.dict(os.environ, {"TBOX_DIR": tmpdir}, clear=True):
                rc = self.core.cmd_inspect(None)
            self.assertEqual(rc, 0)
            out = self._stdout_buffer.getvalue()
            self.assertIn(f"Store: {tmpdir}", out)
            self.assertIn("Session: work", out)
            self.assertIn("Session: play", out)

    def test_save_writes_dump_file(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            def fake_run_cmd(argv):
                tmp_path = argv[-1]
                with open(tmp_path, "w", encoding="utf-8") as f:
                    json.dump({"name": "work", "windows": []}, f)
                return 0, "", ""

            with mock.patch.dict(os.environ, {"TBOX_DIR": tmpdir, "TMUX": "1"}, clear=True), \
                mock.patch.object(self.core, "current_session_name", return_value="work"), \
                mock.patch.object(self.core, "tool_path", return_value="tmux-dump"), \
                mock.patch.object(self.core, "run_cmd", side_effect=fake_run_cmd), \
                mock.patch("builtins.print") as print_mock:
                rc = self.core.cmd_save(None)
            self.assertEqual(rc, 0)
            expected = os.path.join(tmpdir, self.core.safe_filename("work"))
            self.assertTrue(os.path.exists(expected))
            print_mock.assert_called_with("Saved session 'work'")


if __name__ == "__main__":
    unittest.main()

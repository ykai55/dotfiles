import importlib.util
import importlib.machinery
import json
import os
import pathlib
import tempfile
import unittest
from unittest import mock


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


class TboxTests(unittest.TestCase):
    def setUp(self):
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

    def test_cmd_push_writes_dump_file(self):
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
                mock.patch.object(self.tbox, "run_cmd", side_effect=run_cmd):
                rc = self.tbox.cmd_push(None)

            self.assertEqual(rc, 0)
            expected = os.path.join(tmpdir, self.tbox.safe_filename("work"))
            self.assertTrue(os.path.exists(expected))
            with open(expected, "r", encoding="utf-8") as f:
                data = json.load(f)
            self.assertEqual(data.get("name"), "work")

    def test_cmd_drop_removes_selected_entry(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            path = os.path.join(tmpdir, "sess.json")
            with open(path, "w", encoding="utf-8") as f:
                json.dump({"name": "sess", "windows": []}, f)
            entry = {"name": "sess", "path": path, "mtime": 0.0, "windows_count": 0}

            with mock.patch.object(self.tbox, "load_saved_sessions", return_value=[entry]), \
                mock.patch.object(self.tbox, "choose_entry", return_value=entry):
                rc = self.tbox.cmd_drop()

            self.assertEqual(rc, 0)
            self.assertFalse(os.path.exists(path))

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


if __name__ == "__main__":
    unittest.main()

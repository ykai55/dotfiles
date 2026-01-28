import importlib.machinery
import importlib.util
import io
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


def load_tmux_dump_module():
    tmux_dump_path = pathlib.Path(__file__).resolve().parents[1] / "tmux-dump"
    spec = importlib.util.spec_from_file_location(
        "tmux_dump",
        tmux_dump_path,
        loader=importlib.machinery.SourceFileLoader("tmux_dump", str(tmux_dump_path)),
    )
    if not spec or not spec.loader:
        raise RuntimeError(f"Failed to load spec for {tmux_dump_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class TmuxDumpTests(CapturingTestCase):
    def setUp(self):
        super().setUp()
        self.tmux_dump = load_tmux_dump_module()

    def test_dump_includes_automatic_rename(self):
        def run_tmux(args):
            if args[:2] == ["list-sessions", "-F"]:
                return ["$1\tmain\t100\t1\t1\t80\t24"]
            if args[:3] == ["list-windows", "-t", "main"]:
                return [
                    "@1\t0\tw0\t0\t1\tlayout\t0\t80\t24",
                    "@2\t1\tw1\t1\t1\tlayout\t1\t80\t24",
                ]
            if args[:3] == ["list-panes", "-t", "main:0"]:
                return ["%1\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/a"]
            if args[:3] == ["list-panes", "-t", "main:1"]:
                return ["%2\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/b"]
            if args[:5] == ["show-options", "-w", "-t", "main:0", "-v"]:
                return ["on"]
            raise RuntimeError("unexpected args")

        with mock.patch.object(self.tmux_dump, "run_tmux", side_effect=run_tmux), \
            mock.patch.dict(self.tmux_dump.os.environ, {}, clear=True):
            data = self.tmux_dump.tmux_dump()

        window = data["windows"][0]
        self.assertEqual(window["automatic_rename"], "on")
        self.assertFalse(window["zoomed"])
        self.assertTrue(data["windows"][1]["zoomed"])

    def test_dump_handles_missing_automatic_rename(self):
        def run_tmux(args):
            if args[:2] == ["list-sessions", "-F"]:
                return ["$1\tmain\t100\t1\t1\t80\t24"]
            if args[:3] == ["list-windows", "-t", "main"]:
                return ["@1\t0\tw1\t1\t1\tlayout\t0\t80\t24"]
            if args[:3] == ["list-panes", "-t", "main:0"]:
                return ["%1\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/"]
            if args[:5] == ["show-options", "-w", "-t", "main:0", "-v"]:
                raise RuntimeError("no option")
            raise RuntimeError("unexpected args")

        with mock.patch.object(self.tmux_dump, "run_tmux", side_effect=run_tmux), \
            mock.patch.dict(self.tmux_dump.os.environ, {}, clear=True):
            data = self.tmux_dump.tmux_dump()

        window = data["windows"][0]
        self.assertEqual(window["automatic_rename"], "")

    def test_dump_filters_current_session_in_tmux(self):
        def run_tmux(args):
            if args[:2] == ["list-sessions", "-F"]:
                return [
                    "$1\tmain\t100\t1\t1\t80\t24",
                    "$2\tother\t100\t0\t1\t80\t24",
                ]
            if args[:3] == ["list-windows", "-t", "main"]:
                return ["@1\t0\tw1\t1\t1\tlayout\t0\t80\t24"]
            if args[:3] == ["list-panes", "-t", "main:0"]:
                return ["%1\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/"]
            if args[:5] == ["show-options", "-w", "-t", "main:0", "-v"]:
                return ["off"]
            raise RuntimeError("unexpected args")

        with mock.patch.object(self.tmux_dump, "run_tmux", side_effect=run_tmux), \
            mock.patch.object(self.tmux_dump, "current_session_name", return_value="main"), \
            mock.patch.dict(self.tmux_dump.os.environ, {"TMUX": "1"}, clear=True):
            data = self.tmux_dump.tmux_dump()

        self.assertEqual(data["name"], "main")
        self.assertEqual(len(data["windows"]), 1)

    def test_dump_prefers_attached_session(self):
        def run_tmux(args):
            if args[:2] == ["list-sessions", "-F"]:
                return [
                    "$1\tmain\t100\t0\t1\t80\t24",
                    "$2\tother\t100\t1\t1\t80\t24",
                ]
            if args[:3] == ["list-windows", "-t", "other"]:
                return ["@1\t0\tw1\t1\t1\tlayout\t0\t80\t24"]
            if args[:3] == ["list-windows", "-t", "main"]:
                return ["@2\t0\tw2\t0\t1\tlayout\t0\t80\t24"]
            if args[:3] == ["list-panes", "-t", "other:0"]:
                return ["%1\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/"]
            if args[:3] == ["list-panes", "-t", "main:0"]:
                return ["%2\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/"]
            if args[:5] == ["show-options", "-w", "-t", "other:0", "-v"]:
                return ["off"]
            if args[:5] == ["show-options", "-w", "-t", "main:0", "-v"]:
                return ["off"]
            raise RuntimeError("unexpected args")

        with mock.patch.object(self.tmux_dump, "run_tmux", side_effect=run_tmux), \
            mock.patch.dict(self.tmux_dump.os.environ, {}, clear=True):
            data = self.tmux_dump.tmux_dump()

        self.assertEqual(data["name"], "other")
        self.assertTrue(data["attached"])

    def test_dump_uses_target_session(self):
        def run_tmux(args):
            if args[:2] == ["list-sessions", "-F"]:
                return [
                    "$1\tmain\t100\t0\t1\t80\t24",
                    "$2\thost\t100\t1\t1\t80\t24",
                ]
            if args[:3] == ["list-windows", "-t", "host"]:
                return ["@1\t0\tw1\t1\t1\tlayout\t0\t80\t24"]
            if args[:3] == ["list-panes", "-t", "host:0"]:
                return ["%1\t0\ttitle\t1\t0\t80\t24\t0\t0\t\t123\tbash\t\t/"]
            if args[:5] == ["show-options", "-w", "-t", "host:0", "-v"]:
                return ["off"]
            raise RuntimeError("unexpected args")

        with mock.patch.object(self.tmux_dump, "run_tmux", side_effect=run_tmux), \
            mock.patch.dict(self.tmux_dump.os.environ, {}, clear=True):
            data = self.tmux_dump.tmux_dump("host")

        self.assertEqual(data["name"], "host")

    def test_normalize_path_strips_file_scheme(self):
        self.assertEqual(self.tmux_dump.normalize_path("file:///tmp"), "/tmp")
        self.assertEqual(self.tmux_dump.normalize_path("ykai_m4/Users/bytedance"), "/Users/bytedance")
        self.assertEqual(self.tmux_dump.normalize_path("file://ykai_m4/Users/bytedance"), "/Users/bytedance")

    def test_split_command_falls_back_on_invalid(self):
        self.assertEqual(self.tmux_dump.split_command("'oops"), ["'oops"])

    def test_normalize_tty_for_ps(self):
        self.assertEqual(self.tmux_dump.normalize_tty_for_ps("/dev/ttys000"), "ttys000")
        self.assertEqual(self.tmux_dump.normalize_tty_for_ps("ttys001"), "ttys001")

    def test_parse_kv_tsv_pads_missing(self):
        parsed = self.tmux_dump.parse_kv_tsv("a\tb", ["k1", "k2", "k3"])
        self.assertEqual(parsed, {"k1": "a", "k2": "b", "k3": ""})

    def test_main_writes_output_file(self):
        data = {"name": "sess", "windows": []}
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = pathlib.Path(tmpdir) / "out.json"
            argv = ["tmux-dump", str(out_path)]
            with mock.patch.object(self.tmux_dump, "tmux_dump", return_value=data), \
                mock.patch.object(self.tmux_dump.sys, "argv", argv):
                rc = self.tmux_dump.main()

            self.assertEqual(rc, 0)
            with open(out_path, "r", encoding="utf-8") as f:
                loaded = json.load(f)
            self.assertEqual(loaded, data)

    def test_main_writes_compact_output(self):
        data = {"name": "sess", "windows": []}
        expected = json.dumps(data, separators=(",", ":"), ensure_ascii=False)
        with tempfile.TemporaryDirectory() as tmpdir:
            out_path = pathlib.Path(tmpdir) / "out.json"
            argv = ["tmux-dump", "--no-format", str(out_path)]
            with mock.patch.object(self.tmux_dump, "tmux_dump", return_value=data), \
                mock.patch.object(self.tmux_dump.sys, "argv", argv):
                rc = self.tmux_dump.main()

            self.assertEqual(rc, 0)
            with open(out_path, "r", encoding="utf-8") as f:
                raw = f.read().strip()
        self.assertEqual(raw, expected)

    def test_main_requires_output_file(self):
        argv = ["tmux-dump"]
        with mock.patch.object(self.tmux_dump.sys, "argv", argv), \
            mock.patch.object(self.tmux_dump.sys, "stderr", io.StringIO()):
            with self.assertRaises(SystemExit) as ctx:
                self.tmux_dump.main()
        self.assertEqual(ctx.exception.code, 2)


if __name__ == "__main__":
    unittest.main()

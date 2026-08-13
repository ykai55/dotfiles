import json
import os
import pathlib
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
TMUX_DUMP = REPO_ROOT / "bin" / "tmux-dump"
TMUX_LOAD = REPO_ROOT / "bin" / "tmux-load"


@unittest.skipUnless(subprocess.run(["sh", "-c", "command -v tmux"], capture_output=True).returncode == 0, "tmux is required")
class TmuxHelpersIntegrationTests(unittest.TestCase):
    def test_dump_load_round_trip_isolated_server(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            first_dir = root / "first"
            second_dir = root / "second"
            first_dir.mkdir()
            second_dir.mkdir()
            dump_path = root / "dump.json"
            env = os.environ.copy()
            env.pop("TMUX", None)
            env.pop("TMUX_PANE", None)
            env["TMUX_TMPDIR"] = tmpdir

            def tmux(*args):
                return subprocess.run(
                    ["tmux", "-f", "/dev/null", *args],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                ).stdout

            try:
                tmux("new-session", "-d", "-s", "source", "-n", "one\ttab", "-c", str(first_dir))
                tmux("split-window", "-h", "-t", "source:", "-c", str(second_dir))
                tmux("new-window", "-d", "-t", "source:", "-n", "two", "-c", str(first_dir))
                tmux("select-window", "-t", "source:0")

                subprocess.run(
                    [str(TMUX_DUMP), "--session", "source", str(dump_path)],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                data = json.loads(dump_path.read_text(encoding="utf-8"))
                self.assertEqual(data["windows"][0]["name"], "one\ttab")
                self.assertEqual(
                    [pane["path"] for pane in data["windows"][0]["panes"]],
                    [str(first_dir.resolve()), str(second_dir.resolve())],
                )

                subprocess.run(
                    [str(TMUX_LOAD), "--no-run-commands", "--session", "target", str(dump_path)],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                windows = tmux("list-windows", "-t", "target:", "-F", "#{window_index}\t#{window_name}\t#{window_active}")
                panes = tmux("list-panes", "-t", "target:0", "-F", "#{pane_index}\t#{pane_current_path}\t#{pane_active}")
                self.assertEqual(windows.splitlines(), ["0\tone\ttab\t1", "1\ttwo\t0"])
                self.assertEqual(
                    panes.splitlines(),
                    [f"0\t{first_dir.resolve()}\t0", f"1\t{second_dir.resolve()}\t1"],
                )
            finally:
                subprocess.run(
                    ["tmux", "kill-server"],
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )

    def test_restored_shell_command_keeps_pane_after_interrupt(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            dump_path = root / "dump.json"
            env = os.environ.copy()
            env.pop("TMUX", None)
            env.pop("TMUX_PANE", None)
            env["TMUX_TMPDIR"] = tmpdir

            def tmux(*args):
                return subprocess.run(
                    ["tmux", "-f", "/dev/null", *args],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                ).stdout

            try:
                tmux("new-session", "-d", "-s", "source", "--", "bash", "--noprofile", "--norc")
                tmux("send-keys", "-t", "source:", "-l", "sleep 9999")
                tmux("send-keys", "-t", "source:", "Enter")
                subprocess.run(["sleep", "0.2"], check=True)
                subprocess.run(
                    [str(TMUX_DUMP), "--session", "source", str(dump_path)],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                subprocess.run(
                    [str(TMUX_LOAD), "--session", "target", str(dump_path)],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                subprocess.run(["sleep", "0.2"], check=True)
                tmux("send-keys", "-t", "target:", "C-c")
                subprocess.run(["sleep", "0.2"], check=True)

                pane_state = tmux(
                    "display-message",
                    "-p",
                    "-t",
                    "target:",
                    "#{window_panes}:#{pane_dead}",
                ).strip()
                self.assertEqual(pane_state, "1:0")
            finally:
                subprocess.run(
                    ["tmux", "kill-server"],
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )


if __name__ == "__main__":
    unittest.main()

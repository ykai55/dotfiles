import base64
import json
import os
import pathlib
import shlex
import shutil
import subprocess
import tempfile
import time
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
TMUX_DUMP = REPO_ROOT / "bin" / "tmux-dump"
TMUX_LOAD = REPO_ROOT / "bin" / "tmux-load"
TMUX_COMMAND_TRACKER = REPO_ROOT / "fish" / "conf.d" / "tmux_command_tracker.fish"


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

    def test_tracked_fish_command_round_trips_exactly(self):
        fish = shutil.which("fish")
        if not fish:
            self.skipTest("fish is required")

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

            tracker_source = f"set -e fish_private_mode; source {shlex.quote(str(TMUX_COMMAND_TRACKER))}"
            command = "set -lx TRACKED_VALUE 'a b'; sleep 9999"
            try:
                tmux("new-session", "-d", "-s", "source", "--", fish, "-N", "-C", tracker_source)
                tmux("set-option", "-g", "default-shell", fish)
                default_command = f"exec {shlex.quote(fish)} -N -C {shlex.quote(tracker_source)}"
                tmux("set-option", "-g", "default-command", default_command)
                tmux("send-keys", "-t", "source:", "-l", command)
                tmux("send-keys", "-t", "source:", "Enter")
                time.sleep(0.2)

                subprocess.run(
                    [str(TMUX_DUMP), "--session", "source", str(dump_path)],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                data = json.loads(dump_path.read_text(encoding="utf-8"))
                self.assertEqual(
                    data["windows"][0]["panes"][0]["shell_command"],
                    {"command": command, "running": True, "source": "fish"},
                )

                subprocess.run(
                    [str(TMUX_LOAD), "--session", "target", str(dump_path)],
                    env=env,
                    check=True,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                encoded = ""
                for _ in range(20):
                    proc = subprocess.run(
                        [
                            "tmux",
                            "-f",
                            "/dev/null",
                            "show-options",
                            "-p",
                            "-t",
                            "target:",
                            "-v",
                            "@tmux_command_tracker_command",
                        ],
                        env=env,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        text=True,
                    )
                    if proc.returncode == 0:
                        encoded = proc.stdout.strip()
                        break
                    time.sleep(0.1)
                self.assertEqual(base64.b64decode(encoded).decode("utf-8"), command)

                tmux("send-keys", "-t", "target:", "C-c")
                option = None
                for _ in range(20):
                    option = subprocess.run(
                        [
                            "tmux",
                            "-f",
                            "/dev/null",
                            "show-options",
                            "-p",
                            "-t",
                            "target:",
                            "-v",
                            "@tmux_command_tracker_command",
                        ],
                        env=env,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        text=True,
                    )
                    if option.returncode != 0:
                        break
                    time.sleep(0.1)
                self.assertEqual(
                    tmux("display-message", "-p", "-t", "target:", "#{pane_current_command}:#{pane_dead}").strip(),
                    "fish:0",
                )
                self.assertIsNotNone(option)
                self.assertNotEqual(option.returncode, 0)
            finally:
                subprocess.run(
                    ["tmux", "kill-server"],
                    env=env,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                )


if __name__ == "__main__":
    unittest.main()

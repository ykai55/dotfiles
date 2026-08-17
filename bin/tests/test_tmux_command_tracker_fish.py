import base64
import json
import os
import pathlib
import stat
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
TRACKER = REPO_ROOT / "fish" / "conf.d" / "tmux_command_tracker.fish"


@unittest.skipUnless(subprocess.run(["sh", "-c", "command -v fish"], capture_output=True).returncode == 0, "fish is required")
class TmuxCommandTrackerFishTests(unittest.TestCase):
    def test_preexec_records_command_and_postexec_removes_it(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            bindir = root / "bin"
            bindir.mkdir()
            log = root / "tmux.jsonl"
            fake_tmux = bindir / "tmux"
            fake_tmux.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['TMUX_TRACKER_TEST_LOG'], 'a', encoding='utf-8') as f:\n"
                "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n",
                encoding="utf-8",
            )
            fake_tmux.chmod(fake_tmux.stat().st_mode | stat.S_IXUSR)
            env = os.environ.copy()
            env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "")
            env["TMUX"] = "test"
            env["TMUX_PANE"] = "%9"
            env["TMUX_TRACKER_TEST_LOG"] = str(log)
            command = (
                f"source {TRACKER}; "
                "set -e fish_private_mode; "
                "__tmux_command_tracker_preexec \"FOO='a b' vim README.md\"; "
                "__tmux_command_tracker_postexec"
            )

            subprocess.run(
                ["fish", "-N", "-i", "-c", command],
                env=env,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            calls = [json.loads(line) for line in log.read_text(encoding="utf-8").splitlines()]
            command_calls = [call for call in calls if "@tmux_command_tracker_command" in call]
            self.assertEqual(len(command_calls), 2)
            encoded = command_calls[0][-1]
            self.assertEqual(base64.b64decode(encoded).decode("utf-8"), "FOO='a b' vim README.md")
            self.assertIn("-u", command_calls[1])

    def test_preexec_does_not_record_save_command(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            bindir = root / "bin"
            bindir.mkdir()
            log = root / "tmux.jsonl"
            fake_tmux = bindir / "tmux"
            fake_tmux.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['TMUX_TRACKER_TEST_LOG'], 'a', encoding='utf-8') as f:\n"
                "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n",
                encoding="utf-8",
            )
            fake_tmux.chmod(fake_tmux.stat().st_mode | stat.S_IXUSR)
            env = os.environ.copy()
            env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "")
            env["TMUX"] = "test"
            env["TMUX_PANE"] = "%9"
            env["TMUX_TRACKER_TEST_LOG"] = str(log)

            subprocess.run(
                [
                    "fish",
                    "-N",
                    "-i",
                    "-c",
                    f"source {TRACKER}; set -e fish_private_mode; __tmux_command_tracker_preexec 'tbox save work'",
                ],
                env=env,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertFalse(log.exists())

    def test_preexec_does_not_record_leading_space_command(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            bindir = root / "bin"
            bindir.mkdir()
            log = root / "tmux.jsonl"
            fake_tmux = bindir / "tmux"
            fake_tmux.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['TMUX_TRACKER_TEST_LOG'], 'a', encoding='utf-8') as f:\n"
                "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n",
                encoding="utf-8",
            )
            fake_tmux.chmod(fake_tmux.stat().st_mode | stat.S_IXUSR)
            env = os.environ.copy()
            env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "")
            env["TMUX"] = "test"
            env["TMUX_PANE"] = "%9"
            env["TMUX_TRACKER_TEST_LOG"] = str(log)

            subprocess.run(
                [
                    "fish",
                    "-N",
                    "-i",
                    "-c",
                    f"source {TRACKER}; set -e fish_private_mode; __tmux_command_tracker_preexec ' secret command'",
                ],
                env=env,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertFalse(log.exists())

    def test_preexec_does_not_record_fish_private_mode(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            bindir = root / "bin"
            bindir.mkdir()
            log = root / "tmux.jsonl"
            fake_tmux = bindir / "tmux"
            fake_tmux.write_text(
                "#!/usr/bin/env python3\n"
                "import json, os, sys\n"
                "with open(os.environ['TMUX_TRACKER_TEST_LOG'], 'a', encoding='utf-8') as f:\n"
                "    f.write(json.dumps(sys.argv[1:]) + '\\n')\n",
                encoding="utf-8",
            )
            fake_tmux.chmod(fake_tmux.stat().st_mode | stat.S_IXUSR)
            env = os.environ.copy()
            env["PATH"] = str(bindir) + os.pathsep + env.get("PATH", "")
            env["TMUX"] = "test"
            env["TMUX_PANE"] = "%9"
            env["TMUX_TRACKER_TEST_LOG"] = str(log)

            subprocess.run(
                [
                    "fish",
                    "-N",
                    "-i",
                    "-c",
                    f"source {TRACKER}; set -g fish_private_mode 1; __tmux_command_tracker_preexec 'secret command'",
                ],
                env=env,
                check=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

            self.assertFalse(log.exists())


if __name__ == "__main__":
    unittest.main()

import os
import pathlib
import stat
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SCREENSHOT_REGION = REPO_ROOT / "bin" / "screenshot-region"


class ScreenshotRegionTests(unittest.TestCase):
    def setUp(self):
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.bin_dir = pathlib.Path(self.tempdir.name) / "bin"
        self.bin_dir.mkdir()
        self.log_path = pathlib.Path(self.tempdir.name) / "commands.log"
        self.clipboard_path = pathlib.Path(self.tempdir.name) / "clipboard.png"

    def write_command(self, name, body):
        path = self.bin_dir / name
        path.write_text("#!/usr/bin/env bash\n" + body, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)

    def run_script(self):
        env = os.environ.copy()
        env["PATH"] = f"{self.bin_dir}:{env['PATH']}"
        env["COMMAND_LOG"] = str(self.log_path)
        env["CLIPBOARD_PATH"] = str(self.clipboard_path)
        return subprocess.run(
            [str(SCREENSHOT_REGION)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )

    def test_captures_selected_region_as_png(self):
        self.write_command("slurp", "printf '10,20 300x200\\n'\n")
        self.write_command(
            "grim",
            "printf 'grim:%s\\n' \"$*\" >> \"$COMMAND_LOG\"\nprintf 'png-data'\n",
        )
        self.write_command(
            "wl-copy",
            "printf 'wl-copy:%s\\n' \"$*\" >> \"$COMMAND_LOG\"\ncat > \"$CLIPBOARD_PATH\"\n",
        )

        proc = self.run_script()

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(
            self.log_path.read_text(encoding="utf-8"),
            "grim:-g 10,20 300x200 -\nwl-copy:--type image/png\n",
        )
        self.assertEqual(self.clipboard_path.read_bytes(), b"png-data")

    def test_cancel_does_not_replace_clipboard(self):
        self.write_command("slurp", "exit 1\n")
        self.write_command("grim", "printf 'called' >> \"$COMMAND_LOG\"\n")
        self.write_command("wl-copy", "printf 'called' >> \"$COMMAND_LOG\"\n")

        proc = self.run_script()

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertFalse(self.log_path.exists())


if __name__ == "__main__":
    unittest.main()

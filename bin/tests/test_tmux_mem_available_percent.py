import pathlib
import subprocess
import tempfile
import unittest


class TmuxMemAvailablePercentTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-mem-available-percent"

    def test_prints_two_digit_used_percent(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        meminfo = pathlib.Path(tempdir.name) / "meminfo"
        meminfo.write_text("MemTotal: 1000 kB\nMemAvailable: 70 kB\n", encoding="utf-8")

        result = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={"TMUX_MEMINFO_PATH": str(meminfo)},
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "#[fg=#d70000]93%#[fg=#8a8a8a]")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

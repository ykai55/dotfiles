import os
import pathlib
import subprocess
import tempfile
import unittest


class TmuxCpuPercentTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-cpu-percent"

    def run_cpu_percent(self, command_output):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        command = pathlib.Path(tempdir.name) / "cpu.sh"
        command.write_text(f"#!/usr/bin/env bash\nprintf '%s' '{command_output}'\n", encoding="utf-8")
        command.chmod(0o755)

        env = os.environ.copy()
        env["TMUX_CPU_PERCENT_COMMAND"] = str(command)
        return subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

    def test_prints_two_digit_integer_percent_with_leading_zero(self):
        result = self.run_cpu_percent("7.5%")

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "08%")
        self.assertEqual(result.stderr, "")

    def test_prints_red_style_when_at_least_90_percent(self):
        result = self.run_cpu_percent("90.1%")

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "#[fg=#d70000]90%#[fg=#8a8a8a]")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

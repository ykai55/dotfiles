import os
import pathlib
import stat
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

    def run_with_fake_commands(self, commands):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        bin_dir = pathlib.Path(tempdir.name)

        for name, body in commands.items():
            path = bin_dir / name
            path.write_text(body, encoding="utf-8")
            path.chmod(path.stat().st_mode | stat.S_IXUSR)

        env = os.environ.copy()
        env.pop("TMUX_CPU_PERCENT_COMMAND", None)
        env["PATH"] = f"{bin_dir}{os.pathsep}/bin{os.pathsep}/usr/bin"
        env["TMUX_CPU_PERCENT_CACHE_DIR"] = str(bin_dir / "cache")
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

    def test_uses_macos_iostat_without_external_plugin(self):
        result = self.run_with_fake_commands(
            {
                "uname": "#!/usr/bin/env bash\nprintf 'Darwin\\n'\n",
                "iostat": "#!/usr/bin/env bash\nprintf '          disk0           cpu     load average\\n'\nprintf '    KB/t  tps  MB/s  us sy id   1m   5m   15m\\n'\nprintf '    0.00    0  0.00  10 20 70  1.0  1.0  1.0\\n'\n",
            }
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "30%")
        self.assertEqual(result.stderr, "")

    def test_uses_linux_iostat_without_external_plugin(self):
        result = self.run_with_fake_commands(
            {
                "uname": "#!/usr/bin/env bash\nprintf 'Linux\\n'\n",
                "iostat": "#!/usr/bin/env bash\nprintf 'avg-cpu:  %%user   %%nice %%system %%iowait  %%steal   %%idle\\n'\nprintf '          15.00    0.00   10.00    0.00    0.00   75.00\\n'\n",
            }
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "25%")
        self.assertEqual(result.stderr, "")

    def test_falls_back_to_ps_when_iostat_is_missing(self):
        result = self.run_with_fake_commands(
            {
                "uname": "#!/usr/bin/env bash\nprintf 'Linux\\n'\n",
                "nproc": "#!/usr/bin/env bash\nprintf '4\\n'\n",
                "ps": "#!/usr/bin/env bash\nprintf '10.0\\n20.0\\n30.0\\n40.0\\n'\n",
            }
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "25%")
        self.assertEqual(result.stderr, "")

    def test_reuses_cached_iostat_output(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        root = pathlib.Path(tempdir.name)
        bin_dir = root / "bin"
        cache_dir = root / "cache"
        bin_dir.mkdir()
        calls = root / "iostat-calls"

        uname = bin_dir / "uname"
        uname.write_text("#!/usr/bin/env bash\nprintf 'Darwin\\n'\n", encoding="utf-8")
        uname.chmod(uname.stat().st_mode | stat.S_IXUSR)

        iostat = bin_dir / "iostat"
        iostat.write_text(
            "#!/usr/bin/env bash\n"
            f"printf x >> '{calls}'\n"
            "printf '          disk0           cpu     load average\\n'\n"
            "printf '    KB/t  tps  MB/s  us sy id   1m   5m   15m\\n'\n"
            "printf '    0.00    0  0.00  10 20 70  1.0  1.0  1.0\\n'\n",
            encoding="utf-8",
        )
        iostat.chmod(iostat.stat().st_mode | stat.S_IXUSR)

        env = os.environ.copy()
        env.pop("TMUX_CPU_PERCENT_COMMAND", None)
        env["PATH"] = f"{bin_dir}{os.pathsep}/bin{os.pathsep}/usr/bin"
        env["TMUX_CPU_PERCENT_CACHE_DIR"] = str(cache_dir)

        first = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )
        second = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

        self.assertEqual(first.returncode, 0)
        self.assertEqual(second.returncode, 0)
        self.assertEqual(first.stdout, "30%")
        self.assertEqual(second.stdout, "30%")
        self.assertEqual(calls.read_text(encoding="utf-8"), "x")


if __name__ == "__main__":
    unittest.main()

import os
import pathlib
import stat
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
        temp_path = pathlib.Path(tempdir.name)
        meminfo = temp_path / "meminfo"
        meminfo.write_text("MemTotal: 1000 kB\nMemAvailable: 70 kB\n", encoding="utf-8")
        uname = temp_path / "uname"
        uname.write_text("#!/usr/bin/env bash\nprintf 'Linux\\n'\n", encoding="utf-8")
        uname.chmod(uname.stat().st_mode | stat.S_IXUSR)

        result = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={
                "PATH": f"{temp_path}{os.pathsep}/bin{os.pathsep}/usr/bin",
                "TMUX_MEMINFO_PATH": str(meminfo),
            },
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "#[fg=#d70000]93%#[fg=#8a8a8a]")
        self.assertEqual(result.stderr, "")

    def test_prints_macos_used_percent_from_vm_stat(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        bin_dir = pathlib.Path(tempdir.name)

        commands = {
            "uname": "#!/usr/bin/env bash\nprintf 'Darwin\\n'\n",
            "sysctl": "#!/usr/bin/env bash\nprintf '1000\\n'\n",
            "vm_stat": "#!/usr/bin/env bash\nprintf 'Mach Virtual Memory Statistics: (page size of 100 bytes)\\nPages free: 1.\\nPages inactive: 1.\\nPages speculative: 1.\\nPages active: 7.\\n'\n",
        }
        for name, body in commands.items():
            path = bin_dir / name
            path.write_text(body, encoding="utf-8")
            path.chmod(path.stat().st_mode | stat.S_IXUSR)

        env = os.environ.copy()
        env.pop("TMUX_MEMINFO_PATH", None)
        env["PATH"] = f"{bin_dir}{os.pathsep}/bin{os.pathsep}/usr/bin"

        result = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "70%")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

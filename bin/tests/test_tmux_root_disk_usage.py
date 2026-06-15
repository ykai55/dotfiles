import os
import pathlib
import stat
import subprocess
import tempfile
import unittest


class TmuxRootDiskUsageTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-root-disk-usage"
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.bin_dir = pathlib.Path(self.tempdir.name)
        self.df = self.bin_dir / "df"

    def run_script(self):
        env = os.environ.copy()
        env["PATH"] = f"{self.bin_dir}{os.pathsep}/bin{os.pathsep}/usr/bin"
        return subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

    def write_df(self, body):
        self.df.write_text(body, encoding="utf-8")
        self.df.chmod(self.df.stat().st_mode | stat.S_IXUSR)

    def test_prints_root_disk_used_percent(self):
        self.write_df(
            "#!/usr/bin/env bash\n"
            "printf 'Filesystem      Size  Used Avail Use%% Mounted on\\n'\n"
            "printf '/dev/disk3s1s1  460G  334G  126G  73%% /\\n'\n"
        )

        result = self.run_script()

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "73%")
        self.assertEqual(result.stderr, "")

    def test_prints_placeholder_when_df_fails(self):
        self.write_df("#!/usr/bin/env bash\nexit 1\n")

        result = self.run_script()

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "--")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

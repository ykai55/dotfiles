import os
import pathlib
import stat
import subprocess
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
CLS = REPO_ROOT / "bin" / "cls"


class ClsCommandTests(unittest.TestCase):
    def test_cls_is_executable(self):
        self.assertTrue(CLS.exists())
        self.assertTrue(CLS.stat().st_mode & stat.S_IXUSR)

    def test_cls_clears_screen_and_scrollback(self):
        env = os.environ.copy()
        env["TERM"] = "xterm"

        proc = subprocess.run(
            [str(CLS)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
        )

        self.assertEqual(proc.returncode, 0, proc.stderr.decode("utf-8", "replace"))
        self.assertIn(b"\x1b[H\x1b[2J", proc.stdout)
        self.assertTrue(proc.stdout.endswith(b"\x1b[3J"))


if __name__ == "__main__":
    unittest.main()

import os
import pathlib
import stat
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
TBOX_BIN = REPO_ROOT / "bin" / "tbox"


def write_executable(path: pathlib.Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class TboxIntegrationTests(unittest.TestCase):
    def test_tmux_snippet_prints_hooks_and_bind(self):
        proc = subprocess.run([str(TBOX_BIN), "tmux-snippet"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.assertEqual(proc.returncode, 0)
        self.assertIn("set-hook -g", proc.stdout)
        self.assertIn("bind W popup", proc.stdout)
        self.assertIn("bind X confirm-before", proc.stdout)
        self.assertIn("kill-session", proc.stdout)

    def test_tmux_snippet_can_use_command_name(self):
        proc = subprocess.run(
            [str(TBOX_BIN), "tmux-snippet", "--tbox-command", "tbox"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.assertEqual(proc.returncode, 0)
        self.assertIn('set -g @tbox_autosave "tbox autosave', proc.stdout)
        self.assertIn('bind W popup -E "tbox select"', proc.stdout)
        self.assertIn('bind X confirm-before', proc.stdout)
        self.assertIn('tbox save #{session_name}', proc.stdout)
        self.assertIn('tmux kill-session -t #{session_name}', proc.stdout)

    def test_select_live_uses_tmux_switch_client(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            bindir = pathlib.Path(tmpdir) / "bin"
            bindir.mkdir(parents=True, exist_ok=True)
            log = pathlib.Path(tmpdir) / "log.txt"
            write_executable(
                bindir / "tmux",
                "#!/usr/bin/env python3\n"
                "import os,sys\n"
                "log=os.environ.get('TBOX_TEST_LOG')\n"
                "if log:\n"
                "  with open(log,'a',encoding='utf-8') as f: f.write('tmux ' + ' '.join(sys.argv[1:]) + '\\n')\n"
                "if sys.argv[1:3]==['list-sessions','-F']:\n"
                "  sys.stdout.write('work\\t2\\n')\n"
                "  sys.exit(0)\n"
                "sys.exit(0)\n",
            )
            env = os.environ.copy()
            env.update(
                {
                    "PATH": str(bindir) + os.pathsep + env.get("PATH", ""),
                    "TBOX_PREFER_LOCAL": "0",
                    "TBOX_TEST_LOG": str(log),
                    "TMUX": "1",
                    "TBOX_DIR": str(pathlib.Path(tmpdir) / "store"),
                }
            )
            proc = subprocess.run([str(TBOX_BIN), "select", "work"], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            self.assertEqual(proc.returncode, 0)
            logged = log.read_text(encoding="utf-8")
            self.assertIn("tmux switch-client -t work", logged)


if __name__ == "__main__":
    unittest.main()

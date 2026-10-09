import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[2] / "scripts" / "setup-gitconfig"


class SetupGitconfigTests(unittest.TestCase):
    def test_preserves_existing_content_and_is_idempotent(self):
        for kind in ("missing", "file", "symlink", "included"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                home = Path(directory)
                source = home / "dotfiles/git/config"
                source.parent.mkdir(parents=True)
                source.write_text("[core]\n\teditor = shared-editor\n")
                target = home / ".gitconfig"
                original = b"# Local settings\n[core]\n\teditor = local-editor\n"
                if kind == "symlink":
                    source.write_bytes(original)
                    target.symlink_to(source)
                elif kind == "included":
                    original = b'[include]\n\tpath = "~/dotfiles/git/config"\n' + original
                    target.write_bytes(original)
                elif kind == "file":
                    target.write_bytes(original)
                else:
                    original = b""
                source_before = source.read_bytes()
                env = {**os.environ, "HOME": directory}
                command = [sys.executable, str(SCRIPT)]
                dry_run = subprocess.run(command + ["--dry-run"], env=env, capture_output=True)
                self.assertEqual(dry_run.returncode, 0, dry_run.stderr)
                self.assertEqual(target.read_bytes() if target.exists() else b"", original)
                self.assertEqual(target.is_symlink(), kind == "symlink")
                self.assertFalse(list(home.glob(".gitconfig.bak*")))
                for _ in range(2):
                    result = subprocess.run(command, env=env, capture_output=True)
                    self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(target.is_symlink())
                self.assertTrue(target.read_bytes().endswith(original))
                self.assertEqual(target.read_bytes().count(b"~/dotfiles/git/config"), 1)
                self.assertEqual(source.read_bytes(), source_before)
                backups = list(home.glob(".gitconfig.bak*"))
                self.assertEqual(len(backups), int(kind in ("file", "symlink")))
                if backups:
                    self.assertFalse(backups[0].is_symlink())
                    self.assertEqual(backups[0].read_bytes(), original)
                if original:
                    value = subprocess.run(
                        ["git", "config", "--file", str(target), "--includes", "--get", "core.editor"],
                        env=env, capture_output=True, text=True,
                    )
                    self.assertEqual(value.stdout.strip(), "local-editor")

    def test_invalid_config_and_dangling_symlink_are_preserved(self):
        for kind in ("invalid", "dangling"):
            with self.subTest(kind=kind), tempfile.TemporaryDirectory() as directory:
                target = Path(directory) / ".gitconfig"
                if kind == "invalid":
                    target.write_bytes(b"[broken\n")
                else:
                    target.symlink_to(Path(directory) / "missing")
                result = subprocess.run(
                    [sys.executable, str(SCRIPT)], env={**os.environ, "HOME": directory}, capture_output=True,
                )
                self.assertNotEqual(result.returncode, 0)
                if kind == "invalid":
                    self.assertEqual(target.read_bytes(), b"[broken\n")
                else:
                    self.assertTrue(target.is_symlink())

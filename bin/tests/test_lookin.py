import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest


class LookinWrapperTests(unittest.TestCase):
    def setUp(self):
        self.tempdir = tempfile.TemporaryDirectory(prefix="lookin wrapper ")
        self.addCleanup(self.tempdir.cleanup)
        self.root = pathlib.Path(self.tempdir.name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        source = pathlib.Path(__file__).resolve().parents[1] / "lookin"
        shutil.copy2(source, self.bin / "lookin")
        uname = self.bin / "uname"
        uname.write_text('#!/bin/bash\ncase "$1" in -s) echo "$TEST_SYSTEM";; -m) echo "$TEST_ARCH";; esac\n')
        uname.chmod(0o755)
        self.env = dict(os.environ, PATH=str(self.bin) + os.pathsep + os.environ["PATH"],
                        TEST_SYSTEM="Darwin", TEST_ARCH="arm64")

    def test_dispatch_preserves_arguments_and_exit_status_on_both_architectures(self):
        for arch, target in [("arm64", "macos-aarch64"), ("x86_64", "macos-x86_64")]:
            with self.subTest(arch=arch):
                binary = self.bin / ".downloads" / "lookin" / "current" / target / "lookin"
                binary.parent.mkdir(parents=True)
                binary.write_text('#!/bin/bash\nprintf "%s\\n" "$@"\nexit 17\n')
                binary.chmod(0o755)
                self.env["TEST_ARCH"] = arch
                result = subprocess.run([str(self.bin / "lookin"), "find", "label with spaces", "--pretty"],
                                        env=self.env, capture_output=True, text=True)
                self.assertEqual(result.returncode, 17)
                self.assertEqual(result.stdout.splitlines(), ["find", "label with spaces", "--pretty"])

    def test_missing_binary_explains_recovery(self):
        result = subprocess.run([str(self.bin / "lookin"), "--help"],
                                env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("run bin/dotfiles-apply", result.stderr)

    def test_linux_reports_unsupported_platform(self):
        self.env["TEST_SYSTEM"] = "Linux"
        result = subprocess.run([str(self.bin / "lookin")], env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode, 1)
        self.assertIn("unsupported platform", result.stderr)

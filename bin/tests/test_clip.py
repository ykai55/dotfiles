import os
import pathlib
import stat
import subprocess
import tempfile
import textwrap
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE_CLIP = REPO_ROOT / "bin" / "clip"


def write_executable(path: pathlib.Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(content), encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class ClipWrapperTests(unittest.TestCase):
    def make_temp_repo(self) -> pathlib.Path:
        self.tempdir = tempfile.TemporaryDirectory(prefix="clip wrapper ")
        repo_root = pathlib.Path(self.tempdir.name) / "repo root"
        clip_path = repo_root / "bin" / "clip"
        clip_path.parent.mkdir(parents=True, exist_ok=True)
        clip_path.write_text(SOURCE_CLIP.read_text(encoding="utf-8"), encoding="utf-8")
        clip_path.chmod(clip_path.stat().st_mode | stat.S_IXUSR)
        return repo_root

    def tearDown(self) -> None:
        tempdir = getattr(self, "tempdir", None)
        if tempdir is not None:
            tempdir.cleanup()
            del self.tempdir
        super().tearDown()

    def install_fake_uname(self, bindir: pathlib.Path) -> None:
        write_executable(
            bindir / "uname",
            """#!/usr/bin/env bash
            set -euo pipefail
            case "$1" in
              -s)
                printf '%s\\n' "$FAKE_UNAME_S"
                ;;
              -m)
                printf '%s\\n' "$FAKE_UNAME_M"
                ;;
              *)
                echo "unsupported uname args: $*" >&2
                exit 1
                ;;
            esac
            """,
        )

    def install_fake_clip_binary(self, repo_root: pathlib.Path, relpath: str) -> None:
        write_executable(
            repo_root / "bin" / ".downloads" / "clip" / "v1.0.0" / relpath,
            f"""#!/usr/bin/env bash
            set -euo pipefail
            printf 'TARGET={relpath}\\n'
            if [[ -n "${{CLIP_MACOS_HELPER:-}}" ]]; then
              printf 'HELPER=%s\\n' "$CLIP_MACOS_HELPER"
            fi
            for arg in "$@"; do
              printf 'ARG=%s\\n' "$arg"
            done
            """,
        )
        current = repo_root / "bin" / ".downloads" / "clip" / "current"
        if not current.exists():
            current.symlink_to("v1.0.0")

    def run_wrapper(
        self,
        system: str,
        arch: str,
        *args: str,
        binaries: tuple[str, ...] = (),
    ) -> subprocess.CompletedProcess[str]:
        repo_root = self.make_temp_repo()
        bindir = pathlib.Path(self.tempdir.name) / "bin"
        bindir.mkdir(parents=True, exist_ok=True)
        self.install_fake_uname(bindir)
        for binary in binaries:
            self.install_fake_clip_binary(repo_root, binary)

        env = os.environ.copy()
        env.update(
            {
                "PATH": str(bindir) + os.pathsep + env.get("PATH", ""),
                "FAKE_UNAME_S": system,
                "FAKE_UNAME_M": arch,
            }
        )
        return subprocess.run(
            [str(repo_root / "bin" / "clip"), *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )

    def test_macos_wrapper_uses_macos_binary_and_preserves_args(self):
        proc = self.run_wrapper(
            "Darwin",
            "arm64",
            "set",
            "hello world",
            "--type",
            "text/plain",
            binaries=("macos-aarch64/clip", "macos-aarch64/clip-macos-helper"),
        )

        self.assertEqual(proc.returncode, 0)
        lines = proc.stdout.splitlines()
        self.assertTrue(
            lines[1].endswith(
                "bin/.downloads/clip/current/macos-aarch64/clip-macos-helper"
            ),
            lines[1],
        )
        self.assertEqual(
            lines,
            [
                "TARGET=macos-aarch64/clip",
                lines[1],
                "ARG=set",
                "ARG=hello world",
                "ARG=--type",
                "ARG=text/plain",
            ],
        )

    def test_macos_missing_helper_reports_error(self):
        proc = self.run_wrapper(
            "Darwin",
            "arm64",
            "set",
            binaries=("macos-aarch64/clip",),
        )

        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")
        self.assertIn("clip wrapper: missing downloaded clip helper:", proc.stderr)
        self.assertIn(
            "bin/.downloads/clip/current/macos-aarch64/clip-macos-helper",
            proc.stderr,
        )
        self.assertIn("run bin/dotfiles-apply", proc.stderr)

    def test_linux_wrapper_uses_linux_binary(self):
        proc = self.run_wrapper(
            "Linux",
            "x86_64",
            "get",
            "--output",
            "/tmp/out.txt",
            binaries=("linux-x86_64-musl/clip",),
        )

        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout.splitlines(),
            [
                "TARGET=linux-x86_64-musl/clip",
                "ARG=get",
                "ARG=--output",
                "ARG=/tmp/out.txt",
            ],
        )

    def test_windows_wrapper_uses_windows_binary(self):
        proc = self.run_wrapper(
            "MINGW64_NT-10.0",
            "x86_64",
            "targets",
            "--all",
            binaries=("windows-x86_64-gnu/clip.exe",),
        )

        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout.splitlines(),
            [
                "TARGET=windows-x86_64-gnu/clip.exe",
                "ARG=targets",
                "ARG=--all",
            ],
        )

    def test_unsupported_platform_reports_error(self):
        proc = self.run_wrapper("FreeBSD", "x86_64", "get")

        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")
        self.assertIn("clip wrapper: unsupported platform: FreeBSD", proc.stderr)

    def test_missing_binary_reports_error(self):
        proc = self.run_wrapper("Darwin", "arm64", "types")

        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")
        self.assertIn("clip wrapper: missing downloaded clip binary:", proc.stderr)
        self.assertIn("bin/.downloads/clip/current/macos-aarch64/clip", proc.stderr)
        self.assertIn("run bin/dotfiles-apply", proc.stderr)


if __name__ == "__main__":
    unittest.main()

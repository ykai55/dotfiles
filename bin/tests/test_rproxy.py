import os
import pathlib
import stat
import subprocess
import tempfile
import textwrap
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE_RPROXY = REPO_ROOT / "bin" / "rproxy"


def write_executable(path: pathlib.Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(textwrap.dedent(content), encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR)


class RproxyWrapperTests(unittest.TestCase):
    def make_temp_repo(self) -> pathlib.Path:
        self.tempdir = tempfile.TemporaryDirectory(prefix="rproxy wrapper ")
        repo_root = pathlib.Path(self.tempdir.name) / "repo root"
        rproxy_path = repo_root / "bin" / "rproxy"
        rproxy_path.parent.mkdir(parents=True, exist_ok=True)
        rproxy_path.write_text(SOURCE_RPROXY.read_text(encoding="utf-8"), encoding="utf-8")
        rproxy_path.chmod(rproxy_path.stat().st_mode | stat.S_IXUSR)
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
                printf '%s\n' "$FAKE_UNAME_S"
                ;;
              -m)
                printf '%s\n' "$FAKE_UNAME_M"
                ;;
              *)
                echo "unsupported uname args: $*" >&2
                exit 1
                ;;
            esac
            """,
        )

    def install_fake_rproxy_binary(self, repo_root: pathlib.Path, relpath: str) -> None:
        write_executable(
            repo_root / "bin" / ".downloads" / "rproxy" / "v1.0.0" / relpath,
            f"""#!/usr/bin/env bash
            set -euo pipefail
            printf 'TARGET={relpath}\n'
            for arg in "$@"; do
              printf 'ARG=%s\n' "$arg"
            done
            """,
        )
        current = repo_root / "bin" / ".downloads" / "rproxy" / "current"
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
            self.install_fake_rproxy_binary(repo_root, binary)

        env = os.environ.copy()
        env.update(
            {
                "PATH": str(bindir) + os.pathsep + env.get("PATH", ""),
                "FAKE_UNAME_S": system,
                "FAKE_UNAME_M": arch,
            }
        )
        return subprocess.run(
            [str(repo_root / "bin" / "rproxy"), *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )

    def test_linux_wrapper_uses_downloaded_binary(self):
        proc = self.run_wrapper(
            "Linux",
            "x86_64",
            "client",
            "--help",
            binaries=("linux-x86_64-gnu/rproxy",),
        )

        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout.splitlines(),
            [
                "TARGET=linux-x86_64-gnu/rproxy",
                "ARG=client",
                "ARG=--help",
            ],
        )

    def test_macos_wrapper_uses_downloaded_binary(self):
        proc = self.run_wrapper(
            "Darwin",
            "arm64",
            "server",
            "--help",
            binaries=("macos-aarch64/rproxy",),
        )

        self.assertEqual(proc.returncode, 0)
        self.assertEqual(
            proc.stdout.splitlines(),
            [
                "TARGET=macos-aarch64/rproxy",
                "ARG=server",
                "ARG=--help",
            ],
        )

    def test_missing_binary_reports_error(self):
        proc = self.run_wrapper("Linux", "x86_64", "client")

        self.assertEqual(proc.returncode, 1)
        self.assertEqual(proc.stdout, "")
        self.assertIn("rproxy wrapper: missing downloaded rproxy binary:", proc.stderr)
        self.assertIn("bin/.downloads/rproxy/current/linux-x86_64-gnu/rproxy", proc.stderr)
        self.assertIn("run bin/dotfiles-apply", proc.stderr)


if __name__ == "__main__":
    unittest.main()

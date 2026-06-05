import os
import pathlib
import stat
import subprocess
import tempfile
import unittest


class TmuxDockerRunningCountTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-docker-running-count"
        self.tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(self.tempdir.cleanup)
        self.bin_dir = pathlib.Path(self.tempdir.name)
        self.docker = self.bin_dir / "docker"

    def run_script(self, docker_command=None):
        env = os.environ.copy()
        env["TMUX_DOCKER_COMMAND"] = docker_command or str(self.docker)
        return subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

    def write_docker(self, body):
        self.docker.write_text(body, encoding="utf-8")
        self.docker.chmod(self.docker.stat().st_mode | stat.S_IXUSR)

    def test_prints_two_digit_running_container_count(self):
        self.write_docker("#!/bin/bash\nprintf 'abc\\ndef\\n'\n")

        result = self.run_script()

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "02")
        self.assertEqual(result.stderr, "")

    def test_prints_placeholder_when_docker_is_missing(self):
        result = self.run_script(str(self.bin_dir / "missing-docker"))

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "--")
        self.assertEqual(result.stderr, "")

    def test_prints_placeholder_when_docker_command_fails(self):
        self.write_docker("#!/bin/bash\nexit 1\n")

        result = self.run_script()

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "--")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

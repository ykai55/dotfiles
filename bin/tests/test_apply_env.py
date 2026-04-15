import os
import pathlib
import subprocess
import tempfile
import textwrap
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
APPLY_ENV_FISH = REPO_ROOT / "fish" / "functions" / "apply_env.fish"


class ApplyEnvTests(unittest.TestCase):
    def write_script(self, path: pathlib.Path, content: str) -> None:
        path.write_text(textwrap.dedent(content), encoding="utf-8")

    def run_fish(self, command: str, *args: str) -> subprocess.CompletedProcess[str]:
        env = os.environ.copy()
        return subprocess.run(
            ["fish", "-N", "-c", command, str(APPLY_ENV_FISH), *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
        )

    def run_apply_env(
        self,
        script_path: pathlib.Path | None = None,
        *script_args: str,
        before_commands: str = "",
        report_commands: str = "",
    ) -> subprocess.CompletedProcess[str]:
        command_parts = [
            "source $argv[1]",
        ]
        if before_commands:
            command_parts.append(textwrap.dedent(before_commands).strip())
        command_parts.extend(
            [
                "if test (count $argv) -ge 2",
                "  apply_env $argv[2] $argv[3..-1]",
                "else",
                "  apply_env",
                "end",
                "set apply_env_status $status",
                "printf 'STATUS=%s\\n' $apply_env_status",
            ]
        )
        if report_commands:
            command_parts.append(textwrap.dedent(report_commands).strip())

        args = []
        if script_path is not None:
            args.append(str(script_path))
        args.extend(script_args)
        return self.run_fish("\n".join(command_parts), *args)

    def parse_output(self, stdout: str) -> dict[str, str]:
        values: dict[str, str] = {}
        for line in stdout.splitlines():
            if "=" not in line:
                continue
            key, value = line.split("=", 1)
            values[key] = value
        return values

    def test_requires_script_path(self):
        proc = self.run_apply_env()
        values = self.parse_output(proc.stdout)

        self.assertEqual(proc.returncode, 0)
        self.assertEqual(values["STATUS"], "1")
        self.assertEqual(proc.stdout.strip(), "STATUS=1")
        self.assertIn("Usage: apply_env <path_to_script.sh>", proc.stderr)

    def test_rejects_missing_script(self):
        proc = self.run_apply_env(pathlib.Path("/tmp/does-not-exist.sh"))
        values = self.parse_output(proc.stdout)

        self.assertEqual(proc.returncode, 0)
        self.assertEqual(values["STATUS"], "1")
        self.assertIn("Error: File not found", proc.stderr)

    def test_applies_added_variable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "set-var.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_TEST=ok
                """,
            )

            proc = self.run_apply_env(
                script_path,
                report_commands="""
                if set -q APPLY_ENV_TEST
                  printf 'APPLY_ENV_TEST=%s\\n' "$APPLY_ENV_TEST"
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["APPLY_ENV_TEST"], "ok")

    def test_updates_existing_variable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "update-var.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_TEST=new
                """,
            )

            proc = self.run_apply_env(
                script_path,
                before_commands='set -gx APPLY_ENV_TEST old',
                report_commands="""
                if set -q APPLY_ENV_TEST
                  printf 'APPLY_ENV_TEST=%s\\n' "$APPLY_ENV_TEST"
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["APPLY_ENV_TEST"], "new")

    def test_unsets_removed_variable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "unset-var.sh"
            self.write_script(
                script_path,
                """
                unset APPLY_ENV_TEST
                export KEEP_ME=ok
                """,
            )

            proc = self.run_apply_env(
                script_path,
                before_commands='set -gx APPLY_ENV_TEST old',
                report_commands="""
                if set -q APPLY_ENV_TEST
                  printf 'APPLY_ENV_TEST_SET=1\\n'
                else
                  printf 'APPLY_ENV_TEST_SET=0\\n'
                end
                if set -q KEEP_ME
                  printf 'KEEP_ME=%s\\n' "$KEEP_ME"
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["APPLY_ENV_TEST_SET"], "0")
        self.assertEqual(values["KEEP_ME"], "ok")

    def test_preserves_values_with_equals_signs(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "equals.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_TEST='a=b=c'
                """,
            )

            proc = self.run_apply_env(
                script_path,
                report_commands="""
                if set -q APPLY_ENV_TEST
                  printf 'APPLY_ENV_TEST=%s\\n' "$APPLY_ENV_TEST"
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["APPLY_ENV_TEST"], "a=b=c")

    def test_passes_script_arguments_verbatim(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "args.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_ARG1="$1"
                export APPLY_ENV_ARG2="$2"
                """,
            )

            proc = self.run_apply_env(
                script_path,
                "x y",
                "a'b",
                report_commands="""
                printf 'APPLY_ENV_ARG1=%s\\n' "$APPLY_ENV_ARG1"
                printf 'APPLY_ENV_ARG2=%s\\n' "$APPLY_ENV_ARG2"
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["APPLY_ENV_ARG1"], "x y")
        self.assertEqual(values["APPLY_ENV_ARG2"], "a'b")

    def test_supports_script_path_with_spaces(self):
        with tempfile.TemporaryDirectory(prefix="apply env ") as tmpdir:
            script_path = pathlib.Path(tmpdir) / "test script.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_TEST=ok
                """,
            )

            proc = self.run_apply_env(
                script_path,
                report_commands="""
                if set -q APPLY_ENV_TEST
                  printf 'APPLY_ENV_TEST=%s\\n' "$APPLY_ENV_TEST"
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["APPLY_ENV_TEST"], "ok")

    def test_returns_source_failure_status(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "fail.sh"
            self.write_script(
                script_path,
                """
                return 7
                """,
            )

            proc = self.run_apply_env(script_path)
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "7")

    def test_does_not_apply_changes_when_source_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "partial-fail.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_TEST=partial
                return 7
                """,
            )

            proc = self.run_apply_env(
                script_path,
                report_commands="""
                if set -q APPLY_ENV_TEST
                  printf 'APPLY_ENV_TEST_SET=1\\n'
                  printf 'APPLY_ENV_TEST=%s\\n' "$APPLY_ENV_TEST"
                else
                  printf 'APPLY_ENV_TEST_SET=0\\n'
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "7")
        self.assertEqual(values["APPLY_ENV_TEST_SET"], "0")

    def test_does_not_leak_internal_marker_variable(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "marker.sh"
            self.write_script(
                script_path,
                """
                export APPLY_ENV_TEST=ok
                """,
            )

            proc = self.run_apply_env(
                script_path,
                report_commands="""
                if set -q FROM_FISH_APPLY_ENV
                  printf 'FROM_FISH_APPLY_ENV_SET=1\\n'
                else
                  printf 'FROM_FISH_APPLY_ENV_SET=0\\n'
                end
                """,
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["FROM_FISH_APPLY_ENV_SET"], "0")

    def test_applies_path_changes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            script_path = pathlib.Path(tmpdir) / "path.sh"
            self.write_script(
                script_path,
                """
                export PATH="/tmp/one:/tmp/two:$PATH"
                """,
            )

            proc = self.run_apply_env(
                script_path,
                before_commands='set -gx PATH /usr/bin /bin',
                report_commands='printf "PATH=%s\\n" (string join : $PATH)',
            )
            values = self.parse_output(proc.stdout)

        self.assertEqual(values["STATUS"], "0")
        self.assertEqual(values["PATH"], "/tmp/one:/tmp/two:/usr/bin:/bin")


if __name__ == "__main__":
    unittest.main()

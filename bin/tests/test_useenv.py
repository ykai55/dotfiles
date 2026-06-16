import os
import pathlib
import subprocess
import sys
import tempfile
import textwrap
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
USEENV = REPO_ROOT / "bin" / "useenv"


class UseEnvTests(unittest.TestCase):
    def write_config(self, path: pathlib.Path, content: str) -> None:
        path.write_text(textwrap.dedent(content), encoding="utf-8")

    def run_useenv(
        self,
        *args: str,
        env: dict[str, str] | None = None,
    ) -> subprocess.CompletedProcess[str]:
        child_env = os.environ.copy()
        if env:
            child_env.update(env)
        return subprocess.run(
            [str(USEENV), *args],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=child_env,
        )

    def parse_env(self, stdout: str) -> dict[str, str]:
        values: dict[str, str] = {}
        for line in stdout.splitlines():
            if "=" not in line:
                continue
            key, value = line.split("=", 1)
            values[key] = value
        return values

    def test_prints_environment_after_loading_config(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = pathlib.Path(tmpdir) / "config.fish"
            self.write_config(
                config_path,
                """
                set -gx USEENV_TEST_ADDED fish
                set -gx USEENV_TEST_INHERITED "$USEENV_TEST_INHERITED:fish"
                """,
            )

            proc = self.run_useenv(
                "--fish-config",
                str(config_path),
                env={"USEENV_TEST_INHERITED": "base"},
            )
            values = self.parse_env(proc.stdout)

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(values["USEENV_TEST_ADDED"], "fish")
        self.assertEqual(values["USEENV_TEST_INHERITED"], "base:fish")

    def test_runs_command_with_loaded_environment(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = pathlib.Path(tmpdir) / "config.fish"
            self.write_config(
                config_path,
                """
                set -gx USEENV_TEST_COMMAND loaded
                """,
            )

            proc = self.run_useenv(
                "--fish-config",
                str(config_path),
                "--",
                sys.executable,
                "-c",
                "import os; print(os.environ['USEENV_TEST_COMMAND'])",
            )

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.strip(), "loaded")

    def test_preserves_command_arguments_verbatim(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = pathlib.Path(tmpdir) / "config.fish"
            self.write_config(config_path, "set -gx USEENV_TEST_COMMAND loaded\n")

            proc = self.run_useenv(
                "--fish-config",
                str(config_path),
                sys.executable,
                "-c",
                "import sys; print(sys.argv[1]); print(sys.argv[2])",
                "x y",
                "a'b",
            )

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.splitlines(), ["x y", "a'b"])

    def test_config_can_use_repo_fish_functions(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp_path = pathlib.Path(tmpdir)
            config_path = tmp_path / "config.fish"
            bash_script = tmp_path / "set-env.sh"
            bash_script.write_text("export USEENV_TEST_FROM_APPLY=applied\n", encoding="utf-8")
            self.write_config(
                config_path,
                f"""
                apply_env {bash_script}
                set -gx USEENV_TEST_AFTER_APPLY $USEENV_TEST_FROM_APPLY
                """,
            )

            proc = self.run_useenv("--fish-config", str(config_path))
            values = self.parse_env(proc.stdout)

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(values["USEENV_TEST_AFTER_APPLY"], "applied")

    def test_loads_config_in_noninteractive_fish(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = pathlib.Path(tmpdir) / "config.fish"
            self.write_config(
                config_path,
                """
                if status is-interactive
                    set -gx USEENV_TEST_INTERACTIVE yes
                else
                    set -gx USEENV_TEST_NONINTERACTIVE yes
                end
                """,
            )

            proc = self.run_useenv("--fish-config", str(config_path))
            values = self.parse_env(proc.stdout)

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertNotIn("USEENV_TEST_INTERACTIVE", values)
        self.assertEqual(values["USEENV_TEST_NONINTERACTIVE"], "yes")

    def test_isolated_mode_clears_inherited_environment_before_loading_config(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = pathlib.Path(tmpdir) / "config.fish"
            self.write_config(
                config_path,
                """
                if set -q USEENV_TEST_INHERITED
                    set -gx USEENV_TEST_SAW_INHERITED yes
                end
                set -gx USEENV_TEST_FROM_CONFIG loaded
                """,
            )

            proc = self.run_useenv(
                "-i",
                "--fish-config",
                str(config_path),
                env={"USEENV_TEST_INHERITED": "base"},
            )
            values = self.parse_env(proc.stdout)

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertNotIn("USEENV_TEST_INHERITED", values)
        self.assertNotIn("USEENV_TEST_SAW_INHERITED", values)
        self.assertEqual(values["USEENV_TEST_FROM_CONFIG"], "loaded")

    def test_isolated_mode_runs_command_without_inherited_environment(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            config_path = pathlib.Path(tmpdir) / "config.fish"
            self.write_config(config_path, "set -gx USEENV_TEST_FROM_CONFIG loaded\n")

            proc = self.run_useenv(
                "-i",
                "--fish-config",
                str(config_path),
                "--",
                sys.executable,
                "-c",
                (
                    "import os; "
                    "print(os.environ.get('USEENV_TEST_INHERITED', '<missing>')); "
                    "print(os.environ['USEENV_TEST_FROM_CONFIG'])"
                ),
                env={"USEENV_TEST_INHERITED": "base"},
            )

        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stdout.splitlines(), ["<missing>", "loaded"])


if __name__ == "__main__":
    raise SystemExit(unittest.main())

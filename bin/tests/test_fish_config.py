import os
import pathlib
import shutil
import subprocess
import tempfile
import unittest


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
FISH_CONFIG = REPO_ROOT / "fish" / "config.fish"


class FishConfigTests(unittest.TestCase):
    def run_config(
        self,
        interactive: bool = False,
        custom_root: bool = False,
        create_shims: bool = True,
        platform: str = "Linux",
        useenv: bool = False,
        import_bashrc: bool = False,
    ) -> subprocess.CompletedProcess[str]:
        with tempfile.TemporaryDirectory() as tmpdir:
            env = os.environ.copy()
            env.update(
                HOME=tmpdir,
                XDG_CONFIG_HOME=tmpdir,
                XDG_DATA_HOME=tmpdir,
                PATH="/usr/bin:/bin",
                TERM="xterm-256color",
                FISH_CONFIG_TEST_PLATFORM=platform,
            )
            env.pop("PYENV_ROOT", None)
            env.pop("PYENV_SHELL", None)
            env.pop("USEENV_SKIP_BREW", None)
            env.pop("USEENV", None)
            if useenv:
                env["USEENV"] = "1"
            pyenv_root = pathlib.Path(tmpdir) / ("custom root" if custom_root else ".pyenv")
            if custom_root:
                env["PYENV_ROOT"] = str(pyenv_root)
            if create_shims:
                (pyenv_root / "shims").mkdir(parents=True)
            if import_bashrc:
                (pathlib.Path(tmpdir) / ".bashrc").write_text(
                    'export FISH_CONFIG_TEST_SKIP_BREW="${USEENV_SKIP_BREW:-unset}"\n'
                )
            config_path = FISH_CONFIG
            if platform == "Darwin":
                brew = pathlib.Path(tmpdir) / "brew"
                brew.write_text("#!/bin/sh\nprintf 'set -gx HOMEBREW_PREFIX test\\n'\n")
                brew.chmod(0o755)
                config_path = pathlib.Path(tmpdir) / "config.fish"
                config_path.write_text(
                    FISH_CONFIG.read_text().replace("/opt/homebrew/bin/brew", f'"{brew}"')
                )
            command = """
                function uname
                    echo uname >> "$HOME/uname-calls"
                    echo "$FISH_CONFIG_TEST_PLATFORM"
                end
                function pyenv
                    string join ' ' -- $argv >> "$HOME/pyenv-calls"
                    set -l root "$HOME/.pyenv"
                    if set -q PYENV_ROOT; and test -n "$PYENV_ROOT"
                        set root "$PYENV_ROOT"
                    end
                    printf 'set -gx PATH "%s/shims" $PATH\\n' "$root"
                    if not contains -- --path $argv
                        echo 'set -gx PYENV_SHELL fish'
                        echo 'set -gx TEST_PYENV_COMPLETIONS loaded'
                    end
                end
                set -l root "$HOME/.pyenv"
                if set -q PYENV_ROOT; and test -n "$PYENV_ROOT"
                    set root "$PYENV_ROOT"
                end
                set -gx PATH "$root/shims" $PATH "$root/shims"
                set -p fish_function_path "$argv[2]"
                source "$argv[1]"
                printf 'SHELL=%s\\n' "$PYENV_SHELL"
                printf 'FIRST_PATH=%s\\n' "$PATH[1]"
                if set -q TEST_PYENV_COMPLETIONS
                    echo COMPLETIONS=loaded
                end
                is_mac; printf 'IS_MAC=%s\\n' $status
                is_linux; printf 'IS_LINUX=%s\\n' $status
                printf 'UNAME_CALLS=%s\\n' (count (cat "$HOME/uname-calls"))
                printf 'SHIMS_COUNT=%s\\n' (count (string match -- "$root/shims" $PATH))
                printf 'SKIP_BREW_SEEN=%s\\n' "$FISH_CONFIG_TEST_SKIP_BREW"
                if set -q USEENV_SKIP_BREW
                    echo SKIP_BREW_LEAKED=yes
                end
                if test -f "$HOME/pyenv-calls"
                    printf 'PYENV_ARGS=%s\\n' (cat "$HOME/pyenv-calls")
                else
                    echo PYENV_ARGS=none
                end
            """
            args = [shutil.which("fish"), "-N"]
            if interactive:
                args.append("-i")
            return subprocess.run(
                [*args, "-c", command, str(config_path), str(REPO_ROOT / "fish/functions")],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=env,
            )

    def test_noninteractive_initializes_pyenv_path_without_completions(self):
        proc = self.run_config()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("PYENV_ARGS=none\n", proc.stdout)
        self.assertIn("SHELL=fish\n", proc.stdout)
        self.assertRegex(proc.stdout, r"FIRST_PATH=.+/shims\n")
        self.assertNotIn("COMPLETIONS=loaded", proc.stdout)
        self.assertIn("SHIMS_COUNT=1\n", proc.stdout)

    def test_noninteractive_respects_custom_pyenv_root(self):
        proc = self.run_config(custom_root=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertRegex(proc.stdout, r"FIRST_PATH=.+/custom root/shims\n")
        self.assertIn("SHIMS_COUNT=1\n", proc.stdout)
        self.assertIn("PYENV_ARGS=none\n", proc.stdout)

    def test_noninteractive_uses_pyenv_init_when_shims_are_missing(self):
        proc = self.run_config(create_shims=False)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("PYENV_ARGS=init --path --no-rehash fish\n", proc.stdout)
        self.assertIn("SHELL=fish\n", proc.stdout)
        self.assertRegex(proc.stdout, r"FIRST_PATH=.+/shims\n")

    def test_interactive_preserves_full_pyenv_initialization(self):
        proc = self.run_config(interactive=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("PYENV_ARGS=init --no-rehash - fish\n", proc.stdout)
        self.assertIn("SHELL=fish\n", proc.stdout)
        self.assertRegex(proc.stdout, r"FIRST_PATH=.+/shims\n")
        self.assertIn("COMPLETIONS=loaded\n", proc.stdout)

    def test_platform_detection_calls_uname_once(self):
        proc = self.run_config()
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("IS_MAC=1\n", proc.stdout)
        self.assertIn("IS_LINUX=0\n", proc.stdout)
        self.assertIn("UNAME_CALLS=1\n", proc.stdout)

    def test_useenv_defers_bash_homebrew_initialization_only_on_mac(self):
        for platform, expected in (("Darwin", "1"), ("Linux", "unset")):
            with self.subTest(platform=platform):
                proc = self.run_config(platform=platform, useenv=True, import_bashrc=True)
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertIn(f"SKIP_BREW_SEEN={expected}\n", proc.stdout)
                self.assertNotIn("SKIP_BREW_LEAKED=yes", proc.stdout)

    def test_regular_shell_keeps_bash_homebrew_initialization(self):
        proc = self.run_config(platform="Darwin", import_bashrc=True)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertIn("SKIP_BREW_SEEN=unset\n", proc.stdout)
        self.assertNotIn("SKIP_BREW_LEAKED=yes", proc.stdout)

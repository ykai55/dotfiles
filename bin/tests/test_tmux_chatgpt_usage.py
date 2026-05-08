import os
import pathlib
import stat
import subprocess
import tempfile
import textwrap
import unittest


class TmuxChatgptUsageTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-chatgpt-usage"

    def run_usage(self, command):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.close()
        os.unlink(cache.name)
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        env = {
            "PATH": os.environ.get("PATH", ""),
            "TMUX_CHATGPT_USAGE_COMMAND": command,
            "TMUX_CHATGPT_USAGE_CACHE_FILE": cache.name,
        }
        return subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

    def make_command(self, content):
        tempdir = tempfile.TemporaryDirectory()
        path = pathlib.Path(tempdir.name) / "usage-command"
        path.write_text(content, encoding="utf-8")
        path.chmod(path.stat().st_mode | stat.S_IXUSR)
        self.addCleanup(tempdir.cleanup)
        return str(path)

    def test_formats_opencodebar_quota_json(self):
        command = self.make_command(textwrap.dedent(
            """\
            #!/usr/bin/env bash
            printf '%s\n' '{"codex":{"entitlement":100,"remaining":92,"usagePercentage":8}}'
            """
        ))

        result = self.run_usage(command)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "GPT 8%\n")
        self.assertEqual(result.stderr, "")

    def test_formats_float_percentage_compactly(self):
        command = self.make_command(textwrap.dedent(
            """\
            #!/usr/bin/env bash
            printf '%s\n' '{"codex":{"usagePercentage":14.000000000000002}}'
            """
        ))

        result = self.run_usage(command)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "GPT 14%\n")
        self.assertEqual(result.stderr, "")

    def test_prints_nothing_when_usage_command_fails(self):
        result = self.run_usage("definitely-not-opencodebar")

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")

    def test_uses_cached_output_when_available(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.write(b"GPT 7%\n")
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        env = {
            "PATH": os.environ.get("PATH", ""),
            "TMUX_CHATGPT_USAGE_COMMAND": "definitely-not-opencodebar",
            "TMUX_CHATGPT_USAGE_CACHE_FILE": cache.name,
        }

        result = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "GPT 7%\n")
        self.assertEqual(result.stderr, "")

    def test_passes_through_plain_text_output(self):
        command = self.make_command(textwrap.dedent(
            """\
            #!/usr/bin/env bash
            printf '%s\n' 'LLM 12% remaining'
            """
        ))

        result = self.run_usage(command)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "LLM 12% remaining\n")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

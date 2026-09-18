from __future__ import annotations

import contextlib
import http.server
import importlib.machinery
import importlib.util
import io
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest import mock

SCRIPT = pathlib.Path(__file__).resolve().parents[1] / "opencode-provider-gen"
LOADER = importlib.machinery.SourceFileLoader("opencode_provider_gen", str(SCRIPT))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
MODULE = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(MODULE)


class ProviderCliTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.requests = []

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_GET(self):
                cls.requests.append((self.path, self.headers.get("Authorization")))
                status = 200
                payload = {"data": [{"id": "z-chat"}, {"id": "org/a-chat"}, {"id": "z-chat"}]}
                if self.path == "/redirect/models":
                    self.send_response(302)
                    self.send_header("Location", "/destination/models")
                    self.end_headers()
                    return
                if self.path == "/denied/models":
                    status = 401
                    payload = {"error": "sensitive-server-body", "key": "fixture-token"}
                if self.path == "/missing/models":
                    status = 404
                if self.path == "/empty/models":
                    payload = {"data": []}
                if self.path == "/paginated/models":
                    payload["has_more"] = True
                if self.path == "/wrong-shape/models":
                    payload = ["a-chat"]
                if self.path == "/bad-id/models":
                    payload = {"data": [{"id": 123}]}
                if self.path == "/injection/models":
                    payload = {"data": [{"id": "{file:/private/secret}"}]}
                if self.path == "/slow/models":
                    time.sleep(0.1)
                raw = json.dumps(payload).encode("utf-8")
                if self.path == "/invalid/models":
                    raw = b"<html>not-json</html>"
                if self.path == "/large/models":
                    raw = b" " * (MODULE.MAX_RESPONSE_BYTES + 1)
                self.send_response(status)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                try:
                    self.wfile.write(raw)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def log_message(self, *args):
                pass

        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join()

    def setUp(self):
        self.requests.clear()
        self.home_directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.home_directory.cleanup)
        self.env = {
            "HOME": self.home_directory.name,
            "PATH": os.environ.get("PATH", ""),
            "OPENAI_BASE_URL": self.base + "/v1/",
            "OPENAI_API_KEY": "fixture-token",
            "NO_PROXY": "*",
        }

    def run_cli(self, *args, env=None, input_text=None):
        run_env = dict(self.env if env is None else env)
        run_env.setdefault("HOME", self.home_directory.name)
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--name", "fixture", *args],
            input=input_text,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=run_env,
            timeout=10,
        )

    def make_config_dir(self, directory, plugins=None):
        config_dir = pathlib.Path(directory)
        (config_dir / "opencode.json").write_text(json.dumps({
            "$schema": "https://opencode.ai/config.json", "plugin": [] if plugins is None else plugins,
        }, indent=2) + "\n")
        (config_dir / "provider-loader.ts").write_text("export default async () => ({})\n")
        return config_dir

    def test_default_installs_all_models_and_registers_loader(self):
        with tempfile.TemporaryDirectory() as directory:
            config_dir = self.make_config_dir(directory)
            result = self.run_cli("--config-dir", str(config_dir))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, "")
            config = json.loads((config_dir / "opencode.json").read_text())
            self.assertEqual(config["plugin"], ["./provider-loader.ts"])
            generated = (config_dir / "providers/fixture.json").read_text()
            self.assertEqual(json.loads(generated), {
                "$schema": "https://opencode.ai/config.json",
                "provider": {"fixture": {
                    "npm": "@ai-sdk/openai-compatible", "name": "fixture",
                    "options": {
                        "baseURL": "{env:OPENAI_BASE_URL}",
                        "apiKey": "{env:OPENAI_API_KEY}",
                    },
                    "models": {"org/a-chat": {"name": "org/a-chat"}, "z-chat": {"name": "z-chat"}},
                }},
            })
            self.assertEqual(self.requests, [("/v1/models", "Bearer fixture-token")])
            self.assertNotIn("fixture-token", generated)
            self.assertNotIn(self.base, generated)

    def test_install_updates_owned_provider_and_does_not_duplicate_loader(self):
        with tempfile.TemporaryDirectory() as directory:
            config_dir = self.make_config_dir(directory, ["existing", "./provider-loader.ts"])
            provider_dir = config_dir / "providers"
            provider_dir.mkdir()
            target = provider_dir / "fixture.json"
            target.write_text(json.dumps({"provider": {"fixture": {}}}))
            result = self.run_cli("--models", "new-model", "--config-dir", str(config_dir), env={})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(list(json.loads(target.read_text())["provider"]["fixture"]["models"]), ["new-model"])
            self.assertEqual(json.loads((config_dir / "opencode.json").read_text())["plugin"], [
                "existing", "./provider-loader.ts",
            ])

    def test_install_rejects_foreign_or_invalid_existing_provider(self):
        with tempfile.TemporaryDirectory() as directory:
            config_dir = self.make_config_dir(directory)
            provider_dir = config_dir / "providers"
            provider_dir.mkdir()
            target = provider_dir / "fixture.json"
            for content in ("not-json", json.dumps({"provider": {"other": {}}})):
                with self.subTest(content=content):
                    target.write_text(content)
                    original_config = (config_dir / "opencode.json").read_bytes()
                    result = self.run_cli("--models", "a", "--config-dir", str(config_dir), env={})
                    self.assertEqual(result.returncode, 1)
                    self.assertEqual(target.read_text(), content)
                    self.assertEqual((config_dir / "opencode.json").read_bytes(), original_config)

    def test_install_requires_config_and_loader(self):
        with tempfile.TemporaryDirectory() as directory:
            config_dir = pathlib.Path(directory)
            result = self.run_cli("--models", "a", "--config-dir", str(config_dir), env={})
            self.assertEqual(result.returncode, 1)
            self.assertIn("loader not found", result.stderr)
            self.assertFalse((config_dir / "providers").exists())
            (config_dir / "provider-loader.ts").write_text("loader")
            result = self.run_cli("--models", "a", "--config-dir", str(config_dir), env={})
            self.assertEqual(result.returncode, 1)
            self.assertIn("config not found", result.stderr)
            self.assertFalse((config_dir / "providers").exists())

    def test_fixed_environment_names_in_stdout_mode(self):
        env = {
            "OPENAI_BASE_URL": self.base + "/custom/api",
            "OPENAI_API_KEY": "custom-token",
            "NO_PROXY": "*",
        }
        result = self.run_cli("--stdout", env=env)
        self.assertEqual(result.returncode, 0, result.stderr)
        options = json.loads(result.stdout)["provider"]["fixture"]["options"]
        self.assertEqual(options, {"baseURL": "{env:OPENAI_BASE_URL}", "apiKey": "{env:OPENAI_API_KEY}"})
        self.assertEqual(self.requests, [("/custom/api/models", "Bearer custom-token")])
        self.assertNotIn("custom-token", result.stdout)

    def test_reads_fixed_values_from_opencode_dotenv(self):
        env_dir = pathlib.Path(self.home_directory.name) / ".config/opencode"
        env_dir.mkdir(parents=True)
        (env_dir / ".env").write_text(
            f'export OPENAI_BASE_URL="{self.base}/dotenv" # endpoint\n'
            "OPENAI_API_KEY='dotenv-token#part'\n"
            "IGNORED_VALUE=$(this-is-not-executed)\n",
            encoding="utf-8",
        )
        result = self.run_cli("--stdout", env={"HOME": self.home_directory.name, "NO_PROXY": "*"})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.requests, [("/dotenv/models", "Bearer dotenv-token#part")])
        self.assertNotIn("dotenv-token#part", result.stdout)
        self.assertNotIn(self.base, result.stdout)

    def test_process_environment_overrides_dotenv(self):
        env_dir = pathlib.Path(self.home_directory.name) / ".config/opencode"
        env_dir.mkdir(parents=True)
        (env_dir / ".env").write_text(
            "OPENAI_BASE_URL=https://unused.invalid/v1\nOPENAI_API_KEY=unused-token\n",
            encoding="utf-8",
        )
        result = self.run_cli("--stdout")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.requests, [("/v1/models", "Bearer fixture-token")])

    def test_invalid_dotenv_fails_cleanly(self):
        env_dir = pathlib.Path(self.home_directory.name) / ".config/opencode"
        env_dir.mkdir(parents=True)
        (env_dir / ".env").write_text("BROKEN LINE\n", encoding="utf-8")
        result = self.run_cli("--stdout", env={"HOME": self.home_directory.name})
        self.assertEqual(result.returncode, 1)
        self.assertIn("expected NAME=VALUE", result.stderr)
        self.assertEqual(self.requests, [])

    def test_no_auth_omits_header_and_key(self):
        result = self.run_cli("--no-auth", "--stdout")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("apiKey", json.loads(result.stdout)["provider"]["fixture"]["options"])
        self.assertEqual(self.requests, [("/v1/models", None)])

    def test_list_and_filter(self):
        result = self.run_cli("--list")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "org/a-chat\nz-chat\n")
        result = self.run_cli("--include", "org/*", "org/a-chat", "--stdout")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list(json.loads(result.stdout)["provider"]["fixture"]["models"]), ["org/a-chat"])
        result = self.run_cli("--include", "absent*", "--stdout")
        self.assertEqual(result.returncode, 1)
        self.assertIn("matched no models", result.stderr)

    def test_explicit_models_work_offline(self):
        result = self.run_cli("--models", "b", "org/a", "b", "--stdout", env={})
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(list(json.loads(result.stdout)["provider"]["fixture"]["models"]), ["b", "org/a"])
        self.assertEqual(self.requests, [])

    def test_output_is_export_only_and_preserves_existing_path(self):
        with tempfile.TemporaryDirectory() as directory:
            output = pathlib.Path(directory) / "provider.json"
            result = self.run_cli("--models", "a", "-o", str(output), env={})
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn("provider-loader", output.read_text())
            original = output.read_bytes()
            result = self.run_cli("--models", "b", "-o", str(output), env={})
            self.assertEqual(result.returncode, 1)
            self.assertEqual(original, output.read_bytes())

    def test_missing_environment_fails_without_network(self):
        for name in ("OPENAI_BASE_URL", "OPENAI_API_KEY"):
            with self.subTest(name=name):
                env = self.env.copy()
                del env[name]
                result = self.run_cli("--stdout", env=env)
                self.assertEqual(result.returncode, 1)
                self.assertIn(name, result.stderr)
        self.assertEqual(self.requests, [])

    def test_http_errors_bad_responses_and_timeout_do_not_install(self):
        cases = {
            "denied": "HTTP 401", "missing": "--models", "empty": "no models",
            "paginated": "paginated", "invalid": "valid JSON", "wrong-shape": "data array",
            "bad-id": "model IDs", "injection": "model IDs", "large": "safety limit",
        }
        with tempfile.TemporaryDirectory() as directory:
            for path, message in cases.items():
                with self.subTest(path=path):
                    config_dir = self.make_config_dir(directory)
                    self.env["OPENAI_BASE_URL"] = self.base + "/" + path
                    result = self.run_cli("--config-dir", str(config_dir))
                    self.assertEqual(result.returncode, 1)
                    self.assertIn(message, result.stderr)
                    self.assertFalse((config_dir / "providers/fixture.json").exists())
                    self.assertEqual(json.loads((config_dir / "opencode.json").read_text())["plugin"], [])
        self.env["OPENAI_BASE_URL"] = self.base + "/slow"
        result = self.run_cli("--timeout", "0.01", "--stdout")
        self.assertEqual(result.returncode, 1)
        self.assertIn("timeout", result.stderr)

    def test_redirect_is_not_followed(self):
        self.env["OPENAI_BASE_URL"] = self.base + "/redirect"
        result = self.run_cli("--stdout")
        self.assertEqual(result.returncode, 1)
        self.assertIn("redirect refused", result.stderr)
        self.assertEqual(self.requests, [("/redirect/models", "Bearer fixture-token")])

    def test_bad_urls_header_injection_and_extreme_timeout_are_clean_errors(self):
        hosts = [
            "file:///tmp/models", "https://user:secret@example.com/v1", "https://example.com/v1?key=secret",
            "https://example.com/v1#secret", "https://example.com:bad/v1", "https://example.com:0/v1",
            "https://example.com/v1/chat/completions", "http://example.com/v1",
        ]
        for host in hosts:
            with self.subTest(host=host):
                self.env["OPENAI_BASE_URL"] = host
                result = self.run_cli("--stdout")
                self.assertEqual(result.returncode, 1)
                self.assertNotIn("secret", result.stderr)
        self.env["OPENAI_BASE_URL"] = self.base + "/v1"
        self.env["OPENAI_API_KEY"] = "token\r\nX-Injected: yes"
        result = self.run_cli("--stdout")
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("X-Injected", result.stderr)
        self.env["OPENAI_BASE_URL"] = "http://127.0.0.1:1/v1"
        self.env["OPENAI_API_KEY"] = "fixture-token"
        result = self.run_cli("--timeout", "1e300", "--stdout")
        self.assertEqual(result.returncode, 1)
        self.assertNotIn("Traceback", result.stderr)

    def test_name_is_required_and_positional_name_is_rejected(self):
        result = subprocess.run([sys.executable, str(SCRIPT), "fixture", "--models", "a", "--stdout"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env={})
        self.assertEqual(result.returncode, 2)
        result = subprocess.run([sys.executable, str(SCRIPT), "--models", "a", "--stdout"],
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env={})
        self.assertEqual(result.returncode, 2)

    def test_invalid_cli_values_and_conflicting_flags(self):
        for flags in (
            ["--host-env", "NAME"], ["--token-env", "NAME"],
            ["--models", "a", "--include", "a"], ["--stdout", "-o", "unused.json"],
            ["--list", "--stdout"], ["--timeout", "nan"], ["--timeout", "0"],
        ):
            with self.subTest(flags=flags):
                result = self.run_cli(*flags)
                self.assertEqual(result.returncode, 2)

    def test_interactive_selection_and_failures(self):
        for answer, expected in (("1\n", ["org/a-chat"]), ("\n", ["org/a-chat", "z-chat"])):
            with self.subTest(answer=answer):
                stdin, stdout, stderr = io.StringIO(answer), io.StringIO(), io.StringIO()
                with mock.patch.dict(os.environ, self.env, clear=True), mock.patch("sys.stdin", stdin), mock.patch.object(stdin, "isatty", return_value=True), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                    code = MODULE.main(["--name", "fixture", "--select", "--stdout"])
                self.assertEqual(code, 0, stderr.getvalue())
                self.assertEqual(list(json.loads(stdout.getvalue())["provider"]["fixture"]["models"]), expected)
        for answer in ("", "0\n", "3\n", "1-x\n"):
            stdin, stdout, stderr = io.StringIO(answer), io.StringIO(), io.StringIO()
            with mock.patch.dict(os.environ, self.env, clear=True), mock.patch("sys.stdin", stdin), mock.patch.object(stdin, "isatty", return_value=True), contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
                code = MODULE.main(["--name", "fixture", "--select", "--stdout"])
            self.assertEqual(code, 1)
            self.assertEqual(stdout.getvalue(), "")


if __name__ == "__main__":
    unittest.main()

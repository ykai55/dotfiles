import os
import pathlib
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer


class TmuxChatgptUsageTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-chatgpt-usage"

    def run_usage(self, auth_file, url, cache_file=None, extra_env=None):
        if cache_file is None:
            cache = tempfile.NamedTemporaryFile(delete=False)
            cache.close()
            os.unlink(cache.name)
            cache_file = cache.name
            self.addCleanup(lambda: pathlib.Path(cache_file).unlink(missing_ok=True))
        env = {
            "PATH": os.environ.get("PATH", ""),
            "TMUX_CHATGPT_USAGE_AUTH_FILE": str(auth_file),
            "TMUX_CHATGPT_USAGE_CACHE_FILE": cache_file,
            "TMUX_CHATGPT_USAGE_URL": url,
        }
        if extra_env:
            env.update(extra_env)
        return subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

    def make_auth_file(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        path = pathlib.Path(tempdir.name) / "auth.json"
        path.write_text(
            '{"tokens":{"access_token":"test-access","account_id":"acct-123"}}',
            encoding="utf-8",
        )
        return path

    def make_opencode_auth_file(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        path = pathlib.Path(tempdir.name) / "auth.json"
        path.write_text(
            '{"openai":{"type":"oauth","access":"test-access","accountId":"acct-123"}}',
            encoding="utf-8",
        )
        return path

    def start_server(self, status_code, body):
        state = {"requests": 0, "authorization": None, "account_id": None}

        class Handler(BaseHTTPRequestHandler):
            def do_GET(self):
                state["requests"] += 1
                state["authorization"] = self.headers.get("Authorization")
                state["account_id"] = self.headers.get("ChatGPT-Account-Id")
                self.send_response(status_code)
                self.send_header("Content-Type", "application/json")
                self.end_headers()
                self.wfile.write(body.encode("utf-8"))

            def log_message(self, format, *args):
                return

        server = HTTPServer(("127.0.0.1", 0), Handler)
        thread = threading.Thread(target=server.serve_forever)
        thread.daemon = True
        thread.start()
        self.addCleanup(server.shutdown)
        self.addCleanup(server.server_close)
        thread.join(0)
        return f"http://127.0.0.1:{server.server_port}/usage", state

    def test_formats_dual_window_usage_from_http(self):
        auth_file = self.make_auth_file()
        url, state = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":14.0},"secondary_window":{"used_percent":22.25}}}',
        )

        result = self.run_usage(auth_file, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "14%/22.2%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["authorization"], "Bearer test-access")
        self.assertEqual(state["account_id"], "acct-123")

    def test_reads_openai_oauth_from_opencode_auth_file(self):
        auth_file = self.make_opencode_auth_file()
        url, state = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":14.0},"secondary_window":{"used_percent":22.25}}}',
        )

        result = self.run_usage(auth_file, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "14%/22.2%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["authorization"], "Bearer test-access")
        self.assertEqual(state["account_id"], "acct-123")

    def test_prints_nothing_when_auth_file_is_missing(self):
        missing_auth = pathlib.Path(tempfile.gettempdir()) / "missing-auth.json"
        url, _ = self.start_server(200, '{}')

        result = self.run_usage(missing_auth, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")

    def test_prints_nothing_when_auth_file_is_malformed(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        auth_file = pathlib.Path(tempdir.name) / "auth.json"
        auth_file.write_text("{not-json", encoding="utf-8")
        url, state = self.start_server(200, '{}')

        result = self.run_usage(auth_file, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 0)

    def test_prints_nothing_when_api_json_is_invalid(self):
        auth_file = self.make_auth_file()
        url, state = self.start_server(200, "not-json")

        result = self.run_usage(auth_file, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 1)

    def test_formats_single_window_usage_when_secondary_window_is_missing(self):
        auth_file = self.make_auth_file()
        url, state = self.start_server(200, '{"rate_limit":{"primary_window":{"used_percent":14.0}}}')

        result = self.run_usage(auth_file, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "14%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 1)

    def test_prints_nothing_when_primary_window_is_missing(self):
        auth_file = self.make_auth_file()
        url, state = self.start_server(200, '{"rate_limit":{"secondary_window":{"used_percent":22.0}}}')

        result = self.run_usage(auth_file, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 1)

    def test_invalid_ttl_disables_cache_and_fetches_live(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.write(b"GPT 99%\n")
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        auth_file = self.make_auth_file()
        url, state = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":14.0},"secondary_window":{"used_percent":22.25}}}',
        )

        result = self.run_usage(
            auth_file,
            url,
            cache_file=cache.name,
            extra_env={"TMUX_CHATGPT_USAGE_TTL_SECONDS": "not-a-number"},
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "14%/22.2%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 1)

    def test_empty_cache_falls_back_to_live_fetch(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        auth_file = self.make_auth_file()
        url, state = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":14.0},"secondary_window":{"used_percent":22.25}}}',
        )

        result = self.run_usage(auth_file, url, cache_file=cache.name)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "14%/22.2%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 1)

    def test_expired_cache_falls_back_to_live_fetch(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.write(b"GPT 7%\n")
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        auth_file = self.make_auth_file()
        url, state = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":14.0},"secondary_window":{"used_percent":22.25}}}',
        )

        result = self.run_usage(
            auth_file,
            url,
            cache_file=cache.name,
            extra_env={"TMUX_CHATGPT_USAGE_TTL_SECONDS": "0"},
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "14%/22.2%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 1)

    def test_live_fetch_writes_cache_that_later_run_reuses(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.close()
        os.unlink(cache.name)
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        auth_file = self.make_auth_file()
        url, state = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":14.0},"secondary_window":{"used_percent":22.25}}}',
        )

        first_result = self.run_usage(auth_file, url, cache_file=cache.name)
        second_result = self.run_usage(auth_file, url, cache_file=cache.name)

        self.assertEqual(first_result.returncode, 0)
        self.assertEqual(first_result.stdout, "14%/22.2%\n")
        self.assertEqual(first_result.stderr, "")
        self.assertEqual(second_result.returncode, 0)
        self.assertEqual(second_result.stdout, "14%/22.2%\n")
        self.assertEqual(second_result.stderr, "")
        self.assertEqual(state["requests"], 1)
        self.assertEqual(pathlib.Path(cache.name).read_text(encoding="utf-8"), "14%/22.2%\n")

    def test_uses_cached_output_when_available(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.write(b"GPT 7%\n")
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        auth_file = self.make_auth_file()
        url, _ = self.start_server(500, '{}')

        result = subprocess.run(
            [str(self.script)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={
                "PATH": os.environ.get("PATH", ""),
                "TMUX_CHATGPT_USAGE_AUTH_FILE": str(auth_file),
                "TMUX_CHATGPT_USAGE_CACHE_FILE": cache.name,
                "TMUX_CHATGPT_USAGE_URL": url,
            },
            check=False,
        )

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "GPT 7%\n")
        self.assertEqual(result.stderr, "")


if __name__ == "__main__":
    unittest.main()

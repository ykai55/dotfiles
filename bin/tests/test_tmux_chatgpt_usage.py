import json
import os
import pathlib
import sqlite3
import subprocess
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, HTTPServer


class TmuxChatgptUsageTests(unittest.TestCase):
    def setUp(self):
        self.repo_root = pathlib.Path(__file__).resolve().parents[2]
        self.script = self.repo_root / "bin" / "tmux-chatgpt-usage"

    def run_usage(self, auth_file, url, cache_file=None, extra_env=None, extra_args=None):
        if cache_file is None:
            cache = tempfile.NamedTemporaryFile(delete=False)
            cache.close()
            os.unlink(cache.name)
            cache_file = cache.name
            self.addCleanup(lambda: pathlib.Path(cache_file).unlink(missing_ok=True))
        env = {
            "PATH": os.environ.get("PATH", ""),
            "OPENCODE_DB": str(auth_file),
            "TMUX_CHATGPT_USAGE_CACHE_FILE": cache_file,
            "TMUX_CHATGPT_USAGE_URL": url,
        }
        if extra_env:
            env.update(extra_env)
        cmd = [str(self.script)]
        if extra_args:
            cmd.extend(extra_args)
        return subprocess.run(
            cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

    def make_auth_file(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        path = pathlib.Path(tempdir.name) / "opencode.db"
        with sqlite3.connect(path) as db:
            db.execute("CREATE TABLE credential (integration_id TEXT, value TEXT, active INTEGER, time_updated INTEGER)")
            db.execute("INSERT INTO credential VALUES ('openai', ?, 1, 1)", (
                json.dumps({"type": "oauth", "access": "test-access", "metadata": {"accountID": "acct-123"}}),
            ))
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

    def test_reads_openai_oauth_from_opencode_database(self):
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

    def test_database_credential_selection(self):
        auth_file = self.make_auth_file()
        data_home = auth_file.parent
        data_dir = data_home / "opencode"
        data_dir.mkdir()
        (data_dir / "auth.json").write_text('{"openai":{"access":"stale","accountId":"old"}}')
        db_path = data_dir / "opencode.db"
        url, state = self.start_server(200, '{"rate_limit":{"primary_window":{"used_percent":14}}}')
        environment = {"OPENCODE_DB": "", "XDG_DATA_HOME": str(data_home), "HOME": str(data_home)}

        # A legacy file is never used and a missing database is not created.
        result = self.run_usage(auth_file, url, extra_env=environment)
        self.assertEqual(result.stdout, "")
        self.assertEqual(state["requests"], 0)
        self.assertFalse(db_path.exists())

        with sqlite3.connect(db_path) as db:
            db.execute("CREATE TABLE credential (integration_id TEXT, value TEXT, active INTEGER, time_updated INTEGER)")
            for token, active, updated in [("newest", 0, 20), ("selected", 1, 10)]:
                value = json.dumps({"type": "oauth", "access": token, "metadata": {"accountID": "db-account"}})
                db.execute("INSERT INTO credential VALUES (?, ?, ?, ?)", ("openai", value, active, updated))

        result = self.run_usage(auth_file, url, extra_env=environment)
        self.assertEqual(result.stdout, "14%\n")
        self.assertEqual(state["authorization"], "Bearer selected")
        self.assertEqual(state["account_id"], "db-account")

        with sqlite3.connect(db_path) as db:
            db.execute("UPDATE credential SET active = 0")
        result = self.run_usage(auth_file, url, extra_env=environment)
        self.assertEqual(state["authorization"], "Bearer newest")

        # An absolute OPENCODE_DB path overrides the default database.
        result = self.run_usage(auth_file, url, extra_env={"XDG_DATA_HOME": str(data_home)})
        self.assertEqual(state["authorization"], "Bearer test-access")

        # Relative OPENCODE_DB paths are resolved under the OpenCode data dir.
        db_path.rename(data_dir / "custom.db")
        environment["OPENCODE_DB"] = "custom.db"
        result = self.run_usage(auth_file, url, extra_env=environment)
        self.assertEqual(state["authorization"], "Bearer newest")

        for value in [None, '{"type":"key","key":"fake"}', 'null', '{bad-json']:
            with self.subTest(value=value):
                with sqlite3.connect(data_dir / "custom.db") as db:
                    db.execute("DELETE FROM credential")
                    if value is not None:
                        db.execute("INSERT INTO credential VALUES ('openai', ?, 1, 30)", (value,))
                before = state["requests"]
                result = self.run_usage(auth_file, url, extra_env=environment, extra_args=["--detail"])
                self.assertEqual(result.returncode, 1)
                self.assertEqual(state["requests"], before)
                self.assertEqual(result.stdout, "")

    def test_prints_nothing_when_database_is_missing(self):
        missing_auth = pathlib.Path(tempfile.gettempdir()) / "missing-usage-auth.db"
        url, _ = self.start_server(200, '{}')

        result = self.run_usage(missing_auth, url)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "")
        self.assertEqual(result.stderr, "")

    def test_prints_nothing_when_database_is_malformed(self):
        tempdir = tempfile.TemporaryDirectory()
        self.addCleanup(tempdir.cleanup)
        auth_file = pathlib.Path(tempdir.name) / "opencode.db"
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
        cache.write(b'{"rate_limit":{"primary_window":{"used_percent":99}}}')
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
        cache.write(
            b'{"rate_limit":{"primary_window":{"used_percent":7}}}',
        )
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

        expected = "14%/22.2%\n"
        self.assertEqual(first_result.returncode, 0)
        self.assertEqual(first_result.stdout, expected)
        self.assertEqual(first_result.stderr, "")
        self.assertEqual(second_result.returncode, 0)
        self.assertEqual(second_result.stdout, expected)
        self.assertEqual(second_result.stderr, "")
        self.assertEqual(state["requests"], 1)
        cached = json.loads(pathlib.Path(cache.name).read_text(encoding="utf-8"))
        self.assertEqual(
            cached["rate_limit"]["primary_window"]["used_percent"], 14.0
        )

    def test_uses_cached_output_when_available(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.write(
            b'{"rate_limit":{"primary_window":{"used_percent":7}}}',
        )
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))
        auth_file = self.make_auth_file()
        url, state = self.start_server(500, '{}')

        result = self.run_usage(auth_file, url, cache_file=cache.name)

        self.assertEqual(result.returncode, 0)
        self.assertEqual(result.stdout, "7%\n")
        self.assertEqual(result.stderr, "")
        self.assertEqual(state["requests"], 0)

    def test_detail_fetches_live_when_cache_disabled(self):
        auth_file = self.make_auth_file()
        import time as time_module
        reset_at = int(time_module.time()) + 604800
        body = (
            '{"plan_type":"prolite","rate_limit":{'
            '"primary_window":{"used_percent":85,"limit_window_seconds":604800,"reset_at":' + str(reset_at) + '}'
            '}}'
        )
        url, state = self.start_server(200, body)

        result = self.run_usage(
            auth_file, url,
            extra_args=["--detail"],
            extra_env={"TMUX_CHATGPT_USAGE_TTL_SECONDS": "0"},
        )

        self.assertEqual(result.returncode, 0)
        self.assertIn("ChatGPT usage", result.stdout)
        self.assertIn("\033[33m85%\033[0m", result.stdout)
        self.assertEqual(state["requests"], 1)

    def test_detail_reuses_cache(self):
        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.write(
            b'{"plan_type":"prolite","rate_limit":{'
            b'"primary_window":{"used_percent":30,"reset_at":2000000000}}}',
        )
        cache.close()
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))

        auth_file = self.make_auth_file()
        url, state = self.start_server(200, "{}")

        result = self.run_usage(auth_file, url, cache_file=cache.name, extra_args=["--detail"])

        self.assertEqual(result.returncode, 0)
        self.assertIn("\033[32m30%\033[0m", result.stdout)
        self.assertEqual(state["requests"], 0)

    def test_detail_reports_error_when_fetch_fails(self):
        auth_file = self.make_auth_file()
        url, state = self.start_server(500, '{}')

        result = self.run_usage(auth_file, url, extra_args=["--detail"])

        self.assertEqual(result.returncode, 1)
        self.assertIn("failed to fetch", result.stderr)
        self.assertEqual(state["requests"], 1)

    def test_detail_reports_error_when_auth_missing(self):
        missing_auth = pathlib.Path(tempfile.gettempdir()) / "missing-detail-auth.db"
        url, _ = self.start_server(200, '{}')

        cache = tempfile.NamedTemporaryFile(delete=False)
        cache.close()
        os.unlink(cache.name)
        self.addCleanup(lambda: pathlib.Path(cache.name).unlink(missing_ok=True))

        env = {
            "PATH": os.environ.get("PATH", ""),
            "OPENCODE_DB": str(missing_auth),
            "TMUX_CHATGPT_USAGE_CACHE_FILE": cache.name,
            "TMUX_CHATGPT_USAGE_URL": url,
        }
        result = subprocess.run(
            [str(self.script), "--detail"],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env=env,
            check=False,
        )

        self.assertEqual(result.returncode, 1)
        self.assertIn("auth", result.stderr)
        self.assertEqual(result.stdout, "")

    def test_detail_colors_high_percentage_red(self):
        auth_file = self.make_auth_file()
        url, _ = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":95}}}',
        )

        result = self.run_usage(auth_file, url, extra_args=["--detail"])

        self.assertIn("\033[31m95%\033[0m", result.stdout)

    def test_detail_colors_medium_percentage_orange(self):
        auth_file = self.make_auth_file()
        url, _ = self.start_server(
            200,
            '{"rate_limit":{"primary_window":{"used_percent":85}}}',
        )

        result = self.run_usage(auth_file, url, extra_args=["--detail"])

        self.assertIn("\033[33m85%\033[0m", result.stdout)


if __name__ == "__main__":
    unittest.main()

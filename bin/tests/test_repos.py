import importlib.machinery
import importlib.util
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

TESTS_DIR = pathlib.Path(__file__).resolve().parent
if str(TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(TESTS_DIR))
from test_utils import CapturingTestCase


def load_repos_module():
    repos_path = pathlib.Path(__file__).resolve().parents[1] / "repos"
    spec = importlib.util.spec_from_file_location(
        "repos",
        repos_path,
        loader=importlib.machinery.SourceFileLoader("repos", str(repos_path)),
    )
    if not spec or not spec.loader:
        raise RuntimeError(f"Failed to load spec for {repos_path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReposTests(CapturingTestCase):
    def setUp(self):
        super().setUp()
        self.repos = load_repos_module()

    def test_expand_patterns_filters_non_directories_and_deduplicates(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            root = pathlib.Path(tmpdir)
            repo_a = root / "template-assembler-a"
            repo_b = root / "template-assembler-b"
            note = root / "template-assembler.txt"
            repo_a.mkdir()
            repo_b.mkdir()
            note.write_text("x", encoding="utf-8")

            patterns = [
                str(root / "template-assembler*"),
                str(root / "template-assembler-a"),
            ]

            matched = self.repos.expand_patterns(patterns)

        self.assertEqual(matched, [str(repo_a), str(repo_b)])

    def test_format_path_collapses_home_prefix(self):
        home = pathlib.Path.home()
        repo_path = str(home / "src" / "template-assembler")
        self.assertEqual(self.repos.format_path(repo_path), "~/src/template-assembler")

    def test_branch_for_repo_returns_branch_name(self):
        with mock.patch.object(
            self.repos,
            "run_git",
            side_effect=[
                (0, "true\n", ""),
                (0, "main\n", ""),
            ],
        ):
            branch = self.repos.branch_for_repo("/tmp/repo")

        self.assertEqual(branch, "main")

    def test_branch_for_repo_reports_detached_head(self):
        with mock.patch.object(
            self.repos,
            "run_git",
            side_effect=[
                (0, "true\n", ""),
                (1, "", "detached"),
                (0, "abc1234\n", ""),
            ],
        ):
            branch = self.repos.branch_for_repo("/tmp/repo")

        self.assertEqual(branch, "(detached abc1234)")

    def test_branch_for_repo_returns_none_for_non_repo(self):
        with mock.patch.object(
            self.repos,
            "run_git",
            return_value=(1, "", "fatal: not a git repository"),
        ):
            branch = self.repos.branch_for_repo("/tmp/repo")

        self.assertIsNone(branch)

    def test_list_repos_prints_branch_and_non_repo_marker(self):
        with mock.patch.object(
            self.repos,
            "expand_patterns",
            return_value=["/tmp/repo-a", "/tmp/repo-b"],
        ), mock.patch.object(
            self.repos,
            "branch_for_repo",
            side_effect=["main", None],
        ):
            rc = self.repos.list_repos(["~/src/template-assembler*"])

        self.assertEqual(rc, 0)
        out = self._stdout_buffer.getvalue().splitlines()
        self.assertEqual(out, ["/tmp/repo-a\tmain", "/tmp/repo-b\t(not a git repo)"])

    def test_list_repos_reports_missing_matches(self):
        with mock.patch.object(self.repos, "expand_patterns", return_value=[]):
            rc = self.repos.list_repos(["~/src/template-assembler*"])

        self.assertEqual(rc, 1)
        self.assertIn("No directories matched", self._stderr_buffer.getvalue())

    def test_main_uses_default_pattern(self):
        argv = ["repos"]
        with mock.patch.object(self.repos, "list_repos", return_value=0) as list_mock, \
            mock.patch.object(self.repos.sys, "argv", argv):
            rc = self.repos.main()

        self.assertEqual(rc, 0)
        self.assertEqual(list_mock.call_args[0][0], self.repos.DEFAULT_PATTERNS)


if __name__ == "__main__":
    unittest.main()

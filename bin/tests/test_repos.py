from __future__ import annotations

import importlib.util
import importlib.machinery
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock


REPO_SCRIPT = Path(__file__).resolve().parents[1] / "repos"
SPEC = importlib.util.spec_from_file_location(
    "repos_script",
    REPO_SCRIPT,
    loader=importlib.machinery.SourceFileLoader("repos_script", str(REPO_SCRIPT)),
)
if not SPEC or not SPEC.loader:
    raise RuntimeError(f"Failed to load spec for {REPO_SCRIPT}")
repos = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = repos
SPEC.loader.exec_module(repos)


class ReposTests(unittest.TestCase):
    def test_parse_worktree_paths_preserves_git_order(self) -> None:
        porcelain = "\n".join(
            [
                "worktree /src/project",
                "HEAD abc",
                "branch refs/heads/main",
                "",
                "worktree /src/project-feature",
                "HEAD def",
                "branch refs/heads/feature",
            ]
        )

        self.assertEqual(
            repos.parse_worktree_paths(porcelain),
            ["/src/project", "/src/project-feature"],
        )

    def test_order_worktree_paths_moves_main_worktree_first(self) -> None:
        self.assertEqual(
            repos.order_worktree_paths(
                ["/src/project-feature", "/src/project", "/src/project-fix"],
                "/src/project/.git",
            ),
            ["/src/project", "/src/project-feature", "/src/project-fix"],
        )

    def test_fzf_line_aligns_branch_after_padded_path(self) -> None:
        candidate = repos.Candidate(
            repo_path="/src/project-feature",
            repo_id="/src/project/.git",
            display_path="  ~/src/project-feature",
            branch="feature",
        )

        self.assertEqual(
            candidate.fzf_line(24),
            "\033[0m  ~/src/project-feature  [feature]\t/src/project-feature\t/src/project/.git",
        )

    def test_display_path_width_uses_longest_display_path(self) -> None:
        items = [
            repos.Candidate("/src/a", "/src/a/.git", "~/src/a", "main"),
            repos.Candidate("/src/long-name", "/src/a/.git", "  ~/src/long-name", "feature"),
        ]

        self.assertEqual(repos.display_path_width(items), len("  ~/src/long-name"))

    def test_run_fzf_uses_reverse_layout(self) -> None:
        candidate = repos.Candidate(
            repo_path="/src/project",
            repo_id="/src/project/.git",
            display_path="~/src/project",
            branch="main",
        )
        completed = repos.subprocess.CompletedProcess(
            args=[],
            returncode=0,
            stdout="\n~/src/project [main]\t/src/project\t/src/project/.git\n",
        )

        with mock.patch.object(repos.subprocess, "run", return_value=completed) as run:
            selected = repos.run_fzf([candidate])

        argv = run.call_args.args[0]
        self.assertIn("--reverse", argv)
        self.assertIn("ctrl-x", argv)
        self.assertNotIn("ctrl-d", argv)
        self.assertEqual(selected, ("", candidate))

    def test_recent_records_uses_newest_record_per_worktree(self) -> None:
        with tempfile.NamedTemporaryFile("w", encoding="utf-8", delete=False) as f:
            f.write("1\trepo-a\t/project\n")
            f.write("3\trepo-a\t/project-feature\n")
            f.write("2\trepo-a\t/project\n")
            path = f.name

        try:
            records = repos.recent_records(path)
            self.assertEqual(
                [(item.repo_id, item.anchor_path, item.timestamp) for item in records],
                [("repo-a", "/project-feature", 3), ("repo-a", "/project", 2)],
            )
        finally:
            os.unlink(path)

    def test_candidates_keeps_parent_worktree_first_within_recent_repo(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            main_path = os.path.join(tmp, "project")
            child_path = os.path.join(tmp, "project-feature")
            os.mkdir(main_path)
            os.mkdir(child_path)
            repo_id = os.path.join(main_path, ".git")
            recent_file = os.path.join(tmp, "recent-repos.tsv")
            with open(recent_file, "w", encoding="utf-8") as f:
                f.write(f"10\t{repo_id}\t{main_path}\n")
                f.write(f"20\t{repo_id}\t{child_path}\n")

            def fake_worktrees(repo_path: str) -> list[str]:
                self.assertEqual(repo_path, child_path)
                return [child_path, main_path]

            with mock.patch.object(repos, "worktrees", side_effect=fake_worktrees), mock.patch.object(
                repos, "is_git_worktree", return_value=True
            ), mock.patch.object(repos, "branch_name", side_effect=["main", "feature"]):
                candidates = repos.candidates([], recent_file)

        self.assertEqual([item.repo_path for item in candidates], [main_path, child_path])
        self.assertEqual([item.display_path for item in candidates], [main_path, f"  {child_path}"])
        self.assertEqual([item.branch for item in candidates], ["main", "feature"])

    def test_candidates_orders_repo_groups_by_last_entered_time(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            old_repo = os.path.join(tmp, "old")
            new_repo = os.path.join(tmp, "new")
            os.mkdir(old_repo)
            os.mkdir(new_repo)
            old_repo_id = os.path.join(old_repo, ".git")
            new_repo_id = os.path.join(new_repo, ".git")
            recent_file = os.path.join(tmp, "recent-repos.tsv")
            with open(recent_file, "w", encoding="utf-8") as f:
                f.write(f"10\t{old_repo_id}\t{old_repo}\n")
                f.write(f"20\t{new_repo_id}\t{new_repo}\n")

            def fake_worktrees(repo_path: str) -> list[str]:
                return [repo_path]

            with mock.patch.object(repos, "worktrees", side_effect=fake_worktrees), mock.patch.object(
                repos, "is_git_worktree", return_value=True
            ), mock.patch.object(repos, "branch_name", side_effect=["new", "old"]):
                candidates = repos.candidates([], recent_file)

        self.assertEqual([item.repo_path for item in candidates], [new_repo, old_repo])

    def test_repo_group_time_uses_last_entered_child_worktree(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            repo_a = os.path.join(tmp, "repo-a")
            repo_a_child = os.path.join(tmp, "repo-a-child")
            repo_b = os.path.join(tmp, "repo-b")
            os.mkdir(repo_a)
            os.mkdir(repo_a_child)
            os.mkdir(repo_b)
            repo_a_id = os.path.join(repo_a, ".git")
            repo_b_id = os.path.join(repo_b, ".git")
            recent_file = os.path.join(tmp, "recent-repos.tsv")
            with open(recent_file, "w", encoding="utf-8") as f:
                f.write(f"10\t{repo_a_id}\t{repo_a}\n")
                f.write(f"20\t{repo_b_id}\t{repo_b}\n")
                f.write(f"30\t{repo_a_id}\t{repo_a_child}\n")

            def fake_worktrees(repo_path: str) -> list[str]:
                if repo_path == repo_a_child:
                    return [repo_a, repo_a_child]
                return [repo_path]

            with mock.patch.object(repos, "worktrees", side_effect=fake_worktrees), mock.patch.object(
                repos, "is_git_worktree", return_value=True
            ), mock.patch.object(repos, "branch_name", side_effect=["main", "child", "other"]):
                candidates = repos.candidates([], recent_file)

        self.assertEqual([item.repo_path for item in candidates], [repo_a, repo_a_child, repo_b])


if __name__ == "__main__":
    unittest.main()

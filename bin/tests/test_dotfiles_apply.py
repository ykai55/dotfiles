import hashlib
import importlib.machinery
import importlib.util
import io
import json
import os
import pathlib
import stat
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from unittest import mock

TESTS_DIR = pathlib.Path(__file__).resolve().parent
if str(TESTS_DIR) not in sys.path:
    sys.path.insert(0, str(TESTS_DIR))
from test_utils import CapturingTestCase


def load_dotfiles_apply_module():
    script_path = pathlib.Path(__file__).resolve().parents[1] / "dotfiles-apply"
    spec = importlib.util.spec_from_file_location(
        "dotfiles_apply",
        script_path,
        loader=importlib.machinery.SourceFileLoader(
            "dotfiles_apply", str(script_path)
        ),
    )
    if not spec or not spec.loader:
        raise RuntimeError(f"Failed to load spec for {script_path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


class DotfilesApplyTests(CapturingTestCase):
    def setUp(self):
        super().setUp()
        self.dotfiles_apply = load_dotfiles_apply_module()

    def write_json(self, path, payload):
        with open(path, "w", encoding="utf-8") as f:
            json.dump(payload, f)

    def write_clip_download_manifest(self, repo, archive_path, sha256=None):
        manifest_path = os.path.join(repo, "downloads.json")
        self.write_json(
            manifest_path,
            {
                "$schema": "./downloads.schema.json",
                "version": 1,
                "tools": [
                    {
                        "name": "clip",
                        "version": "v1.0.0",
                        "targets": [
                            {
                                "target": "linux-x86_64-musl",
                                "platform": "linux",
                                "arch": "x86_64",
                                "url": "file://" + archive_path,
                                "archive": "tar.gz",
                                "executable": "clip",
                            }
                        ],
                    }
                ],
            },
        )
        if sha256 is not None:
            with open(archive_path + ".sha256", "w", encoding="utf-8") as f:
                f.write(f"{sha256}  {os.path.basename(archive_path)}\n")
        return manifest_path

    def clip_download_marker(self, archive_path, sha256):
        return {
            "tool": "clip",
            "version": "v1.0.0",
            "target": "linux-x86_64-musl",
            "url": "file://" + archive_path,
            "sha256": sha256,
            "archive": "tar.gz",
            "executable": "clip",
        }

    def make_clip_archive(self, tmpdir, content="#!/usr/bin/env bash\nprintf 'clip-ok\\n'\n"):
        source_dir = os.path.join(tmpdir, "archive-src")
        os.makedirs(source_dir, exist_ok=True)
        clip_path = os.path.join(source_dir, "clip")
        with open(clip_path, "w", encoding="utf-8") as f:
            f.write(content)
        os.chmod(clip_path, os.stat(clip_path).st_mode | stat.S_IXUSR)
        archive_path = os.path.join(tmpdir, "clip-linux-x86_64-musl.tar.gz")
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(clip_path, arcname="clip")
        with open(archive_path, "rb") as f:
            sha256 = hashlib.sha256(f.read()).hexdigest()
        return archive_path, sha256

    def write_archive_bytes(self, path):
        with open(path, "rb") as f:
            return hashlib.sha256(f.read()).hexdigest()

    def write_download_manifest_for_archive(self, repo, archive_path, archive_type, sha256):
        manifest_path = self.write_clip_download_manifest(repo, archive_path, sha256)
        with open(manifest_path, "r", encoding="utf-8") as f:
            payload = json.load(f)
        payload["tools"][0]["targets"][0]["archive"] = archive_type
        self.write_json(manifest_path, payload)
        return manifest_path

    def write_empty_manifest(self, repo):
        manifest_path = os.path.join(repo, "manifest.json")
        self.write_json(manifest_path, {"mappings": []})
        return manifest_path

    def write_git_download_manifest(self, repo):
        manifest_path = os.path.join(repo, "downloads.json")
        self.write_json(
            manifest_path,
            {
                "$schema": "./downloads.schema.json",
                "version": 1,
                "tools": [
                    {
                        "name": "tide",
                        "type": "git",
                        "url": "git@github.com:ykai55/tide.git",
                        "ref": "stale-git-prompt",
                        "target": ".managed/tide",
                    }
                ],
            },
        )
        return manifest_path

    def apply_managed_download(self, home, repo, manifest_path):
        with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
            self.dotfiles_apply,
            "current_platform",
            return_value="linux",
        ), mock.patch.object(
            self.dotfiles_apply,
            "current_machine_arch",
            return_value="x86_64",
        ):
            return self.dotfiles_apply.apply_manifest(repo, manifest_path)

    def test_managed_download_dry_run_does_not_fetch(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(
                self.dotfiles_apply.urllib.request,
                "urlopen",
                side_effect=AssertionError("download should not run"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path, dry_run=True)

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.exists(os.path.join(repo, "bin", ".downloads")))
            self.assertIn("[download] clip linux-x86_64-musl", self._stdout_buffer.getvalue())

    def test_git_download_dry_run_does_not_clone(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            manifest_path = self.write_empty_manifest(repo)
            self.write_git_download_manifest(repo)

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply.subprocess,
                "run",
                side_effect=AssertionError("git should not run"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path, dry_run=True)

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.exists(os.path.join(repo, ".managed")))
            self.assertIn("[clone] tide", self._stdout_buffer.getvalue())

    def test_git_download_clones_missing_repo(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            manifest_path = self.write_empty_manifest(repo)
            self.write_git_download_manifest(repo)

            completed = subprocess.CompletedProcess(["git"], 0, "", "")
            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply.subprocess,
                "run",
                return_value=completed,
            ) as run_mock:
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            run_mock.assert_called_once_with(
                [
                    "git",
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    "stale-git-prompt",
                    "--single-branch",
                    "git@github.com:ykai55/tide.git",
                    os.path.join(repo, ".managed", "tide"),
                ],
                cwd=None,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )

    def test_git_download_auto_keeps_existing_clone_without_fetching(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(os.path.join(repo, ".managed", "tide", ".git"), exist_ok=True)
            manifest_path = self.write_empty_manifest(repo)
            self.write_git_download_manifest(repo)

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply.subprocess,
                "run",
                side_effect=AssertionError("git should not run"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertIn("[ok] tide", self._stdout_buffer.getvalue())

    def test_git_download_always_updates_existing_clone(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(os.path.join(repo, ".managed", "tide", ".git"), exist_ok=True)
            self.write_git_download_manifest(repo)

            completed = subprocess.CompletedProcess(["git"], 0, "", "")
            with mock.patch.object(
                self.dotfiles_apply.subprocess,
                "run",
                return_value=completed,
            ) as run_mock:
                stats = self.dotfiles_apply.Stats()
                self.dotfiles_apply.ensure_managed_downloads(
                    repo,
                    dry_run=False,
                    stats=stats,
                    download_mode="always",
                )

            self.assertEqual(stats.errors, 0)
            self.assertEqual(
                [call.args[0] for call in run_mock.call_args_list],
                [
                    [
                        "git",
                        "fetch",
                        "--depth",
                        "1",
                        "origin",
                        "refs/heads/stale-git-prompt:refs/remotes/origin/stale-git-prompt",
                    ],
                    ["git", "checkout", "-B", "stale-git-prompt", "origin/stale-git-prompt"],
                ],
            )

    def test_managed_download_installs_verified_archive(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            installed = os.path.join(
                repo,
                "bin",
                ".downloads",
                "clip",
                "v1.0.0",
                "linux-x86_64-musl",
                "clip",
            )
            current = os.path.join(repo, "bin", ".downloads", "clip", "current")
            self.assertEqual(stats.errors, 0)
            self.assertTrue(os.path.exists(installed))
            self.assertTrue(os.access(installed, os.X_OK))
            self.assertTrue(os.path.islink(current))
            self.assertEqual(os.readlink(current), "v1.0.0")

    def test_managed_download_fetches_sibling_checksum(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, "0" * 64)
            with open(archive_path + ".sha256", "w", encoding="utf-8") as f:
                f.write(f"{sha256.upper()}  {os.path.basename(archive_path)}\n")
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            installed = os.path.join(
                repo,
                "bin",
                ".downloads",
                "clip",
                "v1.0.0",
                "linux-x86_64-musl",
                "clip",
            )
            marker_path = os.path.join(os.path.dirname(installed), ".download.json")
            self.assertEqual(stats.errors, 0)
            self.assertTrue(os.path.exists(installed))
            with open(marker_path, "r", encoding="utf-8") as f:
                marker = json.load(f)
            self.assertEqual(marker["sha256"], sha256)

    def test_managed_download_rejects_invalid_checksum_file(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, _sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, "0" * 64)
            with open(archive_path + ".sha256", "w", encoding="utf-8") as f:
                f.write("no checksum here\n")
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            installed = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0")
            self.assertEqual(stats.errors, 1)
            self.assertFalse(os.path.exists(installed))
            self.assertIn("No sha256 checksum found", self._stderr_buffer.getvalue())

    def test_managed_download_checks_hash_before_archive_download(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, _sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, "0" * 64)
            with open(archive_path + ".sha256", "w", encoding="utf-8") as f:
                f.write("no checksum here\n")
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(
                self.dotfiles_apply,
                "download_file",
                side_effect=AssertionError("archive should not download before checksum"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 1)
            self.assertIn("No sha256 checksum found", self._stderr_buffer.getvalue())

    def test_managed_download_dry_run_does_not_fetch_checksum(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, _sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, "0" * 64)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(
                self.dotfiles_apply.urllib.request,
                "urlopen",
                side_effect=AssertionError("download should not run"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path, dry_run=True)

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.exists(os.path.join(repo, "bin", ".downloads")))
            self.assertIn("[download] clip linux-x86_64-musl", self._stdout_buffer.getvalue())

    def test_managed_download_accepts_cached_binary_with_resolved_marker(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            cached_dir = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0", "linux-x86_64-musl")
            os.makedirs(cached_dir, exist_ok=True)
            cached_clip = os.path.join(cached_dir, "clip")
            with open(cached_clip, "w", encoding="utf-8") as f:
                f.write("cached\n")
            os.chmod(cached_clip, os.stat(cached_clip).st_mode | stat.S_IXUSR)
            current = os.path.join(repo, "bin", ".downloads", "clip", "current")
            os.symlink("v1.0.0", current)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, "0" * 64)
            with open(archive_path + ".sha256", "w", encoding="utf-8") as f:
                f.write(f"{sha256}  {os.path.basename(archive_path)}\n")
            marker_path = os.path.join(cached_dir, ".download.json")
            self.write_json(marker_path, self.clip_download_marker(archive_path, sha256))
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(
                self.dotfiles_apply,
                "download_file",
                side_effect=AssertionError("archive should not download"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertIn("[ok] clip linux-x86_64-musl", self._stdout_buffer.getvalue())

    def test_managed_download_refreshes_stale_cached_binary(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            cached_dir = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0", "linux-x86_64-musl")
            os.makedirs(cached_dir, exist_ok=True)
            cached_clip = os.path.join(cached_dir, "clip")
            with open(cached_clip, "w", encoding="utf-8") as f:
                f.write("old cached\n")
            os.chmod(cached_clip, os.stat(cached_clip).st_mode | stat.S_IXUSR)
            current = os.path.join(repo, "bin", ".downloads", "clip", "current")
            os.symlink("v1.0.0", current)
            _old_archive_path, old_sha256 = self.make_clip_archive(tmpdir, "old archive\n")
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, "0" * 64)
            with open(archive_path + ".sha256", "w", encoding="utf-8") as f:
                f.write(f"{sha256}  {os.path.basename(archive_path)}\n")
            self.write_json(
                os.path.join(cached_dir, ".download.json"),
                self.clip_download_marker(archive_path, old_sha256),
            )
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertIn("[download] clip linux-x86_64-musl", self._stdout_buffer.getvalue())
            with open(cached_clip, "r", encoding="utf-8") as f:
                self.assertEqual(f.read(), "#!/usr/bin/env bash\nprintf 'clip-ok\\n'\n")

    def test_managed_download_accepts_existing_cached_binary(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            cached_dir = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0", "linux-x86_64-musl")
            os.makedirs(cached_dir, exist_ok=True)
            cached_clip = os.path.join(cached_dir, "clip")
            with open(cached_clip, "w", encoding="utf-8") as f:
                f.write("cached\n")
            os.chmod(cached_clip, os.stat(cached_clip).st_mode | stat.S_IXUSR)
            current = os.path.join(repo, "bin", ".downloads", "clip", "current")
            os.symlink("v1.0.0", current)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            marker_path = os.path.join(cached_dir, ".download.json")
            self.write_json(marker_path, self.clip_download_marker(archive_path, sha256))
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(
                self.dotfiles_apply,
                "download_file",
                side_effect=AssertionError("archive should not download"),
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertIn("[ok] clip linux-x86_64-musl", self._stdout_buffer.getvalue())

    def test_managed_download_can_be_disabled(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(
                self.dotfiles_apply.urllib.request,
                "urlopen",
                side_effect=AssertionError("download should not run"),
            ):
                stats = self.dotfiles_apply.apply_manifest(
                    repo,
                    manifest_path,
                    download_mode="never",
                )

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.exists(os.path.join(repo, "bin", ".downloads")))
            self.assertNotIn("[download] clip", self._stdout_buffer.getvalue())

    def test_managed_download_always_refreshes_existing_cached_binary(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            cached_dir = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0", "linux-x86_64-musl")
            os.makedirs(cached_dir, exist_ok=True)
            cached_clip = os.path.join(cached_dir, "clip")
            with open(cached_clip, "w", encoding="utf-8") as f:
                f.write("cached\n")
            os.chmod(cached_clip, os.stat(cached_clip).st_mode | stat.S_IXUSR)
            current = os.path.join(repo, "bin", ".downloads", "clip", "current")
            os.symlink("v1.0.0", current)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            marker_path = os.path.join(cached_dir, ".download.json")
            self.write_json(marker_path, self.clip_download_marker(archive_path, sha256))
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ):
                stats = self.dotfiles_apply.apply_manifest(
                    repo,
                    manifest_path,
                    download_mode="always",
                )

            self.assertEqual(stats.errors, 0)
            self.assertIn("[download] clip linux-x86_64-musl", self._stdout_buffer.getvalue())
            with open(cached_clip, "r", encoding="utf-8") as f:
                self.assertEqual(f.read(), "#!/usr/bin/env bash\nprintf 'clip-ok\\n'\n")

    def test_managed_download_rejects_unmarked_cached_binary(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            cached_dir = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0", "linux-x86_64-musl")
            os.makedirs(cached_dir, exist_ok=True)
            cached_clip = os.path.join(cached_dir, "clip")
            with open(cached_clip, "w", encoding="utf-8") as f:
                f.write("cached\n")
            os.chmod(cached_clip, os.stat(cached_clip).st_mode | stat.S_IXUSR)

            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertIn("[download] clip linux-x86_64-musl", self._stdout_buffer.getvalue())
            with open(cached_clip, "r", encoding="utf-8") as f:
                self.assertEqual(f.read(), "#!/usr/bin/env bash\nprintf 'clip-ok\\n'\n")
            marker_path = os.path.join(cached_dir, ".download.json")
            with open(marker_path, "r", encoding="utf-8") as f:
                self.assertEqual(json.load(f), self.clip_download_marker(archive_path, sha256))

    def test_managed_download_rejects_sha256_mismatch(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path, _sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(
                repo,
                archive_path,
                "f" * 64,
            )
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            installed = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0")
            self.assertEqual(stats.errors, 1)
            self.assertFalse(os.path.exists(installed))
            self.assertIn("sha256 mismatch", self._stderr_buffer.getvalue())

    def test_managed_download_rejects_unsafe_manifest_fields(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(repo, exist_ok=True)
            archive_path, sha256 = self.make_clip_archive(tmpdir)

            cases = (
                ("tool", ["tools", 0, "name"], "../clip"),
                ("version", ["tools", 0, "version"], "v1/0"),
                ("target", ["tools", 0, "targets", 0, "target"], "/tmp/target"),
                ("executable", ["tools", 0, "targets", 0, "executable"], "../clip"),
                ("executable-empty-segment", ["tools", 0, "targets", 0, "executable"], "bin//clip"),
                ("executable-backslash", ["tools", 0, "targets", 0, "executable"], "bin\\clip"),
                ("executable-drive-relative", ["tools", 0, "targets", 0, "executable"], "C:clip.exe"),
                ("executable-windows-absolute", ["tools", 0, "targets", 0, "executable"], "C:/clip.exe"),
            )
            for key, path, value in cases:
                with self.subTest(key=key):
                    manifest_path = self.write_clip_download_manifest(repo, archive_path, sha256)
                    with open(manifest_path, "r", encoding="utf-8") as f:
                        payload = json.load(f)
                    cursor = payload
                    for segment in path[:-1]:
                        cursor = cursor[segment]
                    cursor[path[-1]] = value
                    self.write_json(manifest_path, payload)

                    with self.assertRaises(RuntimeError):
                        self.dotfiles_apply.load_downloads_manifest(manifest_path)

    def test_managed_download_rejects_schema_mismatched_manifest_shapes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(repo, exist_ok=True)
            archive_path, sha256 = self.make_clip_archive(tmpdir)
            manifest_path = self.write_clip_download_manifest(repo, archive_path, sha256)

            with open(manifest_path, "r", encoding="utf-8") as f:
                valid_payload = json.load(f)

            cases = (
                ("missing schema", lambda payload: payload.pop("$schema")),
                ("non-string schema", lambda payload: payload.update({"$schema": 1})),
                ("missing version", lambda payload: payload.pop("version")),
                ("wrong version", lambda payload: payload.update({"version": 2})),
                ("missing tools", lambda payload: payload.pop("tools")),
                ("root extra", lambda payload: payload.update({"extra": True})),
                ("tool extra", lambda payload: payload["tools"][0].update({"extra": True})),
                (
                    "target extra",
                    lambda payload: payload["tools"][0]["targets"][0].update({"extra": True}),
                ),
            )
            for name, mutate in cases:
                with self.subTest(name=name):
                    payload = json.loads(json.dumps(valid_payload))
                    mutate(payload)
                    self.write_json(manifest_path, payload)

                    with self.assertRaises(RuntimeError):
                        self.dotfiles_apply.load_downloads_manifest(manifest_path)

    def test_managed_download_rejects_tar_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path = os.path.join(tmpdir, "clip-linux-x86_64-musl.tar.gz")
            with tarfile.open(archive_path, "w:gz") as archive:
                member = tarfile.TarInfo("../evil")
                data = b"evil\n"
                member.size = len(data)
                archive.addfile(member, fileobj=io.BytesIO(data))
            sha256 = self.write_archive_bytes(archive_path)
            self.write_download_manifest_for_archive(repo, archive_path, "tar.gz", sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            self.assertEqual(stats.errors, 1)
            self.assertFalse(os.path.exists(os.path.join(tmpdir, "evil")))
            self.assertIn("unsafe path", self._stderr_buffer.getvalue())

    def test_managed_download_rejects_tar_absolute_path(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path = os.path.join(tmpdir, "clip-linux-x86_64-musl.tar.gz")
            with tarfile.open(archive_path, "w:gz") as archive:
                member = tarfile.TarInfo("/tmp/evil")
                data = b"evil\n"
                member.size = len(data)
                archive.addfile(member, fileobj=io.BytesIO(data))
            sha256 = self.write_archive_bytes(archive_path)
            self.write_download_manifest_for_archive(repo, archive_path, "tar.gz", sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            self.assertEqual(stats.errors, 1)
            self.assertIn("unsafe path", self._stderr_buffer.getvalue())

    def test_managed_download_rejects_tar_links(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            for link_type in (tarfile.SYMTYPE, tarfile.LNKTYPE):
                with self.subTest(link_type=link_type):
                    home = os.path.join(tmpdir, "home", str(link_type))
                    repo = os.path.join(tmpdir, "repo", str(link_type))
                    os.makedirs(home, exist_ok=True)
                    os.makedirs(repo, exist_ok=True)
                    archive_path = os.path.join(tmpdir, f"clip-{link_type}.tar.gz")
                    with tarfile.open(archive_path, "w:gz") as archive:
                        member = tarfile.TarInfo("clip")
                        member.type = link_type
                        member.linkname = "target"
                        archive.addfile(member)
                    sha256 = self.write_archive_bytes(archive_path)
                    self.write_download_manifest_for_archive(repo, archive_path, "tar.gz", sha256)
                    manifest_path = os.path.join(repo, "manifest.json")
                    self.write_json(manifest_path, {"mappings": []})

                    stats = self.apply_managed_download(home, repo, manifest_path)

                    self.assertEqual(stats.errors, 1)
                    self.assertIn("unsafe link", self._stderr_buffer.getvalue())
                    self._stderr_buffer.truncate(0)
                    self._stderr_buffer.seek(0)

    def test_managed_download_rejects_zip_path_traversal(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path = os.path.join(tmpdir, "clip-linux-x86_64-musl.zip")
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("../evil", "evil\n")
            sha256 = self.write_archive_bytes(archive_path)
            self.write_download_manifest_for_archive(repo, archive_path, "zip", sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            self.assertEqual(stats.errors, 1)
            self.assertFalse(os.path.exists(os.path.join(tmpdir, "evil")))
            self.assertIn("unsafe path", self._stderr_buffer.getvalue())

    def test_managed_download_rejects_zip_absolute_path(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path = os.path.join(tmpdir, "clip-linux-x86_64-musl.zip")
            with zipfile.ZipFile(archive_path, "w") as archive:
                archive.writestr("/tmp/evil", "evil\n")
            sha256 = self.write_archive_bytes(archive_path)
            self.write_download_manifest_for_archive(repo, archive_path, "zip", sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            stats = self.apply_managed_download(home, repo, manifest_path)

            self.assertEqual(stats.errors, 1)
            self.assertIn("unsafe path", self._stderr_buffer.getvalue())

    def test_managed_download_rejects_special_tar_members(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            archive_path = os.path.join(tmpdir, "clip-linux-x86_64-musl.tar.gz")
            with tarfile.open(archive_path, "w:gz") as archive:
                member = tarfile.TarInfo("pipe")
                member.type = tarfile.FIFOTYPE
                archive.addfile(member)
            with open(archive_path, "rb") as f:
                sha256 = hashlib.sha256(f.read()).hexdigest()
            self.write_clip_download_manifest(repo, archive_path, sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            installed = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0")
            self.assertEqual(stats.errors, 1)
            self.assertFalse(os.path.exists(installed))
            self.assertIn("unsafe tar member", self._stderr_buffer.getvalue())

    def test_managed_download_preserves_existing_install_when_replacement_fails(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            cached_dir = os.path.join(repo, "bin", ".downloads", "clip", "v1.0.0", "linux-x86_64-musl")
            os.makedirs(cached_dir, exist_ok=True)
            cached_clip = os.path.join(cached_dir, "clip")
            with open(cached_clip, "w", encoding="utf-8") as f:
                f.write("old install\n")
            os.chmod(cached_clip, os.stat(cached_clip).st_mode | stat.S_IXUSR)
            old_archive_path, old_sha256 = self.make_clip_archive(tmpdir, "old archive\n")
            self.write_json(
                os.path.join(cached_dir, ".download.json"),
                self.clip_download_marker(old_archive_path, old_sha256),
            )

            archive_path, sha256 = self.make_clip_archive(tmpdir)
            self.write_clip_download_manifest(repo, archive_path, sha256)
            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(manifest_path, {"mappings": []})
            final_dir = cached_dir

            real_replace = os.replace
            failed_promotion = False
            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="linux",
            ), mock.patch.object(
                self.dotfiles_apply,
                "current_machine_arch",
                return_value="x86_64",
            ), mock.patch.object(self.dotfiles_apply.os, "replace") as replace_mock:
                def replace_side_effect(src, dst):
                    nonlocal failed_promotion
                    if dst == final_dir and not failed_promotion:
                        failed_promotion = True
                        raise OSError("promotion failed")
                    return real_replace(src, dst)

                replace_mock.side_effect = replace_side_effect
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 1)
            with open(cached_clip, "r", encoding="utf-8") as f:
                self.assertEqual(f.read(), "old install\n")
            self.assertIn("promotion failed", self._stderr_buffer.getvalue())

    def test_current_platform_normalizes_windows(self):
        with mock.patch.object(self.dotfiles_apply.sys, "platform", "win32"):
            self.assertEqual(self.dotfiles_apply.current_platform(), "windows")

    def test_parser_defaults_downloads_to_auto(self):
        parser = self.dotfiles_apply.build_parser()

        default_args = parser.parse_args([])
        apply_args = parser.parse_args(["--apply"])
        never_args = parser.parse_args(["--downloads", "never"])

        self.assertFalse(default_args.apply)
        self.assertTrue(apply_args.apply)
        self.assertEqual(default_args.downloads, "auto")
        self.assertEqual(never_args.downloads, "never")

    def test_main_defaults_to_dry_run_and_prints_apply_hint(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(repo, exist_ok=True)
            self.write_json(os.path.join(repo, "downloads.json"), {"tools": []})

            captured = {}

            def fake_apply_manifest(*args, **kwargs):
                captured.update(kwargs)
                return self.dotfiles_apply.Stats()

            with mock.patch.object(self.dotfiles_apply.sys, "argv", ["dotfiles-apply"]), mock.patch.object(
                self.dotfiles_apply,
                "repo_root",
                return_value=repo,
            ), mock.patch.object(
                self.dotfiles_apply,
                "apply_manifest",
                side_effect=fake_apply_manifest,
            ):
                exit_code = self.dotfiles_apply.main()

            self.assertEqual(exit_code, 0)
            self.assertTrue(captured["dry_run"])
            self.assertIn("DRY RUN: no changes were made", self._stdout_buffer.getvalue())
            self.assertIn("--apply", self._stdout_buffer.getvalue())

    def test_main_apply_disables_dry_run_hint(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(repo, exist_ok=True)
            self.write_json(os.path.join(repo, "downloads.json"), {"tools": []})

            captured = {}

            def fake_apply_manifest(*args, **kwargs):
                captured.update(kwargs)
                return self.dotfiles_apply.Stats()

            with mock.patch.object(self.dotfiles_apply.sys, "argv", ["dotfiles-apply", "--apply"]), mock.patch.object(
                self.dotfiles_apply,
                "repo_root",
                return_value=repo,
            ), mock.patch.object(
                self.dotfiles_apply,
                "apply_manifest",
                side_effect=fake_apply_manifest,
            ):
                exit_code = self.dotfiles_apply.main()

            self.assertEqual(exit_code, 0)
            self.assertFalse(captured["dry_run"])
            self.assertNotIn("DRY RUN: no changes were made", self._stdout_buffer.getvalue())

    def test_repo_downloads_manifest_targets_linux_and_macos_only(self):
        manifest_path = pathlib.Path(__file__).resolve().parents[2] / "downloads.json"

        downloads = self.dotfiles_apply.parse_downloads_manifest(str(manifest_path))
        targets = downloads.targets
        targets_by_tool = {}
        for target in targets:
            targets_by_tool.setdefault(target.tool, []).append(target)

        self.assertEqual(
            [target.target for target in targets_by_tool["clip"]],
            ["linux-x86_64-musl", "macos-aarch64"],
        )
        self.assertTrue(
            all("clip-latest" in target.url for target in targets_by_tool["clip"])
        )
        self.assertTrue(
            all("clip-v1.0.0" not in target.url for target in targets_by_tool["clip"])
        )
        self.assertEqual(
            [target.target for target in targets_by_tool["rproxy"]],
            ["linux-x86_64-gnu", "macos-aarch64"],
        )
        self.assertTrue(
            all("rproxy-latest" in target.url for target in targets_by_tool["rproxy"])
        )
        self.assertEqual(
            [target.executable for target in targets_by_tool["rproxy"]],
            ["rproxy", "rproxy"],
        )
        self.assertEqual([repo.name for repo in downloads.git_repos], ["tide"])
        self.assertEqual(downloads.git_repos[0].target, ".managed/tide")

    def test_repo_default_manifest_is_valid(self):
        manifest_path = pathlib.Path(__file__).resolve().parents[2] / "dotfiles-map.json"
        schema_path = pathlib.Path(__file__).resolve().parents[2] / "dotfiles-map.schema.json"

        with open(manifest_path, "r", encoding="utf-8") as f:
            manifest = json.load(f)
        with open(schema_path, "r", encoding="utf-8") as f:
            schema = json.load(f)

        mappings = self.dotfiles_apply.load_manifest(str(manifest_path))
        mappings_by_name = {mapping.name: mapping for mapping in mappings}

        self.assertEqual(manifest["$schema"], "./dotfiles-map.schema.json")
        self.assertEqual(schema["title"], "Dotfiles Mapping Manifest")
        self.assertIn("mapping", schema["$defs"])
        self.assertIn("fish", mappings_by_name)
        self.assertNotIn("opencode-systemd-user", mappings_by_name)
        self.assertNotIn("opencode-launchagent", mappings_by_name)

        opencode_config = mappings_by_name["opencode"]
        self.assertIn("opencode.service", opencode_config.exclude)
        self.assertIn("opencode.plist", opencode_config.exclude)

        for name in ("niri", "ironbar", "keyd"):
            if name in mappings_by_name:
                self.assertEqual(mappings_by_name[name].platforms, ["linux"])

    def test_repo_opencode_user_service_runs_requested_command(self):
        service_path = (
            pathlib.Path(__file__).resolve().parents[2]
            / "opencode"
            / "opencode.service"
        )

        service = service_path.read_text(encoding="utf-8")

        self.assertIn(
            "ExecStart=/usr/bin/fnm exec --using=default opencode serve --hostname 0.0.0.0 --port 4096",
            service,
        )
        self.assertIn("Restart=on-failure", service)
        self.assertIn("WantedBy=default.target", service)

    def test_repo_opencode_launchagent_runs_requested_command(self):
        service_path = (
            pathlib.Path(__file__).resolve().parents[2]
            / "opencode"
            / "opencode.plist"
        )

        service = service_path.read_text(encoding="utf-8")

        self.assertIn("<string>opencode.server</string>", service)
        self.assertIn(
            "<string>fnm exec --using=default opencode serve --hostname 0.0.0.0 --port 4096</string>",
            service,
        )
        self.assertIn("<key>RunAtLoad</key>", service)
        self.assertIn("<key>KeepAlive</key>", service)

    def test_link_children_preserves_local_files_and_excludes(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            fish_dir = os.path.join(repo, "fish")
            os.makedirs(os.path.join(fish_dir, "functions"), exist_ok=True)
            os.makedirs(home, exist_ok=True)

            with open(os.path.join(fish_dir, "config.fish"), "w", encoding="utf-8") as f:
                f.write("set fish_greeting\n")
            with open(
                os.path.join(fish_dir, "fish_plugins"), "w", encoding="utf-8"
            ) as f:
                f.write("jorgebucaran/fisher\n")
            with open(
                os.path.join(fish_dir, "fish_variables"), "w", encoding="utf-8"
            ) as f:
                f.write("repo state\n")
            with open(
                os.path.join(fish_dir, "functions", "hello.fish"), "w", encoding="utf-8"
            ) as f:
                f.write("function hello\nend\n")

            target_fish = os.path.join(home, ".config", "fish")
            os.makedirs(target_fish, exist_ok=True)
            local_variables = os.path.join(target_fish, "fish_variables")
            with open(local_variables, "w", encoding="utf-8") as f:
                f.write("local state\n")

            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(
                manifest_path,
                {
                    "mappings": [
                        {
                            "name": "fish",
                            "source": "fish",
                            "target": "~/.config/fish",
                            "mode": "link_children",
                            "exclude": ["fish_variables*"],
                        }
                    ]
                },
            )

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertTrue(os.path.islink(os.path.join(home, "dotfiles")))
            config_path = os.path.join(target_fish, "config.fish")
            functions_path = os.path.join(target_fish, "functions")
            self.assertTrue(os.path.islink(config_path))
            self.assertTrue(os.path.islink(functions_path))
            self.assertEqual(
                os.path.realpath(config_path),
                os.path.realpath(os.path.join(repo, "fish", "config.fish")),
            )
            self.assertEqual(
                os.path.realpath(functions_path),
                os.path.realpath(os.path.join(repo, "fish", "functions")),
            )
            self.assertFalse(os.path.islink(local_variables))
            with open(local_variables, "r", encoding="utf-8") as f:
                self.assertEqual(f.read(), "local state\n")

    def test_platforms_filter_skips_non_matching_mapping(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            niri_dir = os.path.join(repo, "niri")
            os.makedirs(niri_dir, exist_ok=True)
            os.makedirs(home, exist_ok=True)

            with open(os.path.join(niri_dir, "config.kdl"), "w", encoding="utf-8") as f:
                f.write("layout {}\n")

            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(
                manifest_path,
                {
                    "mappings": [
                        {
                            "name": "niri",
                            "source": "niri/config.kdl",
                            "target": "~/.config/niri/config.kdl",
                            "mode": "symlink",
                            "platforms": ["linux"],
                        }
                    ]
                },
            )

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False), mock.patch.object(
                self.dotfiles_apply,
                "current_platform",
                return_value="macos",
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertFalse(
                os.path.lexists(os.path.join(home, ".config", "niri", "config.kdl"))
            )

    def test_dry_run_does_not_create_links(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            with open(os.path.join(repo, "gitconfig"), "w", encoding="utf-8") as f:
                f.write("[user]\n")

            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(
                manifest_path,
                {
                    "mappings": [
                        {
                            "name": "git",
                            "source": "gitconfig",
                            "target": "~/.gitconfig",
                            "mode": "symlink",
                        }
                    ]
                },
            )

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False):
                stats = self.dotfiles_apply.apply_manifest(
                    repo,
                    manifest_path,
                    dry_run=True,
                )

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.lexists(os.path.join(home, "dotfiles")))
            self.assertFalse(os.path.lexists(os.path.join(home, ".gitconfig")))

    def test_force_replaces_conflicting_file_and_keeps_backup(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            os.makedirs(home, exist_ok=True)
            os.makedirs(repo, exist_ok=True)
            with open(os.path.join(repo, "gitconfig"), "w", encoding="utf-8") as f:
                f.write("[user]\n")

            target_path = os.path.join(home, ".gitconfig")
            with open(target_path, "w", encoding="utf-8") as f:
                f.write("local\n")

            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(
                manifest_path,
                {
                    "mappings": [
                        {
                            "name": "git",
                            "source": "gitconfig",
                            "target": "~/.gitconfig",
                            "mode": "symlink",
                        }
                    ]
                },
            )

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False):
                stats = self.dotfiles_apply.apply_manifest(
                    repo,
                    manifest_path,
                    force=True,
                )

            self.assertEqual(stats.errors, 0)
            self.assertTrue(os.path.islink(target_path))
            self.assertEqual(
                os.path.realpath(target_path),
                os.path.realpath(os.path.join(repo, "gitconfig")),
            )
            backup_path = target_path + ".bak"
            self.assertTrue(os.path.exists(backup_path))
            with open(backup_path, "r", encoding="utf-8") as f:
                self.assertEqual(f.read(), "local\n")

    def test_prune_removes_stale_managed_child_links(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            home = os.path.join(tmpdir, "home")
            repo = os.path.join(tmpdir, "repo")
            source_dir = os.path.join(repo, "opencode")
            target_dir = os.path.join(home, ".config", "opencode")
            os.makedirs(source_dir, exist_ok=True)
            os.makedirs(target_dir, exist_ok=True)
            os.makedirs(home, exist_ok=True)

            with open(os.path.join(source_dir, "opencode.json"), "w", encoding="utf-8") as f:
                f.write("{}\n")

            manifest_path = os.path.join(repo, "manifest.json")
            self.write_json(
                manifest_path,
                {
                    "mappings": [
                        {
                            "name": "opencode",
                            "source": "opencode",
                            "target": "~/.config/opencode",
                            "mode": "link_children",
                        }
                    ]
                },
            )

            with mock.patch.dict(os.environ, {"HOME": home}, clear=False):
                self.dotfiles_apply.apply_manifest(repo, manifest_path)
                stale = os.path.join(target_dir, "old.json")
                os.symlink(os.path.join(home, "dotfiles", "opencode", "old.json"), stale)
                stats = self.dotfiles_apply.apply_manifest(
                    repo,
                    manifest_path,
                    prune=True,
                )

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.lexists(stale))


if __name__ == "__main__":
    unittest.main()

import importlib.machinery
import importlib.util
import json
import os
import pathlib
import sys
import tempfile
import unittest
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

    def test_repo_default_manifest_is_valid(self):
        manifest_path = pathlib.Path(__file__).resolve().parents[2] / "dotfiles-map.json"
        schema_path = pathlib.Path(__file__).resolve().parents[2] / "dotfiles-map.schema.json"

        with open(manifest_path, "r", encoding="utf-8") as f:
            manifest = json.load(f)
        with open(schema_path, "r", encoding="utf-8") as f:
            schema = json.load(f)

        mappings = self.dotfiles_apply.load_manifest(str(manifest_path))

        self.assertEqual(manifest["$schema"], "./dotfiles-map.schema.json")
        self.assertEqual(schema["title"], "Dotfiles Mapping Manifest")
        self.assertIn("mapping", schema["$defs"])
        self.assertTrue(any(mapping.name == "fish" for mapping in mappings))

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

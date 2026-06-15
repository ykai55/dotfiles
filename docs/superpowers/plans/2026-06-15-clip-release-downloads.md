# Clip Release Downloads Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Git LFS-managed `clip` binaries with GitHub Release assets that `dotfiles-apply` downloads into `bin/.downloads`.

**Architecture:** Keep `clip/` independent by moving download metadata to root-level `downloads.json`. `dotfiles-apply` owns download, checksum, extraction, and `current` symlink management. `bin/clip` remains a small runtime wrapper that executes `bin/.downloads/clip/current/<target>/<binary>`.

**Tech Stack:** Python stdlib (`json`, `hashlib`, `urllib.request`, `tarfile`, `zipfile`, `tempfile`, `os`, `shutil`), Bash wrapper, GitHub Actions, unittest with mocks.

---

## File Structure

- Create `downloads.json`: root manifest for externally downloaded tools. Initial content describes `clip` release assets.
- Modify `.gitignore`: ignore `bin/.downloads/` and local release download output.
- Modify `clip/.gitattributes`: remove the `dist/**` Git LFS rule after `clip/dist/**` is no longer source-controlled.
- Delete tracked `clip/dist/**` binaries: remove LFS-managed release artifacts from the repository.
- Modify `bin/clip`: resolve downloaded binaries from `bin/.downloads/clip/current/<target>`.
- Modify `bin/dotfiles-apply`: add download manifest loading, platform target detection, archive download, checksum verification, extraction, executable setup, and `current` symlink update.
- Modify `bin/tests/test_clip.py`: update fake binary layout and missing-binary assertions.
- Modify `bin/tests/test_dotfiles_apply.py`: add mocked managed-download tests.
- Create `.github/workflows/clip-release.yml`: build `clip` targets and publish durable Release assets.

## Task 1: Update `bin/clip` Wrapper Tests

**Files:**
- Modify: `bin/tests/test_clip.py`
- Modify later: `bin/clip`

- [ ] **Step 1: Change fake binary installation to the new download layout**

Replace `install_fake_clip_binary` in `bin/tests/test_clip.py` with:

```python
    def install_fake_clip_binary(self, repo_root: pathlib.Path, relpath: str) -> None:
        write_executable(
            repo_root / "bin" / ".downloads" / "clip" / "v1.0.0" / relpath,
            f"""#!/usr/bin/env bash
            set -euo pipefail
            printf 'TARGET={relpath}\n'
            for arg in "$@"; do
              printf 'ARG=%s\n' "$arg"
            done
            """,
        )
        current = repo_root / "bin" / ".downloads" / "clip" / "current"
        if not current.exists():
            current.symlink_to("v1.0.0")
```

- [ ] **Step 2: Update missing-binary expectation**

Replace the final assertion in `test_missing_binary_reports_error` with:

```python
        self.assertIn("clip wrapper: missing downloaded clip binary:", proc.stderr)
        self.assertIn("bin/.downloads/clip/current/macos-aarch64/clip", proc.stderr)
        self.assertIn("run bin/dotfiles-apply", proc.stderr)
```

- [ ] **Step 3: Run the wrapper tests and verify failure**

Run:

```sh
python -m unittest bin/tests/test_clip.py
```

Expected: the platform selection tests fail because `bin/clip` still looks in `clip/dist`.

## Task 2: Update `bin/clip` Runtime Path

**Files:**
- Modify: `bin/clip`
- Test: `bin/tests/test_clip.py`

- [ ] **Step 1: Change `bin/clip` to use `bin/.downloads`**

In `bin/clip`, replace the line that sets `clip_bin` with:

```bash
  clip_bin="$repo_root/bin/.downloads/clip/current/$target_dir/$binary_name"
```

Replace the missing binary error block with:

```bash
  if [[ ! -x "$clip_bin" ]]; then
    fail "missing downloaded clip binary: $clip_bin; run bin/dotfiles-apply"
  fi
```

- [ ] **Step 2: Run wrapper tests**

Run:

```sh
python -m unittest bin/tests/test_clip.py
```

Expected: all tests in `bin/tests/test_clip.py` pass.

- [ ] **Step 3: Stage and commit only if commits are requested**

If the user explicitly requested commits, run:

```sh
git add bin/clip bin/tests/test_clip.py
git commit -m "update clip wrapper download path"
```

Otherwise, do not commit.

## Task 3: Add Root Download Manifest And Ignore Local Downloads

**Files:**
- Create: `downloads.json`
- Modify: `.gitignore`

- [ ] **Step 1: Create `downloads.json` with a safe initial release manifest**

Create root `downloads.json` with this exact structure. Replace the owner/repo in URLs only if `git remote get-url origin` shows a different GitHub repository.

```json
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
          "url": "https://github.com/kai/dotfiles/releases/download/clip-v1.0.0/clip-linux-x86_64-musl.tar.gz",
          "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
          "archive": "tar.gz",
          "executable": "clip"
        },
        {
          "target": "macos-aarch64",
          "platform": "macos",
          "arch": "aarch64",
          "url": "https://github.com/kai/dotfiles/releases/download/clip-v1.0.0/clip-macos-aarch64.tar.gz",
          "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
          "archive": "tar.gz",
          "executable": "clip"
        },
        {
          "target": "windows-x86_64-gnu",
          "platform": "windows",
          "arch": "x86_64",
          "url": "https://github.com/kai/dotfiles/releases/download/clip-v1.0.0/clip-windows-x86_64-gnu.zip",
          "sha256": "0000000000000000000000000000000000000000000000000000000000000000",
          "archive": "zip",
          "executable": "clip.exe"
        }
      ]
    }
  ]
}
```

Before finalizing the implementation, update `version`, `url`, and `sha256` to real Release values produced by CI. The all-zero checksums are intentionally invalid so accidental downloads fail safely until real assets exist.

- [ ] **Step 2: Ignore local downloaded binaries**

Add these lines to `.gitignore`:

```gitignore
bin/.downloads/
```

- [ ] **Step 3: Verify manifest parses**

Run:

```sh
python -m json.tool downloads.json >/dev/null
```

Expected: exit status 0 and no output.

## Task 4: Add Managed Download Tests To `dotfiles-apply`

**Files:**
- Modify: `bin/tests/test_dotfiles_apply.py`
- Modify later: `bin/dotfiles-apply`

- [ ] **Step 1: Add imports needed by new tests**

At the top of `bin/tests/test_dotfiles_apply.py`, add:

```python
import hashlib
import stat
import tarfile
```

- [ ] **Step 2: Add helper methods to `DotfilesApplyTests`**

Inside `DotfilesApplyTests`, after `write_json`, add:

```python
    def write_clip_download_manifest(self, repo, archive_path, sha256):
        manifest_path = os.path.join(repo, "downloads.json")
        self.write_json(
            manifest_path,
            {
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
                                "sha256": sha256,
                                "archive": "tar.gz",
                                "executable": "clip",
                            }
                        ],
                    }
                ],
            },
        )
        return manifest_path

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
```

- [ ] **Step 3: Add dry-run test**

Add this test method to `DotfilesApplyTests`:

```python
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
            ):
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path, dry_run=True)

            self.assertEqual(stats.errors, 0)
            self.assertFalse(os.path.exists(os.path.join(repo, "bin", ".downloads")))
            self.assertIn("[download] clip linux-x86_64-musl", self.stdout.getvalue())
```

- [ ] **Step 4: Add install test**

Add this test method to `DotfilesApplyTests`:

```python
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
```

- [ ] **Step 5: Add cached install test**

Add this test method to `DotfilesApplyTests`:

```python
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
                stats = self.dotfiles_apply.apply_manifest(repo, manifest_path)

            self.assertEqual(stats.errors, 0)
            self.assertIn("[ok] clip linux-x86_64-musl", self.stdout.getvalue())
```

- [ ] **Step 6: Add checksum mismatch test**

Add this test method to `DotfilesApplyTests`:

```python
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
            self.assertIn("sha256 mismatch", self.stderr.getvalue())
```

- [ ] **Step 7: Run tests and verify failure**

Run:

```sh
python -m unittest bin/tests/test_dotfiles_apply.py
```

Expected: failures mention missing `current_machine_arch` and missing managed download behavior.

## Task 5: Implement Managed Downloads In `dotfiles-apply`

**Files:**
- Modify: `bin/dotfiles-apply`
- Test: `bin/tests/test_dotfiles_apply.py`

- [ ] **Step 1: Add imports**

In `bin/dotfiles-apply`, add these imports with the existing stdlib imports:

```python
import hashlib
import platform
import tarfile
import tempfile
import urllib.request
import zipfile
```

- [ ] **Step 2: Add download dataclass**

After `Mapping`, add:

```python
@dataclass
class DownloadTarget:
    tool: str
    version: str
    target: str
    platform: str
    arch: str
    url: str
    sha256: str
    archive: str
    executable: str
```

- [ ] **Step 3: Add architecture detection**

After `current_platform`, add:

```python
def current_machine_arch() -> str:
    machine = platform.machine().lower()
    if machine in {"x86_64", "amd64"}:
        return "x86_64"
    if machine in {"arm64", "aarch64"}:
        return "aarch64"
    return machine
```

- [ ] **Step 4: Add download manifest parser**

After `load_manifest`, add:

```python
def load_downloads_manifest(path: str) -> list[DownloadTarget]:
    if not os.path.exists(path):
        return []
    with open(path, "r", encoding="utf-8") as f:
        raw = json.load(f)

    if not isinstance(raw, dict):
        raise RuntimeError("Downloads manifest root must be a JSON object")
    tools = raw.get("tools", [])
    if not isinstance(tools, list):
        raise RuntimeError("Downloads manifest must contain a 'tools' array")

    loaded: list[DownloadTarget] = []
    for tool_index, tool in enumerate(tools, start=1):
        if not isinstance(tool, dict):
            raise RuntimeError(f"Download tool #{tool_index} must be an object")
        name = tool.get("name")
        version = tool.get("version")
        targets = tool.get("targets", [])
        if not isinstance(name, str) or not name:
            raise RuntimeError(f"Download tool #{tool_index} has invalid 'name'")
        if not isinstance(version, str) or not version:
            raise RuntimeError(f"Download tool {name} has invalid 'version'")
        if not isinstance(targets, list):
            raise RuntimeError(f"Download tool {name} has invalid 'targets'")
        for target_index, item in enumerate(targets, start=1):
            if not isinstance(item, dict):
                raise RuntimeError(f"Download target #{target_index} for {name} must be an object")
            values = {
                key: item.get(key)
                for key in ("target", "platform", "arch", "url", "sha256", "archive", "executable")
            }
            for key, value in values.items():
                if not isinstance(value, str) or not value:
                    raise RuntimeError(f"Download target #{target_index} for {name} has invalid '{key}'")
            if values["archive"] not in {"tar.gz", "zip"}:
                raise RuntimeError(f"Download target #{target_index} for {name} has unsupported archive")
            if len(values["sha256"]) != 64:
                raise RuntimeError(f"Download target #{target_index} for {name} has invalid sha256")
            loaded.append(DownloadTarget(tool=name, version=version, **values))
    return loaded
```

- [ ] **Step 5: Add helpers for download paths, hashing, extraction, and symlink update**

After `remove_path`, add:

```python
def download_root(actual_repo_root: str, target: DownloadTarget) -> str:
    return os.path.join(actual_repo_root, "bin", ".downloads", target.tool)


def download_target_dir(actual_repo_root: str, target: DownloadTarget) -> str:
    return os.path.join(download_root(actual_repo_root, target), target.version, target.target)


def download_executable_path(actual_repo_root: str, target: DownloadTarget) -> str:
    return os.path.join(download_target_dir(actual_repo_root, target), target.executable)


def file_sha256(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def download_file(url: str, output_path: str) -> None:
    with urllib.request.urlopen(url) as response, open(output_path, "wb") as f:
        shutil.copyfileobj(response, f)


def extract_archive(archive_path: str, archive_type: str, target_dir: str) -> None:
    if archive_type == "tar.gz":
        with tarfile.open(archive_path, "r:gz") as archive:
            archive.extractall(target_dir)
        return
    if archive_type == "zip":
        with zipfile.ZipFile(archive_path) as archive:
            archive.extractall(target_dir)
        return
    raise RuntimeError(f"Unsupported archive type: {archive_type}")


def update_current_download_link(root: str, version: str) -> None:
    current = os.path.join(root, "current")
    tmp_link = os.path.join(root, ".current.tmp")
    if os.path.lexists(tmp_link):
        os.remove(tmp_link)
    os.symlink(version, tmp_link)
    os.replace(tmp_link, current)
```

- [ ] **Step 6: Add `ensure_download_target`**

After the helper functions from Step 5, add:

```python
def ensure_download_target(
    actual_repo_root: str,
    target: DownloadTarget,
    dry_run: bool,
    stats: Stats,
) -> None:
    executable_path = download_executable_path(actual_repo_root, target)
    root = download_root(actual_repo_root, target)
    if os.path.isfile(executable_path) and os.access(executable_path, os.X_OK):
        print_info(f"[ok] {target.tool} {target.target}")
        if not dry_run:
            update_current_download_link(root, target.version)
        return

    print_info(f"[download] {target.tool} {target.target} -> {executable_path}")
    if dry_run:
        return

    os.makedirs(root, exist_ok=True)
    version_dir = os.path.join(root, target.version)
    final_dir = os.path.join(version_dir, target.target)
    with tempfile.TemporaryDirectory(prefix=f".{target.tool}-download-", dir=root) as tmpdir:
        archive_path = os.path.join(tmpdir, "asset")
        extract_dir = os.path.join(tmpdir, "extract")
        os.makedirs(extract_dir, exist_ok=True)
        download_file(target.url, archive_path)
        actual_sha256 = file_sha256(archive_path)
        if actual_sha256 != target.sha256.lower():
            raise RuntimeError(
                f"sha256 mismatch for {target.tool} {target.target}: "
                f"expected {target.sha256.lower()} got {actual_sha256}"
            )
        extract_archive(archive_path, target.archive, extract_dir)
        extracted_executable = os.path.join(extract_dir, target.executable)
        if not os.path.isfile(extracted_executable):
            raise RuntimeError(f"Archive for {target.tool} {target.target} is missing {target.executable}")
        os.chmod(extracted_executable, os.stat(extracted_executable).st_mode | 0o755)
        if os.path.exists(final_dir):
            shutil.rmtree(final_dir)
        os.makedirs(version_dir, exist_ok=True)
        os.replace(extract_dir, final_dir)
    update_current_download_link(root, target.version)
```

- [ ] **Step 7: Add `ensure_managed_downloads`**

After `ensure_download_target`, add:

```python
def ensure_managed_downloads(actual_repo_root: str, dry_run: bool, stats: Stats) -> None:
    downloads_path = os.path.join(actual_repo_root, "downloads.json")
    targets = load_downloads_manifest(downloads_path)
    platform_name = current_platform()
    arch_name = current_machine_arch()
    for target in targets:
        if target.platform != platform_name or target.arch != arch_name:
            continue
        try:
            ensure_download_target(actual_repo_root, target, dry_run, stats)
        except (OSError, RuntimeError, tarfile.TarError, zipfile.BadZipFile) as exc:
            print_warning(f"[error] {target.tool}: {exc}")
            stats.errors += 1
```

- [ ] **Step 8: Call downloads before applying mappings**

In `apply_manifest`, after `ensure_home_dotfiles_link` succeeds and before iterating mappings, add:

```python
    ensure_managed_downloads(actual_repo_root, dry_run, stats)
```

- [ ] **Step 9: Require the root downloads manifest in the CLI path**

In `main`, after `actual_repo_root = repo_root()` and before `manifest_path = expand_target(args.manifest)`, add:

```python
    downloads_path = os.path.join(actual_repo_root, "downloads.json")
    if not os.path.exists(downloads_path):
        print_warning(f"[error] Missing downloads manifest: {downloads_path}")
        return 1
```

This keeps direct unit tests with synthetic repositories simple while making the real `dotfiles-apply` command fail clearly if the root manifest is missing.

- [ ] **Step 10: Run `dotfiles-apply` tests**

Run:

```sh
python -m unittest bin/tests/test_dotfiles_apply.py
```

Expected: all tests in `bin/tests/test_dotfiles_apply.py` pass.

## Task 6: Remove LFS-Managed `clip/dist` Files

**Files:**
- Delete: `clip/dist/linux-x86_64-musl/clip`
- Delete: `clip/dist/macos-aarch64/clip`
- Delete: `clip/dist/macos-aarch64/clip-macos-helper`
- Delete: `clip/dist/windows-x86_64-gnu/clip.exe`
- Modify: `clip/.gitattributes`

- [ ] **Step 1: Remove LFS rule**

Replace `clip/.gitattributes` content with an empty file, or delete `clip/.gitattributes` if no attributes remain.

- [ ] **Step 2: Delete tracked dist binaries with apply_patch**

Remove the four tracked dist files listed above. Do not remove `clip/target/**` files unless they are part of a separate cleanup requested by the user.

- [ ] **Step 3: Verify no source references to `clip/dist` remain**

Run:

```sh
python -m unittest bin/tests/test_clip.py
```

Expected: tests pass and the wrapper no longer depends on `clip/dist`.

## Task 7: Add Clip Release Workflow

**Files:**
- Create: `.github/workflows/clip-release.yml`

- [ ] **Step 1: Add release workflow**

Create `.github/workflows/clip-release.yml`:

```yaml
name: Build clip release assets

on:
  push:
    tags:
      - "clip-v*"
  workflow_dispatch:

permissions:
  contents: write

jobs:
  build-linux:
    name: Linux x86_64 musl
    runs-on: ubuntu-24.04
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust target
        run: rustup target add x86_64-unknown-linux-musl

      - name: Install musl tools
        run: sudo apt-get update && sudo apt-get install -y --no-install-recommends musl-tools

      - name: Build
        working-directory: clip
        run: cargo build --release --target x86_64-unknown-linux-musl

      - name: Package
        run: |
          set -euo pipefail
          mkdir -p dist/clip-linux-x86_64-musl
          cp clip/target/x86_64-unknown-linux-musl/release/clip dist/clip-linux-x86_64-musl/clip
          tar -C dist/clip-linux-x86_64-musl -czf dist/clip-linux-x86_64-musl.tar.gz clip
          sha256sum dist/clip-linux-x86_64-musl.tar.gz > dist/clip-linux-x86_64-musl.tar.gz.sha256

      - name: Upload build artifact
        uses: actions/upload-artifact@v4
        with:
          name: clip-linux-x86_64-musl
          path: |
            dist/clip-linux-x86_64-musl.tar.gz
            dist/clip-linux-x86_64-musl.tar.gz.sha256

  build-macos:
    name: macOS aarch64
    runs-on: macos-14
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Build Rust CLI
        working-directory: clip
        run: cargo build --release

      - name: Build macOS helper
        working-directory: clip/helpers/macos/clip-macos-helper
        run: swift build -c release

      - name: Package
        run: |
          set -euo pipefail
          mkdir -p dist/clip-macos-aarch64
          cp clip/target/release/clip dist/clip-macos-aarch64/clip
          cp clip/helpers/macos/clip-macos-helper/.build/release/clip-macos-helper dist/clip-macos-aarch64/clip-macos-helper
          tar -C dist/clip-macos-aarch64 -czf dist/clip-macos-aarch64.tar.gz clip clip-macos-helper
          shasum -a 256 dist/clip-macos-aarch64.tar.gz > dist/clip-macos-aarch64.tar.gz.sha256

      - name: Upload build artifact
        uses: actions/upload-artifact@v4
        with:
          name: clip-macos-aarch64
          path: |
            dist/clip-macos-aarch64.tar.gz
            dist/clip-macos-aarch64.tar.gz.sha256

  build-windows:
    name: Windows x86_64 gnu
    runs-on: windows-2022
    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Rust target
        run: rustup target add x86_64-pc-windows-gnu

      - name: Build
        working-directory: clip
        run: cargo build --release --target x86_64-pc-windows-gnu

      - name: Package
        shell: pwsh
        run: |
          New-Item -ItemType Directory -Force -Path dist/clip-windows-x86_64-gnu | Out-Null
          Copy-Item clip/target/x86_64-pc-windows-gnu/release/clip.exe dist/clip-windows-x86_64-gnu/clip.exe
          Compress-Archive -Path dist/clip-windows-x86_64-gnu/clip.exe -DestinationPath dist/clip-windows-x86_64-gnu.zip -Force
          $hash = (Get-FileHash dist/clip-windows-x86_64-gnu.zip -Algorithm SHA256).Hash.ToLower()
          "$hash  dist/clip-windows-x86_64-gnu.zip" | Out-File -Encoding ascii dist/clip-windows-x86_64-gnu.zip.sha256

      - name: Upload build artifact
        uses: actions/upload-artifact@v4
        with:
          name: clip-windows-x86_64-gnu
          path: |
            dist/clip-windows-x86_64-gnu.zip
            dist/clip-windows-x86_64-gnu.zip.sha256

  release:
    name: Publish GitHub Release assets
    runs-on: ubuntu-24.04
    needs:
      - build-linux
      - build-macos
      - build-windows
    if: startsWith(github.ref, 'refs/tags/clip-v')
    steps:
      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist
          merge-multiple: true

      - name: Publish release
        uses: softprops/action-gh-release@v2
        with:
          files: |
            dist/clip-linux-x86_64-musl.tar.gz
            dist/clip-linux-x86_64-musl.tar.gz.sha256
            dist/clip-macos-aarch64.tar.gz
            dist/clip-macos-aarch64.tar.gz.sha256
            dist/clip-windows-x86_64-gnu.zip
            dist/clip-windows-x86_64-gnu.zip.sha256
```

- [ ] **Step 2: Validate YAML is parseable if Ruby is available**

Run:

```sh
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/clip-release.yml")'
```

Expected: exit status 0. If Ruby is not installed, skip this command and rely on GitHub Actions syntax review.

## Task 8: Full Verification

**Files:**
- Verify all changed files.

- [ ] **Step 1: Run focused tests**

Run:

```sh
python -m unittest bin/tests/test_dotfiles_apply.py bin/tests/test_clip.py
```

Expected: all tests pass.

- [ ] **Step 2: Run main repository tests**

Run:

```sh
python -m unittest bin/tests/test_tmux_load.py bin/tests/test_tmux_dump.py bin/tests/test_tbox.py bin/tests/test_tbox_integration.py
```

Expected: all tests pass.

- [ ] **Step 3: Inspect git state**

Run:

```sh
git status --short
```

Expected: only intended files are modified, plus any unrelated pre-existing user changes that must not be reverted.

- [ ] **Step 4: Inspect diff**

Run:

```sh
git diff -- bin/clip bin/dotfiles-apply bin/tests/test_clip.py bin/tests/test_dotfiles_apply.py downloads.json .gitignore clip/.gitattributes .github/workflows/clip-release.yml
```

Expected: diff matches this plan and does not include unrelated reformatting.

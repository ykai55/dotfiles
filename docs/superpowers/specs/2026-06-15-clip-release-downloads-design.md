# Clip Release Downloads Design

## Context

The dotfiles repository currently keeps prebuilt `clip` binaries under
`clip/dist/**`, with `clip/.gitattributes` marking those files for Git LFS. The
`bin/clip` wrapper selects a platform-specific binary from `clip/dist/<target>`
and executes it. `bin/dotfiles-apply` currently applies symlinks only; it does
not download external binaries or verify binary assets.

The goal is to remove the prebuilt `clip` binaries from Git LFS, build them in
GitHub CI, publish them as durable GitHub Release assets, and have
`dotfiles-apply` download the correct binary into `bin/.downloads` during
application.

## Requirements

- Keep the `clip` project independent from dotfiles installation policy.
- Store download metadata at the dotfiles repository root in `downloads.json`.
- Publish CI-built `clip` packages as GitHub Release assets, not short-lived
  workflow artifacts.
- Download only the asset for the current platform and architecture.
- Place downloaded files under `bin/.downloads`.
- Verify downloads before making them executable or using them.
- Preserve `--dry-run` behavior by reporting planned downloads without writing
  files.
- Avoid requiring Git LFS for `clip` binaries.

## Download Manifest

Add a root-level `downloads.json` owned by the dotfiles repository. It describes
externally downloaded tools and their platform-specific assets. The initial
manifest contains only `clip`.

The manifest should include:

- Tool name, for example `clip`.
- Version, matching the GitHub Release tag or asset version.
- Platform target, for example `linux-x86_64-musl`, `macos-aarch64`, or
  `windows-x86_64-gnu`.
- Asset URL.
- SHA-256 checksum for the downloaded archive.
- Archive type, such as `tar.gz` or `zip`.
- Executable path inside the extracted archive.

`downloads.json` stays outside `clip/` so the `clip` source tree remains a normal
standalone project. The dotfiles repository owns how release binaries are
located, cached, and installed.

## CI And Release Publishing

Add or extend GitHub Actions workflow coverage for `clip` release builds. CI
builds the supported targets and packages each target into a small archive:

- `clip-linux-x86_64-musl.tar.gz`
- `clip-macos-aarch64.tar.gz`
- `clip-windows-x86_64-gnu.zip`

The macOS package includes both `clip` and `clip-macos-helper` if the helper is
required at runtime. Linux and Windows packages include their target executable.

Release publishing should run on tags and optionally via `workflow_dispatch`.
The workflow uploads the archives and checksum files to the GitHub Release for
the selected version. Durable Release assets are used because GitHub workflow
artifacts can expire and can require API/auth handling.

## dotfiles-apply Behavior

`dotfiles-apply` gains a preflight step for managed downloads. It reads
`downloads.json`, selects entries matching the current platform and architecture,
and ensures each required tool exists in `bin/.downloads`.

For `clip`, the target layout is:

```text
bin/.downloads/clip/<version>/<target>/...
bin/.downloads/clip/current -> <version>
```

When the expected executable is missing, or when a marker/checksum indicates the
cached archive does not match the manifest, `dotfiles-apply` downloads the asset
to a temporary file, verifies SHA-256, extracts it into a temporary directory,
sets executable permissions where needed, and atomically moves it into the final
download directory. After a successful install, `dotfiles-apply` updates
`bin/.downloads/clip/current` to point at the active version directory. This
gives runtime wrappers a stable path while keeping versioned downloads available
for inspection or rollback.

If `--dry-run` is set, `dotfiles-apply` prints the planned download and target
path but does not create `bin/.downloads` or fetch network content.

If the download, checksum verification, or extraction fails, `dotfiles-apply`
prints an explicit error, increments its error count, and exits non-zero.

## bin/clip Behavior

`bin/clip` remains a small wrapper. It detects the current platform and
architecture, then executes the downloaded binary from
`bin/.downloads/clip/current/<target>` instead of `clip/dist`.

If the expected binary is missing, the wrapper reports a clear error such as:

```text
clip wrapper: missing downloaded clip binary: <path>; run bin/dotfiles-apply
```

The wrapper does not perform network downloads. This keeps runtime behavior
predictable and centralizes installation side effects in `dotfiles-apply`.

## LFS Removal

Remove `clip/dist/**` binaries from the repository and stop tracking them with
Git LFS. `clip/.gitattributes` no longer needs the `dist/**` LFS rule once the
dist files are gone. `bin/.downloads` should be ignored by Git so downloaded
release assets stay local.

## Error Handling

- Missing or invalid `downloads.json`: `dotfiles-apply` reports a manifest error
  and exits non-zero.
- Unsupported platform or architecture: `dotfiles-apply` skips downloads for
  unmatched entries unless a required wrapper later reports that no binary is
  available.
- HTTP failures: report the URL and underlying error.
- Checksum mismatch: delete the temporary file, report expected and actual
  checksums, and exit non-zero.
- Extraction failure: remove the temporary extraction directory and exit
  non-zero.

## Testing

Update `bin/tests/test_clip.py` so fake binaries live under the new
`bin/.downloads/clip/<version>/<target>` layout and expected error messages point
to the downloaded binary location.

Update `bin/tests/test_dotfiles_apply.py` with mocked download behavior for:

- `--dry-run` reports a planned download without writing files.
- Existing cached binary with matching manifest is accepted.
- Missing binary triggers download, checksum verification, extraction, and
  executable permission setup.
- SHA-256 mismatch reports an error and does not install the binary.

Run the relevant unittest files after implementation:

```sh
python -m unittest bin/tests/test_dotfiles_apply.py bin/tests/test_clip.py
```

Also run the repository's main unittest command because `dotfiles-apply` changes
can affect general install behavior:

```sh
python -m unittest bin/tests/test_tmux_load.py bin/tests/test_tmux_dump.py bin/tests/test_tbox.py bin/tests/test_tbox_integration.py
```

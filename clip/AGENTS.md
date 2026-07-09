# AGENTS

This directory is an independent Rust workspace for the cross-platform `clip`
clipboard CLI and history tools.

## Workspace Layout

- `crates/clip-core/`: shared clipboard model, MIME handling, target types, and
  error definitions. Keep this crate platform-neutral.
- `crates/clip-platform/`: platform detection and backend implementations for
  Wayland, X11, macOS helper integration, and current Windows/ADB stubs.
- `crates/clip-cli/`: user-facing `clip` command argument parsing, input
  loading, output writing, and command orchestration.
- `crates/clip-history/`: clipboard history capture, arguments, storage, and
  runtime flow.
- `helpers/macos/clip-macos-helper/`: Swift helper package for macOS clipboard
  and history menu integration.
- `scripts/`: project-specific test and development helpers.
- `testdata/`: fixtures for CLI/backend tests. Reuse these before adding new
  fixtures.

## Rust Guidelines

- Preserve crate boundaries: core data types belong in `clip-core`, environment
  probing and command execution belong in `clip-platform`, and CLI behavior
  belongs in `clip-cli`.
- Keep platform behavior injectable and testable through existing traits such
  as `EnvProbe` and `CommandRunner`; avoid shelling out directly from CLI code.
- Prefer explicit `Result` returns with `ClipError` over panics for operational
  failures.
- Keep target-specific behavior behind the existing backend modules instead of
  scattering `cfg` checks through shared code.
- Match the existing small-module style and avoid broad refactors when fixing a
  focused issue.

## Testing And Formatting

After Rust changes, run:

```bash
cargo test --manifest-path clip/Cargo.toml
```

Format Rust code with:

```bash
cargo fmt --manifest-path clip/Cargo.toml
```

When Docker is available, `bash clip/scripts/test-linux-docker.sh` runs the
Linux smoke test suite in a container.

For platform backend changes, also run the most relevant focused tests, for
example:

```bash
cargo test --manifest-path clip/Cargo.toml -p clip-platform
cargo test --manifest-path clip/Cargo.toml -p clip-cli
```

## Platform Notes

- Linux backends use external clipboard tools; keep command construction in
  `clip-platform` and covered by stubbed command-runner tests.
- macOS Rust code resolves and invokes the Swift helper. If changing helper
  protocol or paths, update both Rust integration and Swift sources together.
- Windows and ADB are currently stubbed targets. Do not imply full support
  without implementing and testing the backend behavior.

## Documentation

- Update `README.md` when user-facing commands, supported targets, fixtures, or
  smoke-test instructions change.
- Keep design notes in `docs/` for larger behavior changes or plans; do not bury
  product decisions only in code comments.

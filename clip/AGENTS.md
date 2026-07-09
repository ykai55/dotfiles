# AGENTS

This directory is an independent Rust workspace for the cross-platform `clip`
clipboard CLI and history tools.

After Rust changes, run `cargo test --manifest-path clip/Cargo.toml`. Format
with `cargo fmt --manifest-path clip/Cargo.toml` when editing Rust code.

When Docker is available, `bash clip/scripts/test-linux-docker.sh` runs the
Linux smoke test suite in a container.

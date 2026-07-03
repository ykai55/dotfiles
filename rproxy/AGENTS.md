# AGENTS

This directory is an independent Rust crate for `rproxy`, a reverse proxy CLI
with server and client modes.

## Build / Test / Run

Run commands from the repository root unless noted otherwise.

Build and test:

- `cargo test --manifest-path rproxy/Cargo.toml`
- `cargo run --manifest-path rproxy/Cargo.toml -- --help`

Format after Rust edits:

- `cargo fmt --manifest-path rproxy/Cargo.toml`

Local HTTP smoke test:

- Start server:
  `cargo run --manifest-path rproxy/Cargo.toml -- server --domain test --token secret --control-listen 127.0.0.1:7000 --http-listen 127.0.0.1:8080 --tcp-port-range 20000-20010`
- Start local service:
  `python3 -m http.server 9000`
- Start client:
  `cargo run --manifest-path rproxy/Cargo.toml -- client --server ws://127.0.0.1:7000 --token secret http --local 127.0.0.1:9000 --subdomain foo`
- Request:
  `curl -H 'Host: foo.test' http://127.0.0.1:8080/`

## Project Layout

- `src/main.rs`: CLI flags and conversion into runtime config.
- `src/client.rs`: client control loop, local target validation, data WebSocket
  handling, and local TCP piping.
- `src/server.rs`: control WebSocket endpoint, registration state, HTTP/TCP
  listener handling, and server-side stream piping.
- `src/protocol.rs`: JSON protocol messages for control and data connections.
- `src/alloc.rs`: TCP port and HTTP subdomain allocation.
- `src/routing.rs`: HTTP `Host` matching for subdomain routes.
- `tests/http_tunnel.rs`: end-to-end HTTP tunnel coverage.
- `tests/tcp_tunnel.rs`: end-to-end TCP tunnel coverage.

## Behavior Notes

- `--server` must include `ws://` or `wss://`; the client appends the internal
  `/_rproxy` path itself.
- Runtime logs use `tracing` and are written to stderr. Keep default output
  useful for manual CLI runs; use stable `[rproxy server]` and
  `[rproxy client]` prefixes for human-readable messages.
- `--local` must be a full `host:port` address, for example
  `127.0.0.1:9000`.
- Tunnel registrations are in-memory and only valid while the client control
  WebSocket stays connected.
- HTTP exposure routes by `Host`; HTTPS is supported by external TLS
  termination that forwards decrypted HTTP with the original `Host` header.
- TCP exposure binds requested or allocated ports on the server.

## Rust Style

- Keep using Rust 2021.
- Prefer direct, local code over new abstractions unless the abstraction names a
  real protocol or domain concept.
- Keep user-facing errors explicit. Invalid CLI inputs should fail before a
  tunnel is registered or traffic is accepted.
- Add or update tests before changing behavior. Keep integration tests local and
  self-contained.
- Do not commit `target/` or downloaded release artifacts.

## Release / Install Notes

- `bin/rproxy` is a thin wrapper around the downloaded binary under
  `bin/.downloads/rproxy/current/<target>/rproxy`.
- `downloads.json` contains the release assets consumed by `bin/dotfiles-apply`.
- `.github/workflows/rproxy-release.yml` publishes `rproxy-latest` assets.
- If asset names or supported targets change, update `downloads.json`,
  `bin/rproxy`, `bin/tests/test_rproxy.py`, and the release workflow together.

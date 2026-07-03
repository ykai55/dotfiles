# rproxy

`rproxy` is a Rust reverse proxy CLI for exposing local HTTP and TCP services
through a remote server.

The binary has two modes:

- `rproxy server`: runs on the remote machine associated with a base domain.
- `rproxy client`: runs near the local service and registers one temporary
  tunnel with the server.

## Design

The client connects to the server over WebSocket. Users pass only a WebSocket
service prefix with `--server`, such as `ws://127.0.0.1:7000` for local testing
or `wss://a.com` for production. The internal control path is always appended
by the client as `/_rproxy`.

Each client keeps one control WebSocket open. When the server receives an
external HTTP or TCP connection for that tunnel, it sends an `open` message on
the control connection. The client then opens a second data WebSocket for that
single connection and pipes raw bytes between the local service and the server.

The first version intentionally avoids custom stream multiplexing. One inbound
connection maps to one data WebSocket. This keeps connection lifecycle, back
pressure, and error handling easier to reason about.

## Features

- HTTP tunnels routed by `Host` header, for example `foo.a.com`.
- TCP tunnels exposed on a requested or automatically allocated remote port.
- Static token authentication for control and data WebSocket connections.
- Temporary in-memory tunnel registrations. When the client disconnects, its
  ports, subdomains, and active connections are released.
- HTTPS compatibility through external TLS termination. `rproxy` routes the
  decrypted HTTP request by `Host`; it does not manage certificates.
- Download/install integration through the repository `downloads.json` manifest
  and the `bin/rproxy` wrapper.

## Local HTTP Test

Start the server:

```bash
cargo run --manifest-path rproxy/Cargo.toml -- server \
  --domain test \
  --token secret \
  --control-listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:8080 \
  --tcp-port-range 20000-20010
```

Start a local HTTP service:

```bash
python3 -m http.server 9000
```

Register an HTTP tunnel:

```bash
cargo run --manifest-path rproxy/Cargo.toml -- client \
  --server ws://127.0.0.1:7000 \
  --token secret \
  http \
  --local 127.0.0.1:9000 \
  --subdomain foo
```

Request through the server HTTP listener:

```bash
curl -H 'Host: foo.test' http://127.0.0.1:8080/
```

## Local TCP Test

Start the server as above, then start a local TCP or HTTP service on port 9000.
Register a TCP tunnel:

```bash
cargo run --manifest-path rproxy/Cargo.toml -- client \
  --server ws://127.0.0.1:7000 \
  --token secret \
  tcp \
  --local 127.0.0.1:9000 \
  --remote-port 20000
```

Connect through the exposed TCP port:

```bash
curl http://127.0.0.1:20000/
```

## Production Shape

For production, run the server control listener behind a TLS terminator:

```text
rproxy client
  -> wss://a.com/_rproxy
  -> TLS terminator
  -> ws://127.0.0.1:7000/_rproxy
  -> rproxy server --control-listen 127.0.0.1:7000
```

For HTTP service exposure, route HTTP traffic for `*.a.com` to the server
`--http-listen` address. If HTTPS is needed for exposed services, terminate TLS
before forwarding the decrypted HTTP request to `rproxy server`, preserving the
original `Host` header.

## Development

Runtime logs are written to stderr with tracing levels and stable
`[rproxy server]` or `[rproxy client]` prefixes. The default log level is
`info`; per-connection traffic logs use `debug`, and recoverable failures use
`warn`. Set `RUST_LOG=debug` to show detailed connection activity.

Run tests:

```bash
cargo test --manifest-path rproxy/Cargo.toml
```

Format code:

```bash
cargo fmt --manifest-path rproxy/Cargo.toml
```

The release workflow builds archives for Linux x86_64 GNU and macOS arm64 and
publishes them under the `rproxy-latest` GitHub release tag.

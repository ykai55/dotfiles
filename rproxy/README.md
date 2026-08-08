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
by the client as `/_rproxy`. The server URL must not include another path, query,
fragment, or embedded credentials.

Each client keeps one control WebSocket open. When the server receives an
external HTTP or TCP connection for that tunnel, it sends an `open` message on
the control connection. The client then opens a second data WebSocket for that
single connection and pipes raw bytes between the local service and the server.

The first version intentionally avoids custom stream multiplexing. One inbound
connection maps to one data WebSocket. This keeps connection lifecycle, back
pressure, and error handling easier to reason about.

### Connection Lifecycle

The control WebSocket owns the tunnel and all of its data connections. If it
closes, the server releases the public route and cancels pending and active data
connections. The client also closes its data WebSockets and local TCP
connections before reconnecting the control WebSocket.

After an external connection arrives, the server waits up to three seconds for
the matching authenticated data WebSocket. A timeout or tunnel shutdown removes
the pending connection state rather than leaving it allocated.

### Data Connection Protocol

After the initial authenticated data `hello`, WebSocket binary frames carry TCP
bytes unchanged. TCP EOF is directional, so either peer sends the following text
frame after its TCP reader reaches EOF:

```json
{"type":"half_close"}
```

The receiver shuts down only its TCP write half and continues forwarding bytes
in the other direction. This supports protocols in which a requester sends EOF
and then waits for a response. A WebSocket close or control-connection shutdown
closes the whole data connection. Because `half_close` changes the data-plane
protocol, deploy matching server and client versions together.

## Features

- HTTP tunnels routed by `Host` header, for example `foo.a.com`.
- TCP tunnels exposed on a requested or automatically allocated remote port.
- Directional TCP half-close propagation through data WebSocket control frames.
- Static client token authentication for control and data WebSocket connections.
  Servers can use one legacy token or a config file with multiple client
  identities.
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

When the public HTTP entrypoint differs from the internal listener, configure the
URL advertised to HTTP tunnel clients:

```bash
rproxy server \
  --domain rp.ykai.cc \
  --token "$RPROXY_TOKEN" \
  --control-listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:8080 \
  --http-public-scheme https \
  --http-public-port 444
```

With a client subdomain of `foo`, the advertised URL is
`https://foo.rp.ykai.cc:444`. Routing still uses the HTTP `Host` header
`foo.rp.ykai.cc` after TLS termination.

## Client Identity Config

For one client identity, start the server with `--token` as shown in the local
examples. For multiple client identities, write a TOML config file and start the
server with `--config` instead. `--token` and `--config` are mutually exclusive.

```toml
[[clients]]
id = "client-one"
token = "secret-one"

[[clients]]
id = "client-two"
token = "secret-two"
```

```bash
rproxy server \
  --domain example.com \
  --config clients.toml \
  --control-listen 127.0.0.1:7000 \
  --http-listen 127.0.0.1:8080
```

The server reads the config file only at startup. Restart the server after token
changes. Client tokens are plaintext in the first config format, so protect the
file with normal filesystem permissions and do not log or publish real tokens.

## Input Boundaries

- Requested HTTP subdomains are normalized to lowercase and must be one ASCII
  DNS label of at most 63 characters. Letters, digits, and internal hyphens are
  accepted; empty labels, dots, spaces, underscores, and leading or trailing
  hyphens are rejected.
- TCP port ranges cannot include port `0`. Requested ports must be inside the
  configured range; port `65535` is supported.
- HTTP `Host` matching is case-insensitive. Missing, empty, duplicate, or
  malformed Host headers are rejected rather than used for routing.
- Initial HTTP request headers are limited to 64 KiB and must complete within
  five seconds. Oversized headers receive `431`, incomplete headers receive
  `400`, and timed-out headers receive `408`.
- `--server` accepts a `ws://` or `wss://` base URL with a host and optional
  port. Paths, queries, fragments, and URL credentials are rejected.

## Performance Notes

The current data path favors deployment simplicity over maximum throughput. Each
external HTTP or TCP connection creates one data WebSocket back from the client
to the server. The connection is then proxied as raw bytes over WebSocket binary
frames, with the text control frame described above for half-close signaling.

This has a few important costs compared with a local reverse proxy such as
nginx:

- Every new external connection waits for the server to notify the client and
  for the client to open a data WebSocket.
- Data is wrapped in WebSocket frames instead of flowing over a raw TCP data
  channel.
- HTTP routing reads the first request headers on a connection to choose the
  tunnel, then treats the rest of that connection as raw TCP. It does not parse
  every keep-alive request like a full HTTP reverse proxy.
- Server state currently uses shared in-memory maps guarded by a mutex. This is
  simple but may become a hot path under high concurrency.

Potential optimization directions:

1. Instrument the data path first. Track accept-to-data-attached latency, local
   connect latency, bytes transferred, and connection duration. This identifies
   whether time is spent in tunnel setup, local connection setup, or byte
   forwarding.
2. Add a raw TCP idle data pool. The client would pre-open N data TCP
   connections to the server. When an external request arrives, the server takes
   an idle connection and immediately starts piping bytes. The simplest version
   uses each data TCP connection for one external connection, then closes it and
   lets the client replenish the pool.
3. Consider sequential reuse of raw data TCP connections only with explicit
   framing. Reusing one data TCP connection for multiple external connections
   requires in-band boundaries such as begin/data/fin/reset messages; a separate
   control WebSocket cannot safely mark a raw byte stream idle because data and
   control ordering are not guaranteed across different channels.
4. Consider long-lived multiplexed data channels if higher throughput is needed.
   This would use a small number of persistent data connections and carry many
   logical streams over them. It removes per-connection setup costs but requires
   stream IDs, framing, flow control, half-close handling, and error isolation.
5. Revisit HTTP semantics if correctness across HTTP keep-alive requests with
   different `Host` headers becomes important. A conservative approach is to
   close HTTP connections after one routed request. A more complete approach is
   to implement request-level HTTP proxying and route each request separately.

The most practical next step is usually instrumentation, followed by a raw TCP
idle data pool. Multiplexing should wait until measurements show the simpler
pool model is not enough.

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

# rproxy

`rproxy` is a Rust reverse proxy CLI for exposing local HTTP and TCP services
through a remote server.

The binary has two modes:

- `rproxy server`: runs on the remote machine associated with a base domain.
- `rproxy client`: runs near the local service and registers one temporary
  tunnel with the server.

## Design

The client connects to the server over WebSocket. Users pass a domain such as
`a.com`, or a WebSocket service prefix such as `ws://127.0.0.1:7000`. A bare
domain defaults to `ws://`; HTTP redirects are followed and an `https://`
redirect is connected to as `wss://`. The internal control path is always
appended by the client as `/_rproxy`. The server URL must not include another
path, query, fragment, or embedded credentials.

Each registered tunnel keeps exactly two WebSockets open: one control connection
and one data connection. The data connection multiplexes up to 64 active logical
HTTP or TCP streams, so external connections do not perform additional
WebSocket handshakes.

### Connection Lifecycle

The control WebSocket owns the tunnel and its data connection. If either closes,
the server releases the public route and cancels all active logical streams. The
client also closes local TCP connections before reconnecting both WebSockets.
WebSocket handshakes, the first hello, data attachment, local TCP connects, and
logical stream readiness all have explicit deadlines.

### Data Connection Protocol

After the authenticated data `hello` attaches a `session_id`, binary frames use
`Open`, `Ready`, `Data`, `Credit`, `Fin`, and `Reset` opcodes keyed by a non-zero
stream ID. `Data` payloads are limited to 16 KiB. Each direction starts with an
eight-frame credit window and returns one credit only after writing a frame to
TCP, preventing a slow stream from blocking the WebSocket reader.

`Fin` shuts down only the receiver's TCP write half and leaves the opposite
direction active. `Reset` isolates protocol and overload failures to one logical
stream. Server and client versions must match; the mux protocol does not support
the older per-stream WebSocket protocol.

## Features

- HTTP tunnels routed by `Host` header, for example `foo.a.com`.
- TCP tunnels exposed on a requested or automatically allocated remote port.
- Directional TCP half-close propagation through per-stream `Fin` frames.
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

## Server Configuration

`--config` loads the complete server configuration and cannot be combined with
other server options. The file must have mode `0600` on Unix because configured
client tokens are plaintext.

```toml
[server]
domain = "example.com"
control_listen = "127.0.0.1:7000"
http_listen = "0.0.0.0:8080"
tcp_port_range = "20000-30000"
http_public_scheme = "https"

[authentication]
database = "/var/lib/rproxy/auth.db"

[management]
listen = "127.0.0.1:7001"
token_file = "/run/secrets/rproxy-management-token"
requests_per_minute = 120
body_limit_bytes = 16384

[[clients]]
id = "build-agent"
subdomains = ["preview-*", "docs"]

[[clients.tokens]]
name = "primary"
label = "production"
token = "configured-secret"
```

```bash
chmod 600 server.toml /run/secrets/rproxy-management-token
rproxy server --config server.toml
```

Configured identities and tokens remain in memory and are never written to
SQLite. The management API can read them but returns `409 managed_by_config` for
mutations. SQLite contains only identities and tokens created by the management
API. Startup fails if config and SQLite contain the same Identity ID or token
secret.

Keep the management listener private or put it behind an HTTPS reverse proxy
reachable only by the trusted operator.

Create an identity and token:

```bash
curl -X POST http://127.0.0.1:7001/v1/client-identities \
  -H "Authorization: Bearer $RPROXY_MANAGEMENT_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"id":"build-agent","subdomain_policy":{"rules":["preview-*","docs"]}}'

curl -X POST http://127.0.0.1:7001/v1/client-identities/build-agent/tokens \
  -H "Authorization: Bearer $RPROXY_MANAGEMENT_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"label":"production","expires_at":null}'
```

The token secret is returned only by the create request. List and get endpoints
return metadata, never the secret. Rotate a token by creating a replacement,
deploying it to the client, and deleting the old token. Deleting a token,
disabling or deleting an identity, or restricting its subdomain policy closes
affected active tunnels immediately.

Management endpoints are documented in
[`docs/management-api-design.md`](docs/management-api-design.md). The complete
configuration schema and ownership rules are documented in
[`docs/server-config-design.md`](docs/server-config-design.md).

## Input Boundaries

- Requested HTTP subdomains are normalized to lowercase and must be one ASCII
  DNS label of at most 63 characters. Letters, digits, and internal hyphens are
  accepted; empty labels, dots, spaces, underscores, and leading or trailing
  hyphens are rejected.
- Subdomain policy rules may be exact labels, `*`, or one wildcard inside a
  single label, for example `preview-*`, `*-dev`, or `api-*-dev`. Multi-level
  rules such as `*.dev` are not supported.
- TCP port ranges cannot include port `0`. Requested ports must be inside the
  configured range; port `65535` is supported.
- HTTP `Host` matching is case-insensitive. Missing, empty, duplicate, or
  malformed Host headers are rejected rather than used for routing.
- Initial HTTP request headers are limited to 64 KiB and must complete within
  five seconds. Oversized headers receive `431`, incomplete headers receive
  `400`, and timed-out headers receive `408`.
- `--server` accepts a bare domain or a `ws://` or `wss://` base URL with a host
  and optional port. Bare domains default to `ws://`; redirects are followed,
  with `https://` locations mapped to `wss://`. Paths, queries, fragments, and
  URL credentials are rejected.

## Performance Notes

The current data path multiplexes every external HTTP or TCP connection over one
long-lived data WebSocket per tunnel. Together with the control WebSocket, each
registered tunnel uses exactly two connections to the control endpoint.

This has a few important costs compared with a local reverse proxy such as
nginx:

- Data is wrapped in framed WebSocket messages instead of flowing over a raw TCP
  data channel.
- A tunnel supports at most 64 active logical streams; additional TCP accepts
  remain backpressured until a stream permit is released.
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
2. Tune bounded queue sizes and stream windows from measurements rather than
   increasing them speculatively.
3. Revisit HTTP semantics if correctness across HTTP keep-alive requests with
   different `Host` headers becomes important. A conservative approach is to
   close HTTP connections after one routed request. A more complete approach is
   to implement request-level HTTP proxying and route each request separately.

The most practical next step is instrumentation around queue saturation, resets,
and stream setup latency.

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

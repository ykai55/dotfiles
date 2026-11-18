# dsh

Personal DeepSeek Harness extras, wired up by `bin/dsh-web`.

## dsh-web

`bin/dsh-web` runs `dsh web` with one extra patch layer, `overlay.yml`, which
loads `no-auth.cjs`. The plugin removes the per-process browser-session token
gate: the page is served directly without the `?token=` exchange, which keeps an
installed PWA working. It also reports the plain origin, so neither the startup
line nor a browser handoff carries the launch token.

The wrapper passes `--no-open`, so starting it never launches a browser: open
the installed PWA, or the printed origin, when you want the UI.

The Host/Origin fence is untouched, so a request with a foreign `Host` still
gets 403. Only the 401 "no session cookie" refusal is dropped.

```sh
dsh-web                 # 127.0.0.1:3080, no browser opened
dsh-web --port 8080
dsh-web --help          # dsh web's own flags, plus --patch from the launcher
```

The stock `dsh web` command and the desktop app are unaffected: they never load
this overlay. `--patch` overlays are applied after the profile layer, and the
plugin path resolves relative to `overlay.yml`, so the wrapper works from any
working directory.

## Security

Without the token, anything that can reach the port and pass the Host/Origin
check acts as the owner Peer: it can run bash, read and write workspace files,
and drive sessions. That is acceptable only on loopback.

Do not add `--host` with a non-loopback address. A remote deployment needs its
own authentication in front (reverse proxy with mTLS or basic auth, a private
network, a tunnel with access control).

## Maintenance

`no-auth.cjs` patches `client-connection` internals (`authorizeIndex`, `admit`,
`authenticatedUrl`, `browserPeer`) because DSH exposes no configuration switch
for this. DSH is in preview, so re-check after an upgrade: the plugin failing to
load aborts startup loudly, while renamed methods would simply bring the token
gate back.

Verified against `dsh 0.2.2-rc.2`:

| Behavior | Result |
| --- | --- |
| Startup line | `dsh web: http://127.0.0.1:3080/`, no token |
| Browser handoff | never runs (`--no-open` in the wrapper) |
| `GET /` without a cookie | 200, serves the app |
| `POST /api/...` without a cookie | reaches the API (404 for an unknown route) |
| Any request with a foreign `Host` | 403 |
| Stock `dsh web` | 401 as before |

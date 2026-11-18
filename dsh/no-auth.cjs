// Drop DeepSeek Harness Web's browser-session token gate for this process.
//
// `dsh web` authenticates every page and API call with a per-process launch
// token that only the printed `?token=` URL carries. This plugin replaces the
// two entry points on the `connection` service so a browser is admitted as the
// owner's shared Peer without that exchange.
//
// `authenticatedUrl` is what startup prints and hands to the browser, so the
// announcement carries the plain origin instead of the launch token; the token
// itself stays valid for `/connect` clients but is never advertised.
//
// The Host/Origin fence is left in place on purpose: a request whose Host is not
// loopback, the listener bind address, or a --trusted-host entry still gets 403.
// Only the 401 "no session cookie" refusal is dropped.
//
// Loaded by `bin/dsh-web` through `dsh/overlay.yml`; the stock `dsh web`
// profile keeps its authentication. Relies on client-connection internals
// (admit / authorizeIndex / authenticatedUrl / browserPeer), so a DSH upgrade
// may require a review.

exports.name = 'dsh-web-no-auth'

exports.apply = (ctx) => {
  ctx.inject(['connection'], (connectionCtx) => {
    const connection = connectionCtx.connection
    connection.authorizeIndex = () => true
    connection.authenticatedUrl = (url) => url

    const admit = connection.admit.bind(connection)
    connection.admit = (request) => {
      const outcome = admit(request)
      return 'rejection' in outcome && outcome.rejection === 401
        ? { peer: connection.browserPeer }
        : outcome
    }
  })
}

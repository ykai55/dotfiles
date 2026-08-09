# rproxy

rproxy exposes local services through a remote server using temporary tunnels.
This glossary defines the stable domain language used by the CLI, protocol, and
operator-facing configuration.

## Language

**Client Identity**:
A long-lived identity that is allowed to register tunnels with an rproxy server.
A client identity may create many control connections over time.
_Avoid_: client connection, tunnel, user

**Client Token**:
A revocable bearer credential assigned to one client identity and presented
when registering a tunnel or opening data connections for that tunnel. A client
identity may temporarily hold multiple client tokens during rotation.
_Avoid_: global token, password

**Subdomain Policy**:
The set of single-label subdomain rules a client identity is allowed to request
under the server's domain. A restricted policy requires an explicit subdomain.
_Avoid_: domain, route

**Management Credential**:
A server-level bearer credential used by a trusted operator to manage client
identities, client tokens, and subdomain policies. It is never a client token.
_Avoid_: admin client token, root client

**Control Connection**:
The WebSocket connection a client identity uses to authenticate and register one
tunnel. Its lifetime owns the tunnel and its data connection.
_Avoid_: client, session

**Data Connection**:
The single long-lived authenticated WebSocket attached to one tunnel session.
Binary frames multiplex all logical streams for the tunnel by stream ID.
_Avoid_: control connection, tunnel

**Logical Stream**:
One external HTTP or TCP connection multiplexed over a tunnel's data connection.
Each logical stream has independent flow control and directional shutdown state.
_Avoid_: data connection, WebSocket

**Half-Close**:
Directional TCP EOF propagated over a data connection with a stream `Fin` frame.
It closes the receiver's TCP write half without stopping traffic in the other
direction.
_Avoid_: disconnect, close

**Tunnel**:
A temporary public route from the rproxy server to one local service behind a
client identity.
_Avoid_: client, token

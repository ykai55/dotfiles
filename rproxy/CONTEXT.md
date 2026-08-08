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
A secret assigned to one client identity and presented when registering a tunnel
or opening data connections for that tunnel.
_Avoid_: global token, password

**Control Connection**:
The WebSocket connection a client identity uses to authenticate and register one
tunnel, then receive requests to open data connections.
_Avoid_: client, session

**Data Connection**:
The authenticated WebSocket created for one external TCP connection. Binary
frames carry TCP bytes, and its lifetime is owned by the control connection.
_Avoid_: control connection, tunnel

**Half-Close**:
Directional TCP EOF propagated over a data connection with a `half_close`
control frame. It closes the receiver's TCP write half without stopping traffic
in the other direction.
_Avoid_: disconnect, close

**Tunnel**:
A temporary public route from the rproxy server to one local service behind a
client identity.
_Avoid_: client, token

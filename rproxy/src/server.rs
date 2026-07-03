use crate::alloc::{AllocError, PortAllocator, SubdomainAllocator};
use crate::protocol::{ClientHello, ServerErrorCode, ServerMessage, ServiceRequest};
use crate::routing::subdomain_for_host;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RegisteredTunnel {
    pub public: String,
    pub subdomain: Option<String>,
    pub remote_port: Option<u16>,
}

#[derive(Debug, Clone)]
pub struct TunnelHandle {
    pub client_id: String,
    pub local: String,
    pub control_tx: mpsc::UnboundedSender<ServerMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ServerStateError {
    #[error("subdomain {0} is unavailable")]
    SubdomainUnavailable(String),
    #[error("port {0} is unavailable")]
    PortUnavailable(u16),
    #[error("port {0} is not allowed")]
    PortNotAllowed(u16),
    #[error("port range exhausted")]
    PortRangeExhausted,
    #[error("invalid port range")]
    InvalidPortRange,
}

impl From<AllocError> for ServerStateError {
    fn from(error: AllocError) -> Self {
        match error {
            AllocError::InvalidPortRange => Self::InvalidPortRange,
            AllocError::PortNotAllowed(port) => Self::PortNotAllowed(port),
            AllocError::PortUnavailable(port) => Self::PortUnavailable(port),
            AllocError::PortRangeExhausted => Self::PortRangeExhausted,
            AllocError::SubdomainUnavailable(subdomain) => Self::SubdomainUnavailable(subdomain),
        }
    }
}

fn server_log_line(message: &str) -> String {
    format!("[rproxy server] {message}")
}

fn log_server(message: &str) {
    tracing::info!("{}", server_log_line(message));
}

#[derive(Debug, Clone)]
pub struct ServerState {
    inner: Arc<Mutex<InnerState>>,
}

#[derive(Debug)]
struct InnerState {
    domain: String,
    ports: PortAllocator,
    subdomains: SubdomainAllocator,
    http_tunnels: HashMap<String, TunnelHandle>,
    tcp_tunnels: HashMap<u16, TunnelHandle>,
    client_resources: HashMap<String, Vec<ClientResource>>,
    pending_data: HashMap<String, oneshot::Sender<WebSocket>>,
}

#[derive(Debug)]
enum ClientResource {
    HttpSubdomain(String),
    TcpPort(u16),
}

impl ServerState {
    pub fn new(domain: String, _token: String, port_allocator: PortAllocator) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerState {
                domain,
                ports: port_allocator,
                subdomains: SubdomainAllocator::new(),
                http_tunnels: HashMap::new(),
                tcp_tunnels: HashMap::new(),
                client_resources: HashMap::new(),
                pending_data: HashMap::new(),
            })),
        }
    }

    pub async fn register_control(
        &self,
        client_id: String,
        service: ServiceRequest,
        control_tx: mpsc::UnboundedSender<ServerMessage>,
    ) -> Result<RegisteredTunnel, ServerStateError> {
        let mut inner = self.inner.lock().await;
        match service {
            ServiceRequest::Http { local, subdomain } => {
                let subdomain = inner.subdomains.allocate(subdomain.as_deref())?;
                let handle = TunnelHandle {
                    client_id: client_id.clone(),
                    local,
                    control_tx,
                };
                inner.http_tunnels.insert(subdomain.clone(), handle);
                inner
                    .client_resources
                    .entry(client_id)
                    .or_default()
                    .push(ClientResource::HttpSubdomain(subdomain.clone()));
                Ok(RegisteredTunnel {
                    public: format!("http://{}.{}", subdomain, inner.domain),
                    subdomain: Some(subdomain),
                    remote_port: None,
                })
            }
            ServiceRequest::Tcp { local, remote_port } => {
                let port = inner.ports.allocate(remote_port)?;
                let handle = TunnelHandle {
                    client_id: client_id.clone(),
                    local,
                    control_tx,
                };
                inner.tcp_tunnels.insert(port, handle);
                inner
                    .client_resources
                    .entry(client_id)
                    .or_default()
                    .push(ClientResource::TcpPort(port));
                Ok(RegisteredTunnel {
                    public: format!("{}:{}", inner.domain, port),
                    subdomain: None,
                    remote_port: Some(port),
                })
            }
        }
    }

    pub async fn release_client(&self, client_id: &str) {
        let mut inner = self.inner.lock().await;
        let Some(resources) = inner.client_resources.remove(client_id) else {
            return;
        };

        for resource in resources {
            match resource {
                ClientResource::HttpSubdomain(subdomain) => {
                    inner.subdomains.release(&subdomain);
                    inner.http_tunnels.remove(&subdomain);
                }
                ClientResource::TcpPort(port) => {
                    inner.ports.release(port);
                    inner.tcp_tunnels.remove(&port);
                }
            }
        }
    }

    pub async fn http_tunnel_for_host(&self, host: &str) -> Option<TunnelHandle> {
        let inner = self.inner.lock().await;
        let subdomain = subdomain_for_host(host, &inner.domain)?;
        inner.http_tunnels.get(&subdomain).cloned()
    }

    pub async fn tcp_tunnel_for_port(&self, port: u16) -> Option<TunnelHandle> {
        let inner = self.inner.lock().await;
        inner.tcp_tunnels.get(&port).cloned()
    }

    async fn open_data_connection(
        &self,
        tunnel: &TunnelHandle,
    ) -> anyhow::Result<oneshot::Receiver<WebSocket>> {
        let connection_id = Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();
        self.inner
            .lock()
            .await
            .pending_data
            .insert(connection_id.clone(), tx);

        if tunnel
            .control_tx
            .send(ServerMessage::Open { connection_id })
            .is_err()
        {
            anyhow::bail!("control connection closed");
        }

        Ok(rx)
    }

    async fn attach_data_connection(&self, connection_id: &str, socket: WebSocket) -> bool {
        let sender = self.inner.lock().await.pending_data.remove(connection_id);
        if let Some(sender) = sender {
            sender.send(socket).is_ok()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc::PortAllocator;
    use crate::protocol::ServiceRequest;
    use tokio::sync::mpsc;

    fn state() -> ServerState {
        ServerState::new(
            "a.com".into(),
            "secret".into(),
            PortAllocator::new(20000, 20002).unwrap(),
        )
    }

    #[tokio::test]
    async fn registers_http_with_requested_subdomain() {
        let state = state();
        let (tx, _rx) = mpsc::unbounded_channel();

        let registered = state
            .register_control(
                "client-1".into(),
                ServiceRequest::Http {
                    local: "127.0.0.1:3000".into(),
                    subdomain: Some("foo".into()),
                },
                tx,
            )
            .await
            .unwrap();

        assert_eq!(registered.public, "http://foo.a.com");
        assert_eq!(registered.subdomain, Some("foo".into()));
        assert!(state.http_tunnel_for_host("foo.a.com").await.is_some());
    }

    #[tokio::test]
    async fn registers_tcp_with_requested_port() {
        let state = state();
        let (tx, _rx) = mpsc::unbounded_channel();

        let registered = state
            .register_control(
                "client-1".into(),
                ServiceRequest::Tcp {
                    local: "127.0.0.1:5432".into(),
                    remote_port: Some(20001),
                },
                tx,
            )
            .await
            .unwrap();

        assert_eq!(registered.public, "a.com:20001");
        assert_eq!(registered.remote_port, Some(20001));
        assert!(state.tcp_tunnel_for_port(20001).await.is_some());
    }

    #[tokio::test]
    async fn rejects_duplicate_subdomain() {
        let state = state();
        let (first_tx, _first_rx) = mpsc::unbounded_channel();
        let (second_tx, _second_rx) = mpsc::unbounded_channel();

        state
            .register_control(
                "client-1".into(),
                ServiceRequest::Http {
                    local: "127.0.0.1:3000".into(),
                    subdomain: Some("foo".into()),
                },
                first_tx,
            )
            .await
            .unwrap();

        let error = state
            .register_control(
                "client-2".into(),
                ServiceRequest::Http {
                    local: "127.0.0.1:4000".into(),
                    subdomain: Some("foo".into()),
                },
                second_tx,
            )
            .await
            .unwrap_err();

        assert_eq!(error, ServerStateError::SubdomainUnavailable("foo".into()));
    }

    #[tokio::test]
    async fn releases_client_resources() {
        let state = state();
        let (tx, _rx) = mpsc::unbounded_channel();

        state
            .register_control(
                "client-1".into(),
                ServiceRequest::Tcp {
                    local: "127.0.0.1:5432".into(),
                    remote_port: Some(20001),
                },
                tx,
            )
            .await
            .unwrap();
        state.release_client("client-1").await;

        assert!(state.tcp_tunnel_for_port(20001).await.is_none());

        let (tx, _rx) = mpsc::unbounded_channel();
        let registered = state
            .register_control(
                "client-2".into(),
                ServiceRequest::Tcp {
                    local: "127.0.0.1:5432".into(),
                    remote_port: Some(20001),
                },
                tx,
            )
            .await
            .unwrap();
        assert_eq!(registered.remote_port, Some(20001));
    }

    #[test]
    fn formats_server_log_line() {
        assert_eq!(
            server_log_line("control listening on 127.0.0.1:7000"),
            "[rproxy server] control listening on 127.0.0.1:7000"
        );
    }
}

#[derive(Debug)]
pub struct ServerConfig {
    pub domain: String,
    pub token: String,
    pub control_listen: SocketAddr,
    pub http_listen: SocketAddr,
    pub tcp_port_range: String,
}

pub async fn run(config: ServerConfig) -> anyhow::Result<()> {
    let ports = PortAllocator::parse_range(&config.tcp_port_range)?;
    let domain = config.domain.clone();
    let state = ServerState::new(config.domain, config.token.clone(), ports);
    let app_state = AppState {
        state: state.clone(),
        token: config.token,
    };

    let control_listener = TcpListener::bind(config.control_listen).await?;
    log_server(&format!(
        "control listening on {} for domain {domain}",
        config.control_listen
    ));
    let http_listener = TcpListener::bind(config.http_listen).await?;
    log_server(&format!("http listening on {}", config.http_listen));
    let app = Router::new()
        .route("/_rproxy", get(control_ws))
        .with_state(app_state);
    let control_task = tokio::spawn(async move { axum::serve(control_listener, app).await });

    let http_task = tokio::spawn(run_http_listener(state, http_listener));

    tokio::select! {
        result = control_task => result??,
        result = http_task => result??,
    }

    Ok(())
}

#[derive(Clone)]
struct AppState {
    state: ServerState,
    token: String,
}

async fn control_ws(State(app): State<AppState>, ws: WebSocketUpgrade) -> axum::response::Response {
    ws.on_upgrade(move |socket| handle_control_socket(app, socket))
}

async fn handle_control_socket(app: AppState, mut socket: WebSocket) {
    let Some(Ok(Message::Text(text))) = socket.recv().await else {
        log_server("control websocket closed before hello");
        return;
    };
    let Ok(hello) = serde_json::from_str::<ClientHello>(&text) else {
        log_server("invalid hello message received");
        let _ = send_error(socket, ServerErrorCode::InvalidRequest, "invalid hello").await;
        return;
    };

    match hello {
        ClientHello::Control { token, service } => {
            if token != app.token {
                log_server("control authentication failed");
                let _ =
                    send_error(socket, ServerErrorCode::AuthFailed, "authentication failed").await;
                return;
            }
            log_server("control authentication succeeded");
            handle_registered_control(app.state, socket, service).await;
        }
        ClientHello::Data {
            token,
            connection_id,
        } => {
            if token != app.token {
                log_server(&format!("data authentication failed: {connection_id}"));
                let _ =
                    send_error(socket, ServerErrorCode::AuthFailed, "authentication failed").await;
                return;
            }
            if app
                .state
                .attach_data_connection(&connection_id, socket)
                .await
            {
                log_server(&format!("data websocket attached: {connection_id}"));
            } else {
                log_server(&format!(
                    "data websocket has no pending connection: {connection_id}"
                ));
            }
        }
    }
}

async fn handle_registered_control(state: ServerState, socket: WebSocket, service: ServiceRequest) {
    let client_id = Uuid::new_v4().to_string();
    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let is_tcp = matches!(service, ServiceRequest::Tcp { .. });
    let registered = match state
        .register_control(client_id.clone(), service, control_tx)
        .await
    {
        Ok(registered) => registered,
        Err(error) => {
            let code = match error {
                ServerStateError::SubdomainUnavailable(_) => ServerErrorCode::SubdomainUnavailable,
                ServerStateError::PortUnavailable(_) => ServerErrorCode::PortUnavailable,
                ServerStateError::PortNotAllowed(_) => ServerErrorCode::PortNotAllowed,
                ServerStateError::PortRangeExhausted => ServerErrorCode::PortRangeExhausted,
                ServerStateError::InvalidPortRange => ServerErrorCode::InvalidRequest,
            };
            log_server(&format!("registration failed: {error}"));
            let _ = send_error(socket, code, &error.to_string()).await;
            return;
        }
    };

    let mut tcp_listener_task = None;
    if is_tcp {
        if let Some(port) = registered.remote_port {
            let listener = match TcpListener::bind(("0.0.0.0", port)).await {
                Ok(listener) => listener,
                Err(error) => {
                    state.release_client(&client_id).await;
                    log_server(&format!("failed to listen on TCP port {port}: {error}"));
                    let _ = send_error(
                        socket,
                        ServerErrorCode::PortUnavailable,
                        &format!("failed to listen on TCP port {port}: {error}"),
                    )
                    .await;
                    return;
                }
            };
            log_server(&format!("tcp listening on 0.0.0.0:{port}"));
            let state_for_tcp = state.clone();
            tcp_listener_task = Some(tokio::spawn(async move {
                let _ = run_tcp_port_listener(state_for_tcp, port, listener).await;
            }));
        }
    }

    let (mut sender, mut receiver) = socket.split();
    let message = ServerMessage::Registered {
        public: registered.public,
        subdomain: registered.subdomain,
        remote_port: registered.remote_port,
    };
    log_server(&format!(
        "registered tunnel for client {client_id}: {}",
        message_public(&message)
    ));
    if sender
        .send(Message::Text(serde_json::to_string(&message).unwrap()))
        .await
        .is_err()
    {
        state.release_client(&client_id).await;
        if let Some(task) = tcp_listener_task {
            task.abort();
        }
        return;
    }

    loop {
        tokio::select! {
            Some(message) = control_rx.recv() => {
                if sender.send(Message::Text(serde_json::to_string(&message).unwrap())).await.is_err() {
                    break;
                }
            }
            incoming = receiver.next() => {
                if incoming.is_none() {
                    break;
                }
            }
        }
    }

    state.release_client(&client_id).await;
    log_server(&format!("released client resources: {client_id}"));
    if let Some(task) = tcp_listener_task {
        task.abort();
    }
}

fn message_public(message: &ServerMessage) -> &str {
    match message {
        ServerMessage::Registered { public, .. } => public,
        _ => "unknown",
    }
}

async fn send_error(
    mut socket: WebSocket,
    code: ServerErrorCode,
    message: &str,
) -> anyhow::Result<()> {
    let message = ServerMessage::Error {
        code,
        message: message.to_string(),
    };
    socket
        .send(Message::Text(serde_json::to_string(&message)?))
        .await?;
    Ok(())
}

async fn run_http_listener(state: ServerState, listener: TcpListener) -> anyhow::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let _ = handle_http_stream(state, stream).await;
        });
    }
}

async fn run_tcp_port_listener(
    state: ServerState,
    port: u16,
    listener: TcpListener,
) -> anyhow::Result<()> {
    loop {
        let (stream, _) = listener.accept().await?;
        let state = state.clone();
        tokio::spawn(async move {
            let _ = handle_tcp_stream(state, port, stream).await;
        });
    }
}

async fn handle_tcp_stream(state: ServerState, port: u16, stream: TcpStream) -> anyhow::Result<()> {
    let Some(tunnel) = state.tcp_tunnel_for_port(port).await else {
        log_server(&format!(
            "tcp connection ignored without tunnel: port {port}"
        ));
        return Ok(());
    };
    log_server(&format!(
        "tcp connection accepted: port {port} -> {}",
        tunnel.local
    ));
    let rx = state.open_data_connection(&tunnel).await?;
    let socket = timeout(Duration::from_secs(3), rx).await??;
    proxy_tcp_with_websocket(stream, socket, None).await
}

async fn handle_http_stream(state: ServerState, mut stream: TcpStream) -> anyhow::Result<()> {
    let initial = read_http_headers(&mut stream).await?;
    let Some(host) = http_host(&initial) else {
        log_server("http request rejected without Host header");
        stream
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    let Some(tunnel) = state.http_tunnel_for_host(&host).await else {
        log_server(&format!("http request has no tunnel: host {host}"));
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };

    log_server(&format!(
        "http request accepted: host {host} -> {}",
        tunnel.local
    ));
    let rx = state.open_data_connection(&tunnel).await?;
    let socket = timeout(Duration::from_secs(3), rx).await??;
    proxy_tcp_with_websocket(stream, socket, Some(initial)).await
}

async fn read_http_headers(stream: &mut TcpStream) -> anyhow::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut chunk = [0; 1024];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..n]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() > 64 * 1024 {
            break;
        }
    }
    Ok(buffer)
}

fn http_host(initial: &[u8]) -> Option<String> {
    let request = std::str::from_utf8(initial).ok()?;
    request.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("host")
            .then(|| value.trim().to_string())
    })
}

async fn proxy_tcp_with_websocket(
    stream: TcpStream,
    socket: WebSocket,
    initial_to_websocket: Option<Vec<u8>>,
) -> anyhow::Result<()> {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (mut tcp_reader, mut tcp_writer) = stream.into_split();

    if let Some(initial) = initial_to_websocket {
        ws_sender.send(Message::Binary(initial)).await?;
    }

    let tcp_to_ws = tokio::spawn(async move {
        let mut buffer = [0; 8192];
        loop {
            let n = tcp_reader.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            ws_sender
                .send(Message::Binary(buffer[..n].to_vec()))
                .await?;
        }
        anyhow::Ok(())
    });

    let ws_to_tcp = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message? {
                Message::Binary(data) => tcp_writer.write_all(&data).await?,
                Message::Close(_) => break,
                _ => {}
            }
        }
        anyhow::Ok(())
    });

    tokio::select! {
        result = tcp_to_ws => result??,
        result = ws_to_tcp => result??,
    }

    Ok(())
}

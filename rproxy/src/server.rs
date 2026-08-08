use crate::alloc::{AllocError, PortAllocator, SubdomainAllocator};
use crate::protocol::{
    ClientHello, DataFrame, ServerErrorCode, ServerMessage, ServiceRequest, INITIAL_CREDIT,
    MAX_DATA_SIZE,
};
use crate::routing::subdomain_for_host;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::stream::SplitStream;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot, Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const MAX_STREAMS: usize = 64;
const OPEN_QUEUE_SIZE: usize = MAX_STREAMS;
const STREAM_QUEUE_SIZE: usize = INITIAL_CREDIT as usize * 2 + 4;
const WRITER_QUEUE_SIZE: usize = 128;
const MAX_PENDING_HELLOS: usize = 1024;
const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const DATA_ATTACH_TIMEOUT: Duration = Duration::from_secs(5);
const READY_TIMEOUT: Duration = Duration::from_secs(6);

#[derive(Debug, Clone)]
pub struct RegisteredTunnel {
    pub session_id: String,
    pub public: String,
    pub subdomain: Option<String>,
    pub remote_port: Option<u16>,
}

#[derive(Debug)]
struct OpenCommand {
    stream: TcpStream,
    initial: Option<Vec<u8>>,
    _permit: OwnedSemaphorePermit,
}

#[derive(Debug, Clone)]
pub struct TunnelHandle {
    pub local: String,
    open_tx: mpsc::Sender<OpenCommand>,
    permits: Arc<Semaphore>,
    cancellation: CancellationToken,
}

#[derive(Debug)]
struct PendingDataConnection {
    client_id: String,
    client_identity_id: String,
    sender: oneshot::Sender<WebSocket>,
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
    #[error("invalid subdomain {0:?}")]
    InvalidSubdomain(String),
}

impl From<AllocError> for ServerStateError {
    fn from(error: AllocError) -> Self {
        match error {
            AllocError::InvalidPortRange => Self::InvalidPortRange,
            AllocError::PortNotAllowed(port) => Self::PortNotAllowed(port),
            AllocError::PortUnavailable(port) => Self::PortUnavailable(port),
            AllocError::PortRangeExhausted => Self::PortRangeExhausted,
            AllocError::SubdomainUnavailable(subdomain) => Self::SubdomainUnavailable(subdomain),
            AllocError::InvalidSubdomain(subdomain) => Self::InvalidSubdomain(subdomain),
        }
    }
}

fn server_log_line(message: &str) -> String {
    format!("[rproxy server] {message}")
}

fn log_server_info(message: &str) {
    tracing::info!("{}", server_log_line(message));
}

fn log_server_debug(message: &str) {
    tracing::debug!("{}", server_log_line(message));
}

fn log_server_warn(message: &str) {
    tracing::warn!("{}", server_log_line(message));
}

fn http_public_url(scheme: &str, port: Option<u16>, subdomain: &str, domain: &str) -> String {
    match port {
        Some(port) => format!("{scheme}://{subdomain}.{domain}:{port}"),
        None => format!("{scheme}://{subdomain}.{domain}"),
    }
}

fn validate_http_public_scheme(scheme: &str) -> anyhow::Result<()> {
    match scheme {
        "http" | "https" => Ok(()),
        _ => anyhow::bail!("--http-public-scheme must be http or https"),
    }
}

#[derive(Debug, Clone)]
pub struct AuthConfig {
    clients_by_token: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct AuthConfigFile {
    clients: Vec<ClientIdentityConfig>,
}

#[derive(Debug, Deserialize)]
struct ClientIdentityConfig {
    id: String,
    token: String,
}

impl AuthConfig {
    pub fn legacy_token(token: String) -> anyhow::Result<Self> {
        Self::from_clients(vec![ClientIdentityConfig {
            id: "legacy".into(),
            token,
        }])
    }

    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        Self::from_toml(&fs::read_to_string(path)?)
    }

    pub fn from_toml(text: &str) -> anyhow::Result<Self> {
        Self::from_clients(toml::from_str::<AuthConfigFile>(text)?.clients)
    }

    pub fn client_id_for_token(&self, token: &str) -> Option<&str> {
        self.clients_by_token.get(token).map(String::as_str)
    }

    fn from_clients(clients: Vec<ClientIdentityConfig>) -> anyhow::Result<Self> {
        if clients.is_empty() {
            anyhow::bail!("auth config must contain at least one client");
        }
        let mut clients_by_token = HashMap::new();
        let mut client_ids = HashSet::new();
        for client in clients {
            if client.id.trim().is_empty() || client.token.is_empty() {
                anyhow::bail!("client id and token must not be empty");
            }
            if !client_ids.insert(client.id.clone()) {
                anyhow::bail!("client ids must be unique");
            }
            if clients_by_token.insert(client.token, client.id).is_some() {
                anyhow::bail!("client tokens must be unique");
            }
        }
        Ok(Self { clients_by_token })
    }
}

#[derive(Debug, Clone)]
pub struct ServerState {
    inner: Arc<Mutex<InnerState>>,
}

#[derive(Debug)]
struct InnerState {
    domain: String,
    http_public_scheme: String,
    http_public_port: Option<u16>,
    ports: PortAllocator,
    subdomains: SubdomainAllocator,
    http_tunnels: HashMap<String, TunnelHandle>,
    tcp_tunnels: HashMap<u16, TunnelHandle>,
    client_resources: HashMap<String, Vec<ClientResource>>,
    pending_data: HashMap<String, PendingDataConnection>,
}

#[derive(Debug)]
enum ClientResource {
    HttpSubdomain(String),
    TcpPort(u16),
}

impl ServerState {
    pub fn new(
        domain: String,
        ports: PortAllocator,
        http_public_scheme: String,
        http_public_port: Option<u16>,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(InnerState {
                domain: domain.trim_end_matches('.').to_ascii_lowercase(),
                http_public_scheme,
                http_public_port,
                ports,
                subdomains: SubdomainAllocator::new(),
                http_tunnels: HashMap::new(),
                tcp_tunnels: HashMap::new(),
                client_resources: HashMap::new(),
                pending_data: HashMap::new(),
            })),
        }
    }

    async fn register_control(
        &self,
        client_id: String,
        client_identity_id: String,
        service: ServiceRequest,
        open_tx: mpsc::Sender<OpenCommand>,
        cancellation: CancellationToken,
    ) -> Result<(RegisteredTunnel, oneshot::Receiver<WebSocket>), ServerStateError> {
        let session_id = Uuid::new_v4().to_string();
        let (data_tx, data_rx) = oneshot::channel();
        let mut inner = self.inner.lock().await;
        let common = |local: String| TunnelHandle {
            local,
            open_tx: open_tx.clone(),
            permits: Arc::new(Semaphore::new(MAX_STREAMS)),
            cancellation: cancellation.clone(),
        };
        let registered = match service {
            ServiceRequest::Http { local, subdomain } => {
                let subdomain = inner.subdomains.allocate(subdomain.as_deref())?;
                inner.http_tunnels.insert(subdomain.clone(), common(local));
                inner
                    .client_resources
                    .entry(client_id.clone())
                    .or_default()
                    .push(ClientResource::HttpSubdomain(subdomain.clone()));
                RegisteredTunnel {
                    session_id: session_id.clone(),
                    public: http_public_url(
                        &inner.http_public_scheme,
                        inner.http_public_port,
                        &subdomain,
                        &inner.domain,
                    ),
                    subdomain: Some(subdomain),
                    remote_port: None,
                }
            }
            ServiceRequest::Tcp { local, remote_port } => {
                let port = inner.ports.allocate(remote_port)?;
                inner.tcp_tunnels.insert(port, common(local));
                inner
                    .client_resources
                    .entry(client_id.clone())
                    .or_default()
                    .push(ClientResource::TcpPort(port));
                RegisteredTunnel {
                    session_id: session_id.clone(),
                    public: format!("{}:{port}", inner.domain),
                    subdomain: None,
                    remote_port: Some(port),
                }
            }
        };
        inner.pending_data.insert(
            session_id,
            PendingDataConnection {
                client_id,
                client_identity_id,
                sender: data_tx,
            },
        );
        Ok((registered, data_rx))
    }

    pub async fn release_client(&self, client_id: &str) {
        let mut inner = self.inner.lock().await;
        if let Some(resources) = inner.client_resources.remove(client_id) {
            for resource in resources {
                match resource {
                    ClientResource::HttpSubdomain(subdomain) => {
                        inner.subdomains.release(&subdomain);
                        if let Some(tunnel) = inner.http_tunnels.remove(&subdomain) {
                            tunnel.cancellation.cancel();
                        }
                    }
                    ClientResource::TcpPort(port) => {
                        inner.ports.release(port);
                        if let Some(tunnel) = inner.tcp_tunnels.remove(&port) {
                            tunnel.cancellation.cancel();
                        }
                    }
                }
            }
        }
        inner
            .pending_data
            .retain(|_, pending| pending.client_id != client_id);
    }

    pub async fn http_tunnel_for_host(&self, host: &str) -> Option<TunnelHandle> {
        let inner = self.inner.lock().await;
        let subdomain = subdomain_for_host(host, &inner.domain)?;
        inner.http_tunnels.get(&subdomain).cloned()
    }

    pub async fn tcp_tunnel_for_port(&self, port: u16) -> Option<TunnelHandle> {
        self.inner.lock().await.tcp_tunnels.get(&port).cloned()
    }

    async fn attach_data_connection(
        &self,
        session_id: &str,
        client_identity_id: &str,
        socket: WebSocket,
    ) -> Result<(), WebSocket> {
        let mut inner = self.inner.lock().await;
        if inner
            .pending_data
            .get(session_id)
            .is_some_and(|pending| pending.client_identity_id != client_identity_id)
        {
            return Err(socket);
        }
        let pending = inner.pending_data.remove(session_id);
        drop(inner);
        match pending {
            Some(pending) => pending.sender.send(socket),
            None => Err(socket),
        }
    }
}

#[derive(Debug)]
pub struct ServerConfig {
    pub domain: String,
    pub token: Option<String>,
    pub config: Option<String>,
    pub control_listen: SocketAddr,
    pub http_listen: SocketAddr,
    pub tcp_port_range: String,
    pub http_public_scheme: String,
    pub http_public_port: Option<u16>,
}

pub async fn run(config: ServerConfig) -> anyhow::Result<()> {
    validate_http_public_scheme(&config.http_public_scheme)?;
    let auth = match (config.token, config.config) {
        (Some(token), None) => AuthConfig::legacy_token(token)?,
        (None, Some(path)) => AuthConfig::from_file(&path)?,
        (Some(_), Some(_)) => anyhow::bail!("--token and --config cannot be used together"),
        (None, None) => anyhow::bail!("one of --token or --config is required"),
    };
    let state = ServerState::new(
        config.domain.clone(),
        PortAllocator::parse_range(&config.tcp_port_range)?,
        config.http_public_scheme,
        config.http_public_port,
    );
    let control_listener = TcpListener::bind(config.control_listen).await?;
    let http_listener = TcpListener::bind(config.http_listen).await?;
    log_server_info(&format!(
        "control listening on {} for domain {}",
        config.control_listen, config.domain
    ));
    log_server_info(&format!("http listening on {}", config.http_listen));
    let app = Router::new()
        .route("/_rproxy", get(control_ws))
        .with_state(AppState {
            state: state.clone(),
            auth,
            hello_permits: Arc::new(Semaphore::new(MAX_PENDING_HELLOS)),
        });
    let mut tasks = JoinSet::new();
    tasks.spawn(async move {
        axum::serve(control_listener, app)
            .tcp_nodelay(true)
            .await
            .map_err(anyhow::Error::from)
    });
    tasks.spawn(run_http_listener(state, http_listener));
    tasks.join_next().await.unwrap()??;
    tasks.abort_all();
    Ok(())
}

#[derive(Clone)]
struct AppState {
    state: ServerState,
    auth: AuthConfig,
    hello_permits: Arc<Semaphore>,
}

async fn control_ws(State(app): State<AppState>, ws: WebSocketUpgrade) -> axum::response::Response {
    let Ok(permit) = app.hello_permits.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    ws.max_message_size(MAX_DATA_SIZE + 5)
        .max_frame_size(MAX_DATA_SIZE + 5)
        .on_upgrade(move |socket| handle_socket(app, socket, permit))
}

async fn handle_socket(app: AppState, mut socket: WebSocket, hello_permit: OwnedSemaphorePermit) {
    let hello = match timeout(HELLO_TIMEOUT, socket.recv()).await {
        Ok(Some(Ok(Message::Text(text)))) => serde_json::from_str::<ClientHello>(&text).ok(),
        _ => None,
    };
    let Some(hello) = hello else {
        log_server_warn("websocket closed or timed out before a valid hello");
        let _ = socket.close().await;
        return;
    };
    drop(hello_permit);
    match hello {
        ClientHello::Control { token, service } => {
            let Some(identity) = app.auth.client_id_for_token(&token).map(str::to_owned) else {
                let _ =
                    send_error(socket, ServerErrorCode::AuthFailed, "authentication failed").await;
                return;
            };
            handle_registered_control(app.state, socket, identity, service).await;
        }
        ClientHello::Data { token, session_id } => {
            let Some(identity) = app.auth.client_id_for_token(&token).map(str::to_owned) else {
                let _ =
                    send_error(socket, ServerErrorCode::AuthFailed, "authentication failed").await;
                return;
            };
            if let Err(socket) = app
                .state
                .attach_data_connection(&session_id, &identity, socket)
                .await
            {
                let _ = send_error(
                    socket,
                    ServerErrorCode::InvalidRequest,
                    "unknown session id",
                )
                .await;
            }
        }
    }
}

async fn handle_registered_control(
    state: ServerState,
    mut socket: WebSocket,
    client_identity_id: String,
    service: ServiceRequest,
) {
    let client_id = Uuid::new_v4().to_string();
    let cancellation = CancellationToken::new();
    let (open_tx, open_rx) = mpsc::channel(OPEN_QUEUE_SIZE);
    let is_tcp = matches!(service, ServiceRequest::Tcp { .. });
    let (registered, data_rx) = match state
        .register_control(
            client_id.clone(),
            client_identity_id.clone(),
            service,
            open_tx,
            cancellation.clone(),
        )
        .await
    {
        Ok(value) => value,
        Err(error) => {
            let code = match error {
                ServerStateError::SubdomainUnavailable(_) => ServerErrorCode::SubdomainUnavailable,
                ServerStateError::PortUnavailable(_) => ServerErrorCode::PortUnavailable,
                ServerStateError::PortNotAllowed(_) => ServerErrorCode::PortNotAllowed,
                ServerStateError::PortRangeExhausted => ServerErrorCode::PortRangeExhausted,
                ServerStateError::InvalidPortRange | ServerStateError::InvalidSubdomain(_) => {
                    ServerErrorCode::InvalidRequest
                }
            };
            let _ = send_error(socket, code, &error.to_string()).await;
            return;
        }
    };

    let mut listener_task = None;
    if is_tcp {
        if let Some(port) = registered.remote_port {
            match TcpListener::bind(("0.0.0.0", port)).await {
                Ok(listener) => {
                    let tunnel = state.tcp_tunnel_for_port(port).await.unwrap();
                    listener_task =
                        Some(tokio::spawn(run_tcp_port_listener(port, tunnel, listener)));
                }
                Err(error) => {
                    state.release_client(&client_id).await;
                    let _ =
                        send_error(socket, ServerErrorCode::PortUnavailable, &error.to_string())
                            .await;
                    return;
                }
            }
        }
    }

    let message = ServerMessage::Registered {
        session_id: registered.session_id,
        public: registered.public,
        subdomain: registered.subdomain,
        remote_port: registered.remote_port,
    };
    if socket
        .send(Message::Text(serde_json::to_string(&message).unwrap()))
        .await
        .is_err()
    {
        cancellation.cancel();
    } else {
        log_server_info(&format!(
            "registered tunnel for client identity {client_identity_id}: {}",
            message_public(&message)
        ));
        let (sender, mut receiver) = socket.split();
        let data_socket = tokio::select! {
            result = timeout(DATA_ATTACH_TIMEOUT, data_rx) => result.ok().and_then(Result::ok),
            _ = wait_for_control_close(&mut receiver) => None,
            _ = cancellation.cancelled() => None,
        };
        if let Some(data_socket) = data_socket {
            let mut mux = Box::pin(run_server_mux(data_socket, open_rx, cancellation.clone()));
            tokio::select! {
                result = &mut mux => {
                    if let Err(error) = result { log_server_warn(&format!("data websocket failed: {error}")); }
                }
                _ = wait_for_control_close(&mut receiver) => {
                    cancellation.cancel();
                    let _ = mux.await;
                }
                _ = cancellation.cancelled() => {
                    let _ = mux.await;
                }
            }
        }
        drop(sender);
    }
    cancellation.cancel();
    state.release_client(&client_id).await;
    if let Some(task) = listener_task {
        task.abort();
        let _ = task.await;
    }
}

async fn wait_for_control_close(receiver: &mut SplitStream<WebSocket>) {
    while let Some(message) = receiver.next().await {
        if matches!(message, Ok(Message::Close(_)) | Err(_)) {
            break;
        }
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
    socket
        .send(Message::Text(serde_json::to_string(
            &ServerMessage::Error {
                code,
                message: message.to_string(),
            },
        )?))
        .await?;
    Ok(())
}

async fn run_tcp_port_listener(
    port: u16,
    tunnel: TunnelHandle,
    listener: TcpListener,
) -> anyhow::Result<()> {
    loop {
        let permit = tokio::select! {
            result = tunnel.permits.clone().acquire_owned() => result?,
            _ = tunnel.cancellation.cancelled() => return Ok(()),
        };
        let (stream, _) = tokio::select! {
            result = listener.accept() => result?,
            _ = tunnel.cancellation.cancelled() => return Ok(()),
        };
        stream.set_nodelay(true)?;
        log_server_debug(&format!(
            "tcp connection accepted: port {port} -> {}",
            tunnel.local
        ));
        if tunnel
            .open_tx
            .send(OpenCommand {
                stream,
                initial: None,
                _permit: permit,
            })
            .await
            .is_err()
        {
            return Ok(());
        }
    }
}

async fn run_http_listener(state: ServerState, listener: TcpListener) -> anyhow::Result<()> {
    let admission = Arc::new(Semaphore::new(1024));
    let mut tasks = JoinSet::new();
    loop {
        while tasks.len() >= 1024 {
            let _ = tasks.join_next().await;
        }
        let permit = admission.clone().acquire_owned().await?;
        let (stream, _) = listener.accept().await?;
        stream.set_nodelay(true)?;
        let state = state.clone();
        tasks.spawn(async move {
            let _permit = permit;
            if let Err(error) = handle_http_stream(state, stream).await {
                log_server_debug(&format!("http stream failed: {error}"));
            }
        });
    }
}

async fn handle_http_stream(state: ServerState, mut stream: TcpStream) -> anyhow::Result<()> {
    let initial = match timeout(Duration::from_secs(5), read_http_headers(&mut stream)).await {
        Ok(Ok(initial)) => initial,
        Ok(Err(HttpHeaderError::TooLarge)) => {
            stream.write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
            return Ok(());
        }
        Ok(Err(HttpHeaderError::Incomplete)) => {
            stream
                .write_all(
                    b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
            return Ok(());
        }
        Ok(Err(HttpHeaderError::Io(error))) => return Err(error.into()),
        Err(_) => {
            stream.write_all(b"HTTP/1.1 408 Request Timeout\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await?;
            return Ok(());
        }
    };
    let Some(host) = http_host(&initial) else {
        stream
            .write_all(b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    let Some(tunnel) = state.http_tunnel_for_host(&host).await else {
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    };
    let permit = tokio::select! {
        result = tunnel.permits.clone().acquire_owned() => result?,
        _ = tunnel.cancellation.cancelled() => return Ok(()),
    };
    tunnel
        .open_tx
        .send(OpenCommand {
            stream,
            initial: Some(initial),
            _permit: permit,
        })
        .await?;
    Ok(())
}

#[derive(Debug, Error)]
enum HttpHeaderError {
    #[error("HTTP request headers exceed 64 KiB")]
    TooLarge,
    #[error("incomplete HTTP request headers")]
    Incomplete,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

async fn read_http_headers(stream: &mut TcpStream) -> Result<Vec<u8>, HttpHeaderError> {
    let mut buffer = Vec::new();
    let mut chunk = [0; 1024];
    loop {
        let n = stream.read(&mut chunk).await?;
        if n == 0 {
            return Err(HttpHeaderError::Incomplete);
        }
        buffer.extend_from_slice(&chunk[..n]);
        if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            return if end + 4 <= 64 * 1024 {
                Ok(buffer)
            } else {
                Err(HttpHeaderError::TooLarge)
            };
        }
        if buffer.len() > 64 * 1024 {
            return Err(HttpHeaderError::TooLarge);
        }
    }
}

fn http_host(initial: &[u8]) -> Option<String> {
    let end = initial
        .windows(4)
        .position(|window| window == b"\r\n\r\n")?;
    let request = std::str::from_utf8(&initial[..end]).ok()?;
    let mut hosts = request.split("\r\n").skip(1).filter_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.eq_ignore_ascii_case("host").then(|| value.trim())
    });
    let host = hosts.next()?.to_string();
    (!host.is_empty() && hosts.next().is_none()).then_some(host)
}

struct StreamEntry {
    events: mpsc::Sender<DataFrame>,
    cancellation: CancellationToken,
}

async fn run_server_mux(
    socket: WebSocket,
    mut open_rx: mpsc::Receiver<OpenCommand>,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (writer_tx, mut writer_rx) = mpsc::channel::<DataFrame>(WRITER_QUEUE_SIZE);
    let writer_cancellation = cancellation.clone();
    let mut writer = tokio::spawn(async move {
        while let Some(frame) = writer_rx.recv().await {
            ws_sender.send(Message::Binary(frame.encode()?)).await?;
        }
        anyhow::Ok(())
    });
    let mut streams = HashMap::<u32, StreamEntry>::new();
    let mut tasks = JoinSet::<u32>::new();
    let mut next_stream_id = 1_u32;
    let result = loop {
        tokio::select! {
            _ = cancellation.cancelled() => break Ok(()),
            result = &mut writer => {
                break match result { Ok(result) => result, Err(error) => Err(error.into()) };
            }
            Some(stream_id) = tasks.join_next() => {
                if let Ok(stream_id) = stream_id { streams.remove(&stream_id); }
            }
            command = open_rx.recv() => {
                let Some(command) = command else { break Ok(()); };
                let stream_id = next_stream_id;
                next_stream_id = next_stream_id.wrapping_add(1).max(1);
                let (events_tx, events_rx) = mpsc::channel(STREAM_QUEUE_SIZE);
                let stream_cancellation = cancellation.child_token();
                streams.insert(stream_id, StreamEntry { events: events_tx, cancellation: stream_cancellation.clone() });
                if writer_tx.send(DataFrame::Open { stream_id }).await.is_err() {
                    streams.remove(&stream_id);
                    break Err(anyhow::anyhow!("data websocket writer closed"));
                }
                let stream_writer = writer_tx.clone();
                tasks.spawn(async move {
                    if run_stream(command.stream, command.initial, events_rx, stream_writer.clone(), stream_cancellation).await.is_err() {
                        let _ = stream_writer.send(DataFrame::Reset { stream_id }).await;
                    }
                    drop(command._permit);
                    stream_id
                });
            }
            incoming = ws_receiver.next() => {
                let frame = match incoming {
                    Some(Ok(Message::Binary(data))) => match DataFrame::decode(&data) {
                        Ok(frame) => frame,
                        Err(error) => break Err(error.into()),
                    },
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break Err(anyhow::anyhow!("data websocket closed")),
                    Some(Ok(_)) => continue,
                };
                let stream_id = frame.stream_id();
                if matches!(&frame, DataFrame::Open { .. }) {
                    break Err(anyhow::anyhow!("client sent Open frame"));
                }
                let Some(entry) = streams.get(&stream_id) else {
                    if matches!(frame, DataFrame::Reset { .. }) {
                        continue;
                    }
                    writer_tx.send(DataFrame::Reset { stream_id }).await?;
                    continue;
                };
                if entry.events.try_send(frame).is_err() {
                    if let Some(entry) = streams.remove(&stream_id) { entry.cancellation.cancel(); }
                    writer_tx.send(DataFrame::Reset { stream_id }).await?;
                }
            }
        }
    };
    writer_cancellation.cancel();
    for entry in streams.values() {
        entry.cancellation.cancel();
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    writer.abort();
    let _ = writer.await;
    result
}

async fn run_stream(
    stream: TcpStream,
    initial: Option<Vec<u8>>,
    mut events: mpsc::Receiver<DataFrame>,
    writer: mpsc::Sender<DataFrame>,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let ready = timeout(READY_TIMEOUT, events.recv())
        .await?
        .ok_or_else(|| anyhow::anyhow!("stream closed before Ready"))?;
    let stream_id = match ready {
        DataFrame::Ready { stream_id } => stream_id,
        DataFrame::Reset { .. } => return Ok(()),
        _ => anyhow::bail!("expected Ready frame"),
    };
    let (mut reader, mut tcp_writer) = stream.into_split();
    let mut credit = INITIAL_CREDIT;
    let mut local_fin = false;
    let mut peer_fin = false;
    let mut initial = initial.unwrap_or_default();
    let mut initial_offset = 0;
    let mut buffer = [0_u8; MAX_DATA_SIZE];
    while !(local_fin && peer_fin) {
        if credit > 0 && initial_offset < initial.len() {
            let end = (initial_offset + MAX_DATA_SIZE).min(initial.len());
            writer
                .send(DataFrame::Data {
                    stream_id,
                    payload: initial[initial_offset..end].to_vec(),
                })
                .await?;
            initial_offset = end;
            credit -= 1;
            if initial_offset == initial.len() {
                initial.clear();
            }
            continue;
        }
        tokio::select! {
            _ = cancellation.cancelled() => return Ok(()),
            event = events.recv() => match event {
                Some(DataFrame::Data { payload, .. }) => {
                    tcp_writer.write_all(&payload).await?;
                    writer.send(DataFrame::Credit { stream_id, amount: 1 }).await?;
                }
                Some(DataFrame::Credit { amount, .. }) if credit + amount <= INITIAL_CREDIT => credit += amount,
                Some(DataFrame::Fin { .. }) => { tcp_writer.shutdown().await?; peer_fin = true; }
                Some(DataFrame::Reset { .. }) | None => return Ok(()),
                _ => anyhow::bail!("invalid stream frame sequence"),
            },
            read = reader.read(&mut buffer), if credit > 0 && initial_offset >= initial.len() && !local_fin => {
                let n = read?;
                if n == 0 {
                    writer.send(DataFrame::Fin { stream_id }).await?;
                    local_fin = true;
                } else {
                    writer.send(DataFrame::Data { stream_id, payload: buffer[..n].to_vec() }).await?;
                    credit -= 1;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_client_identity_config() {
        let auth = AuthConfig::from_toml(
            r#"
[[clients]]
id = "one"
token = "secret-1"
[[clients]]
id = "two"
token = "secret-2"
"#,
        )
        .unwrap();
        assert_eq!(auth.client_id_for_token("secret-1"), Some("one"));
        assert_eq!(auth.client_id_for_token("secret-2"), Some("two"));
    }

    #[test]
    fn rejects_duplicate_client_identity_ids() {
        let error = AuthConfig::from_toml(
            r#"
[[clients]]
id = "one"
token = "secret-1"
[[clients]]
id = "one"
token = "secret-2"
"#,
        )
        .unwrap_err();
        assert!(error.to_string().contains("client ids must be unique"));
    }

    #[test]
    fn ignores_host_lines_in_request_body() {
        assert_eq!(
            http_host(b"POST / HTTP/1.0\r\nContent-Length: 18\r\n\r\nHost: foo.a.com\r\n"),
            None
        );
    }
}

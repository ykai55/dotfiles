use crate::protocol::{
    ClientHello, DataFrame, ServerMessage, ServiceRequest, INITIAL_CREDIT, MAX_DATA_SIZE,
};
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;
use url::Url;

const MAX_STREAMS: usize = 64;
const STREAM_QUEUE_SIZE: usize = INITIAL_CREDIT as usize * 2 + 4;
const WRITER_QUEUE_SIZE: usize = 128;
const WEBSOCKET_HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const REGISTRATION_TIMEOUT: Duration = Duration::from_secs(5);
const LOCAL_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub struct ClientConfig {
    pub server: String,
    pub token: String,
    pub service: ClientServiceConfig,
}

#[derive(Debug)]
pub enum ClientServiceConfig {
    Http {
        local: String,
        subdomain: Option<String>,
    },
    Tcp {
        local: String,
        remote_port: Option<u16>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ClientError {
    #[error("--server must be a ws:// or wss:// base URL without a path, query, fragment, or credentials")]
    InvalidServerUrl,
    #[error("--local must be a host:port address, got {0:?}; try 127.0.0.1:{0}")]
    InvalidLocalAddress(String),
}

#[derive(Debug, Error)]
enum ControlConnectionError {
    #[error(transparent)]
    Disconnected(#[from] anyhow::Error),
    #[error("server error {0:?}: {1}")]
    Rejected(crate::protocol::ServerErrorCode, String),
}

pub fn control_url(server: &str) -> Result<String, ClientError> {
    let mut url = Url::parse(server).map_err(|_| ClientError::InvalidServerUrl)?;
    if !matches!(url.scheme(), "ws" | "wss")
        || url.host().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(ClientError::InvalidServerUrl);
    }
    url.set_path("/_rproxy");
    Ok(url.into())
}

pub fn validate_local_addr(local: &str) -> Result<(), ClientError> {
    if local.to_socket_addrs().is_ok() {
        Ok(())
    } else {
        Err(ClientError::InvalidLocalAddress(local.to_string()))
    }
}

fn client_log_line(message: &str) -> String {
    format!("[rproxy client] {message}")
}

fn log_client_info(message: &str) {
    tracing::info!("{}", client_log_line(message));
}

fn log_client_debug(message: &str) {
    tracing::debug!("{}", client_log_line(message));
}

fn log_client_warn(message: &str) {
    tracing::warn!("{}", client_log_line(message));
}

fn control_reconnect_delay() -> Duration {
    Duration::from_secs(1)
}

pub async fn run(config: ClientConfig) -> anyhow::Result<()> {
    let url = control_url(&config.server)?;
    let local = match &config.service {
        ClientServiceConfig::Http { local, .. } | ClientServiceConfig::Tcp { local, .. } => local,
    };
    validate_local_addr(local)?;
    let service = match &config.service {
        ClientServiceConfig::Http { local, subdomain } => ServiceRequest::Http {
            local: local.clone(),
            subdomain: subdomain.clone(),
        },
        ClientServiceConfig::Tcp { local, remote_port } => ServiceRequest::Tcp {
            local: local.clone(),
            remote_port: *remote_port,
        },
    };
    loop {
        match run_control_connection(&url, &config, service.clone()).await {
            Ok(()) => {}
            Err(ControlConnectionError::Disconnected(error)) => {
                log_client_warn(&format!("control websocket disconnected: {error}"));
            }
            Err(ControlConnectionError::Rejected(code, message)) => {
                anyhow::bail!("server error {code:?}: {message}");
            }
        }
        sleep(control_reconnect_delay()).await;
        log_client_info("reconnecting control websocket");
    }
}

async fn connect_websocket(
    url: &str,
) -> anyhow::Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>>
{
    let websocket_config = WebSocketConfig {
        max_write_buffer_size: 256 * 1024,
        max_message_size: Some(MAX_DATA_SIZE + 5),
        max_frame_size: Some(MAX_DATA_SIZE + 5),
        ..WebSocketConfig::default()
    };
    timeout(
        WEBSOCKET_HANDSHAKE_TIMEOUT,
        connect_async_with_config(url, Some(websocket_config), true),
    )
    .await
    .map_err(|_| anyhow::anyhow!("websocket handshake timed out"))?
    .map(|(socket, _)| socket)
    .map_err(anyhow::Error::from)
}

async fn run_control_connection(
    url: &str,
    config: &ClientConfig,
    service: ServiceRequest,
) -> Result<(), ControlConnectionError> {
    log_client_info(&format!("connecting control websocket: {url}"));
    let mut control = connect_websocket(url).await?;
    let hello = serde_json::to_string(&ClientHello::Control {
        token: config.token.clone(),
        service,
    })
    .map_err(anyhow::Error::from)?;
    control
        .send(Message::Text(hello))
        .await
        .map_err(anyhow::Error::from)?;
    let registered = timeout(REGISTRATION_TIMEOUT, control.next())
        .await
        .map_err(|_| anyhow::anyhow!("registration timed out"))?
        .ok_or_else(|| anyhow::anyhow!("control websocket closed"))?
        .map_err(anyhow::Error::from)?;
    let Message::Text(text) = registered else {
        return Err(anyhow::anyhow!("expected registration message").into());
    };
    let (session_id, public) =
        match serde_json::from_str::<ServerMessage>(&text).map_err(anyhow::Error::from)? {
            ServerMessage::Registered {
                session_id, public, ..
            } => (session_id, public),
            ServerMessage::Error { code, message } => {
                return Err(ControlConnectionError::Rejected(code, message));
            }
        };
    let local = match &config.service {
        ClientServiceConfig::Http { local, .. } | ClientServiceConfig::Tcp { local, .. } => {
            local.clone()
        }
    };
    log_client_info(&format!("registered tunnel: {public} -> {local}"));

    let mut data = connect_websocket(url).await?;
    let hello = serde_json::to_string(&ClientHello::Data {
        token: config.token.clone(),
        session_id,
    })
    .map_err(anyhow::Error::from)?;
    data.send(Message::Text(hello))
        .await
        .map_err(anyhow::Error::from)?;

    let cancellation = CancellationToken::new();
    let (control_sender, mut control_receiver) = control.split();
    let mut mux = Box::pin(run_client_mux(data, local, cancellation.clone()));
    let result = tokio::select! {
        result = &mut mux => result,
        _ = async {
            while let Some(message) = control_receiver.next().await {
                if matches!(message, Ok(Message::Close(_)) | Err(_)) {
                    break;
                }
            }
        } => {
            cancellation.cancel();
            mux.await
        }
    };
    drop(control_sender);
    result.map_err(ControlConnectionError::Disconnected)
}

struct StreamEntry {
    events: mpsc::Sender<DataFrame>,
    cancellation: CancellationToken,
}

async fn run_client_mux<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    local: String,
    cancellation: CancellationToken,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (writer_tx, mut writer_rx) = mpsc::channel::<DataFrame>(WRITER_QUEUE_SIZE);
    let mut writer = tokio::spawn(async move {
        while let Some(frame) = writer_rx.recv().await {
            ws_sender.send(Message::Binary(frame.encode()?)).await?;
        }
        anyhow::Ok(())
    });
    let permits = Arc::new(Semaphore::new(MAX_STREAMS));
    let mut streams = HashMap::<u32, StreamEntry>::new();
    let mut tasks = JoinSet::<u32>::new();
    let result = loop {
        tokio::select! {
            _ = cancellation.cancelled() => break Ok(()),
            result = &mut writer => break match result { Ok(result) => result, Err(error) => Err(error.into()) },
            Some(result) = tasks.join_next() => {
                if let Ok(stream_id) = result { streams.remove(&stream_id); }
            }
            incoming = ws_receiver.next() => {
                let frame = match incoming {
                    Some(Ok(Message::Binary(data))) => DataFrame::decode(&data)?,
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break Err(anyhow::anyhow!("data websocket closed")),
                    Some(Ok(_)) => continue,
                };
                let stream_id = frame.stream_id();
                if matches!(&frame, DataFrame::Open { .. }) {
                    if streams.contains_key(&stream_id) {
                        break Err(anyhow::anyhow!("duplicate stream id {stream_id}"));
                    }
                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                        writer_tx.send(DataFrame::Reset { stream_id }).await?;
                        continue;
                    };
                    let (events_tx, events_rx) = mpsc::channel(STREAM_QUEUE_SIZE);
                    let stream_cancellation = cancellation.child_token();
                    streams.insert(stream_id, StreamEntry { events: events_tx, cancellation: stream_cancellation.clone() });
                    let stream_writer = writer_tx.clone();
                    let stream_local = local.clone();
                    tasks.spawn(async move {
                        if run_local_stream(stream_id, stream_local, events_rx, stream_writer.clone(), stream_cancellation, permit).await.is_err() {
                            let _ = stream_writer.send(DataFrame::Reset { stream_id }).await;
                        }
                        stream_id
                    });
                    continue;
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
    cancellation.cancel();
    for entry in streams.values() {
        entry.cancellation.cancel();
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    writer.abort();
    let _ = writer.await;
    result
}

async fn run_local_stream(
    stream_id: u32,
    local: String,
    events: mpsc::Receiver<DataFrame>,
    writer: mpsc::Sender<DataFrame>,
    cancellation: CancellationToken,
    _permit: OwnedSemaphorePermit,
) -> anyhow::Result<()> {
    log_client_debug(&format!(
        "connecting local target for stream {stream_id}: {local}"
    ));
    let stream = tokio::select! {
        _ = cancellation.cancelled() => return Ok(()),
        result = timeout(LOCAL_CONNECT_TIMEOUT, TcpStream::connect(&local)) => {
            result.map_err(|_| anyhow::anyhow!("local connect timed out"))??
        }
    };
    stream.set_nodelay(true)?;
    writer.send(DataFrame::Ready { stream_id }).await?;
    pump_stream(stream_id, stream, events, writer, cancellation).await
}

async fn pump_stream(
    stream_id: u32,
    stream: TcpStream,
    mut events: mpsc::Receiver<DataFrame>,
    writer: mpsc::Sender<DataFrame>,
    cancellation: CancellationToken,
) -> anyhow::Result<()> {
    let (mut reader, mut tcp_writer) = stream.into_split();
    let mut credit = INITIAL_CREDIT;
    let mut local_fin = false;
    let mut peer_fin = false;
    let mut buffer = [0_u8; MAX_DATA_SIZE];
    while !(local_fin && peer_fin) {
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
            read = reader.read(&mut buffer), if credit > 0 && !local_fin => {
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
    fn maps_ws_server_to_control_url() {
        assert_eq!(
            control_url("ws://127.0.0.1:7000").unwrap(),
            "ws://127.0.0.1:7000/_rproxy"
        );
    }

    #[test]
    fn rejects_malformed_or_ambiguous_server_urls() {
        for server in [
            "ws://",
            "ws://a.com/base",
            "ws://a.com?query=value",
            "ws://user:password@a.com",
        ] {
            assert_eq!(
                control_url(server).unwrap_err(),
                ClientError::InvalidServerUrl
            );
        }
    }

    #[test]
    fn rejects_local_address_without_host() {
        assert_eq!(
            validate_local_addr("9000").unwrap_err(),
            ClientError::InvalidLocalAddress("9000".into())
        );
    }

    #[test]
    fn reconnect_delay_is_short() {
        assert_eq!(control_reconnect_delay(), Duration::from_secs(1));
    }
}

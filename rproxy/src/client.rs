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
use tokio::sync::{mpsc, watch, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinSet;
use tokio::time::{sleep, timeout, Duration, Instant};
use tokio_tungstenite::connect_async_with_config;
use tokio_tungstenite::tungstenite::error::Error as WebSocketError;
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
const MAX_WEBSOCKET_REDIRECTS: usize = 5;
const CONTROL_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);
const CONTROL_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(10);
const DATA_WRITE_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(30);
const STABLE_CONNECTION_DURATION: Duration = Duration::from_secs(60);

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
    #[error("--server must be a domain or ws:// or wss:// base URL without a path, query, fragment, or credentials")]
    InvalidServerUrl,
    #[error("--local must be a host:port address, got {0:?}; try 127.0.0.1:{0}")]
    InvalidLocalAddress(String),
}

#[derive(Debug, Error)]
enum ControlConnectionError {
    #[error(transparent)]
    Disconnected(#[from] anyhow::Error),
    #[error("{source}")]
    EstablishedConnectionLost {
        source: anyhow::Error,
        connected_for: Duration,
    },
    #[error("server error {0:?}: {1}")]
    Rejected(crate::protocol::ServerErrorCode, String),
}

pub fn control_url(server: &str) -> Result<String, ClientError> {
    let server = if server.contains("://") {
        server.to_string()
    } else {
        format!("ws://{server}")
    };
    let mut url = Url::parse(&server).map_err(|_| ClientError::InvalidServerUrl)?;
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

fn control_reconnect_delay(consecutive_failures: u32, jitter: f64) -> Duration {
    let exponent = consecutive_failures.saturating_sub(1).min(5);
    let base = Duration::from_secs((1_u64 << exponent).min(MAX_RECONNECT_DELAY.as_secs()));
    let jitter_factor = 0.5 + jitter.clamp(0.0, 1.0) * 0.5;
    Duration::from_secs_f64(
        (base.as_secs_f64() * jitter_factor).min(MAX_RECONNECT_DELAY.as_secs_f64()),
    )
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
    let mut consecutive_failures = 0_u32;
    loop {
        match run_control_connection(&url, &config, service.clone()).await {
            Ok(()) => {}
            Err(ControlConnectionError::Disconnected(error)) => {
                log_client_warn(&format!("control websocket disconnected: {error}"));
            }
            Err(ControlConnectionError::EstablishedConnectionLost {
                source,
                connected_for,
            }) => {
                log_client_warn(&format!("control websocket disconnected: {source}"));
                if connected_for >= STABLE_CONNECTION_DURATION {
                    consecutive_failures = 0;
                }
            }
            Err(ControlConnectionError::Rejected(code, message)) => {
                anyhow::bail!("server error {code:?}: {message}");
            }
        }
        consecutive_failures = consecutive_failures.saturating_add(1);
        let delay = control_reconnect_delay(consecutive_failures, rand::random());
        log_client_info(&format!(
            "reconnecting control websocket in {:.1}s",
            delay.as_secs_f64()
        ));
        sleep(delay).await;
    }
}

async fn connect_websocket(
    url: &str,
) -> anyhow::Result<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>>
{
    let mut current_url = url.to_string();
    for redirect_count in 0..=MAX_WEBSOCKET_REDIRECTS {
        let websocket_config = WebSocketConfig {
            max_write_buffer_size: 256 * 1024,
            max_message_size: Some(MAX_DATA_SIZE + 5),
            max_frame_size: Some(MAX_DATA_SIZE + 5),
            ..WebSocketConfig::default()
        };
        let result = timeout(
            WEBSOCKET_HANDSHAKE_TIMEOUT,
            connect_async_with_config(&current_url, Some(websocket_config), true),
        )
        .await
        .map_err(|_| anyhow::anyhow!("websocket handshake timed out"))?;

        match result {
            Ok((socket, _)) => return Ok(socket),
            Err(WebSocketError::Http(response)) if response.status().is_redirection() => {
                if redirect_count == MAX_WEBSOCKET_REDIRECTS {
                    anyhow::bail!(
                        "websocket handshake exceeded {MAX_WEBSOCKET_REDIRECTS} redirects"
                    );
                }
                let location = response
                    .headers()
                    .get("location")
                    .ok_or_else(|| anyhow::anyhow!("websocket redirect has no Location header"))?
                    .to_str()
                    .map_err(|_| {
                        anyhow::anyhow!("websocket redirect has an invalid Location header")
                    })?;
                current_url = websocket_redirect_url(&current_url, location)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!()
}

fn websocket_redirect_url(current_url: &str, location: &str) -> anyhow::Result<String> {
    let current = Url::parse(current_url)?;
    let mut redirect = current.join(location)?;
    let websocket_scheme = match redirect.scheme() {
        "http" | "ws" => "ws",
        "https" | "wss" => "wss",
        scheme => anyhow::bail!("websocket redirect uses unsupported scheme {scheme:?}"),
    };
    if current.scheme() == "wss" && websocket_scheme == "ws" {
        anyhow::bail!("websocket redirect cannot downgrade wss to ws");
    }
    redirect
        .set_scheme(websocket_scheme)
        .map_err(|_| anyhow::anyhow!("invalid websocket redirect scheme"))?;
    if redirect.host().is_none()
        || !redirect.username().is_empty()
        || redirect.password().is_some()
        || redirect.fragment().is_some()
    {
        anyhow::bail!("websocket redirect target is invalid");
    }
    Ok(redirect.into())
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

    let connected_at = Instant::now();
    let cancellation = CancellationToken::new();
    let mut control_monitor = Box::pin(monitor_control_connection(
        control,
        CONTROL_HEARTBEAT_INTERVAL,
        CONTROL_HEARTBEAT_TIMEOUT,
    ));
    let mut mux = Box::pin(run_client_mux(
        data,
        local,
        cancellation.clone(),
        CONTROL_HEARTBEAT_INTERVAL,
        CONTROL_HEARTBEAT_TIMEOUT,
    ));
    let result = tokio::select! {
        result = &mut mux => result,
        result = &mut control_monitor => {
            cancellation.cancel();
            let _ = mux.await;
            result
        }
    };
    result.map_err(|source| ControlConnectionError::EstablishedConnectionLost {
        source,
        connected_for: connected_at.elapsed(),
    })
}

async fn monitor_control_connection<S>(
    mut control: tokio_tungstenite::WebSocketStream<S>,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut sequence = 0_u64;
    let heartbeat = sleep(heartbeat_interval);
    tokio::pin!(heartbeat);
    loop {
        tokio::select! {
            incoming = control.next() => match incoming {
                Some(Err(error)) => return Err(error.into()),
                Some(Ok(Message::Close(_))) | None => {
                    anyhow::bail!("control websocket closed")
                }
                Some(Ok(_)) => {}
            },
            _ = &mut heartbeat => {
                sequence = sequence.wrapping_add(1);
                let payload = sequence.to_be_bytes().to_vec();
                timeout(
                    heartbeat_timeout,
                    control.send(Message::Ping(payload.clone())),
                )
                .await
                .map_err(|_| anyhow::anyhow!("control heartbeat write timed out"))??;

                timeout(heartbeat_timeout, async {
                    loop {
                        match control.next().await {
                            Some(Ok(Message::Pong(received))) if received == payload => return Ok(()),
                            Some(Err(error)) => return Err(error.into()),
                            Some(Ok(Message::Close(_))) | None => {
                                anyhow::bail!("control websocket closed")
                            }
                            Some(Ok(_)) => {}
                        }
                    }
                })
                .await
                .map_err(|_| anyhow::anyhow!("control heartbeat timed out"))??;
                heartbeat.as_mut().reset(Instant::now() + heartbeat_interval);
            }
        }
    }
}

struct StreamEntry {
    events: mpsc::Sender<DataFrame>,
    cancellation: CancellationToken,
}

async fn run_client_mux<S>(
    socket: tokio_tungstenite::WebSocketStream<S>,
    local: String,
    cancellation: CancellationToken,
    heartbeat_interval: Duration,
    heartbeat_timeout: Duration,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (writer_tx, mut writer_rx) = mpsc::channel::<DataFrame>(WRITER_QUEUE_SIZE);
    let (heartbeat_ack_tx, mut heartbeat_ack_rx) = watch::channel(0_u64);
    let writer_cancellation = cancellation.clone();
    let mut writer = tokio::spawn(async move {
        let mut heartbeat_sequence = 0_u64;
        let heartbeat = sleep(heartbeat_interval);
        tokio::pin!(heartbeat);
        loop {
            tokio::select! {
                _ = writer_cancellation.cancelled() => return Ok(()),
                outgoing = writer_rx.recv() => {
                    let Some(frame) = outgoing else { return Ok(()); };
                    let encoded = frame.encode()?;
                    tokio::select! {
                        _ = writer_cancellation.cancelled() => return Ok(()),
                        result = timeout(DATA_WRITE_TIMEOUT, ws_sender.send(Message::Binary(encoded))) => {
                            result.map_err(|_| anyhow::anyhow!("data websocket write timed out"))??;
                        }
                    }
                }
                _ = &mut heartbeat => {
                    heartbeat_sequence = heartbeat_sequence.wrapping_add(1);
                    let payload = heartbeat_sequence.to_be_bytes().to_vec();
                    tokio::select! {
                        _ = writer_cancellation.cancelled() => return Ok(()),
                        result = timeout(heartbeat_timeout, ws_sender.send(Message::Ping(payload))) => {
                            result.map_err(|_| anyhow::anyhow!("data heartbeat write timed out"))??;
                        }
                    }
                    tokio::select! {
                        _ = writer_cancellation.cancelled() => return Ok(()),
                        result = timeout(heartbeat_timeout, heartbeat_ack_rx.wait_for(|ack| *ack == heartbeat_sequence)) => {
                            result.map_err(|_| anyhow::anyhow!("data heartbeat timed out"))?
                                .map_err(|_| anyhow::anyhow!("data heartbeat acknowledgement channel closed"))?;
                        }
                    }
                    heartbeat.as_mut().reset(Instant::now() + heartbeat_interval);
                }
            }
        }
    });
    let permits = Arc::new(Semaphore::new(MAX_STREAMS));
    let mut streams = HashMap::<u32, StreamEntry>::new();
    let mut tasks = JoinSet::<u32>::new();
    let mut writer_finished = false;
    let result = async {
        loop {
            tokio::select! {
                _ = cancellation.cancelled() => break Ok(()),
                result = &mut writer => {
                    writer_finished = true;
                    break match result { Ok(result) => result, Err(error) => Err(error.into()) };
                }
            Some(result) = tasks.join_next() => {
                if let Ok(stream_id) = result { streams.remove(&stream_id); }
            }
            incoming = ws_receiver.next() => {
                let frame = match incoming {
                    Some(Ok(Message::Binary(data))) => DataFrame::decode(&data)?,
                    Some(Ok(Message::Pong(received))) if received.len() == 8 => {
                        let mut sequence = [0_u8; 8];
                        sequence.copy_from_slice(&received);
                        heartbeat_ack_tx.send_replace(u64::from_be_bytes(sequence));
                        continue;
                    }
                    Some(Err(error)) => break Err(error.into()),
                    Some(Ok(Message::Close(_))) | None => break Err(anyhow::anyhow!("data websocket closed")),
                    Some(Ok(_)) => continue,
                };
                let stream_id = frame.stream_id();
                if matches!(&frame, DataFrame::Open { .. }) {
                    if streams.contains_key(&stream_id) {
                        break Err(anyhow::anyhow!("duplicate stream id {stream_id}"));
                    }
                    let Ok(permit) = permits.clone().try_acquire_owned() else {
                        writer_tx.try_send(DataFrame::Reset { stream_id })
                            .map_err(|_| anyhow::anyhow!("data websocket writer queue full"))?;
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
                    writer_tx.try_send(DataFrame::Reset { stream_id })
                        .map_err(|_| anyhow::anyhow!("data websocket writer queue full"))?;
                    continue;
                };
                if entry.events.try_send(frame).is_err() {
                    if let Some(entry) = streams.remove(&stream_id) { entry.cancellation.cancel(); }
                    writer_tx.try_send(DataFrame::Reset { stream_id })
                        .map_err(|_| anyhow::anyhow!("data websocket writer queue full"))?;
                }
            }
            }
        }
    }
    .await;
    cancellation.cancel();
    for entry in streams.values() {
        entry.cancellation.cancel();
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    if !writer_finished {
        writer.abort();
        let _ = writer.await;
    }
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
    fn maps_bare_server_domain_to_ws_control_url() {
        assert_eq!(
            control_url("rp.example.com").unwrap(),
            "ws://rp.example.com/_rproxy"
        );
    }

    #[test]
    fn maps_https_redirect_to_wss() {
        assert_eq!(
            websocket_redirect_url(
                "ws://rp.example.com/_rproxy",
                "https://rp.example.com/_rproxy"
            )
            .unwrap(),
            "wss://rp.example.com/_rproxy"
        );
    }

    #[test]
    fn rejects_wss_redirect_downgrade() {
        assert!(websocket_redirect_url(
            "wss://rp.example.com/_rproxy",
            "http://rp.example.com/_rproxy"
        )
        .is_err());
    }

    #[tokio::test]
    async fn follows_websocket_handshake_redirect() {
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target_addr = target.local_addr().unwrap();
        let target_task = tokio::spawn(async move {
            let (stream, _) = target.accept().await.unwrap();
            tokio_tungstenite::accept_async(stream).await.unwrap()
        });

        let redirect = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let redirect_addr = redirect.local_addr().unwrap();
        let redirect_task = tokio::spawn(async move {
            let (mut stream, _) = redirect.accept().await.unwrap();
            let mut request = [0; 1024];
            stream.read(&mut request).await.unwrap();
            stream
                .write_all(
                    format!(
                        "HTTP/1.1 302 Found\r\nLocation: ws://{target_addr}/_rproxy\r\nContent-Length: 0\r\n\r\n"
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });

        let socket = connect_websocket(&format!("ws://{redirect_addr}/_rproxy"))
            .await
            .unwrap();
        drop(socket);
        redirect_task.await.unwrap();
        drop(target_task.await.unwrap());
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
    fn reconnect_delay_increases_and_stays_bounded() {
        assert_eq!(control_reconnect_delay(1, 1.0), Duration::from_secs(1));
        assert_eq!(control_reconnect_delay(2, 1.0), Duration::from_secs(2));
        assert_eq!(control_reconnect_delay(20, 1.0), Duration::from_secs(30));
        assert_eq!(control_reconnect_delay(20, 0.0), Duration::from_secs(15));
    }

    #[tokio::test]
    async fn control_heartbeat_times_out_when_peer_stops_responding() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            sleep(Duration::from_secs(1)).await;
        });
        let socket = connect_websocket(&format!("ws://{address}")).await.unwrap();

        let error = monitor_control_connection(
            socket,
            Duration::from_millis(50),
            Duration::from_millis(150),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("heartbeat timed out"));
        server.abort();
    }

    #[tokio::test]
    async fn control_heartbeat_keeps_responsive_connection_alive() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            while socket.next().await.is_some() {}
        });
        let socket = connect_websocket(&format!("ws://{address}")).await.unwrap();
        let monitor = monitor_control_connection(
            socket,
            Duration::from_millis(50),
            Duration::from_millis(150),
        );

        assert!(timeout(Duration::from_millis(500), monitor).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn data_heartbeat_times_out_when_peer_stops_responding() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            sleep(Duration::from_secs(1)).await;
        });
        let socket = connect_websocket(&format!("ws://{address}")).await.unwrap();

        let error = run_client_mux(
            socket,
            "127.0.0.1:1".into(),
            CancellationToken::new(),
            Duration::from_millis(50),
            Duration::from_millis(100),
        )
        .await
        .unwrap_err();

        assert!(error.to_string().contains("data heartbeat timed out"));
        server.abort();
    }

    #[tokio::test]
    async fn data_heartbeat_keeps_responsive_connection_alive() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            while socket.next().await.is_some() {}
        });
        let socket = connect_websocket(&format!("ws://{address}")).await.unwrap();
        let mux = run_client_mux(
            socket,
            "127.0.0.1:1".into(),
            CancellationToken::new(),
            Duration::from_millis(50),
            Duration::from_millis(150),
        );

        assert!(timeout(Duration::from_millis(500), mux).await.is_err());
        server.abort();
    }

    #[tokio::test]
    async fn data_mux_stops_promptly_when_cancelled() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            sleep(Duration::from_secs(1)).await;
        });
        let socket = connect_websocket(&format!("ws://{address}")).await.unwrap();
        let cancellation = CancellationToken::new();
        let mux = run_client_mux(
            socket,
            "127.0.0.1:1".into(),
            cancellation.clone(),
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        cancellation.cancel();

        timeout(Duration::from_millis(200), mux)
            .await
            .expect("data mux should stop when its control connection is cancelled")
            .unwrap();
        server.abort();
    }
}

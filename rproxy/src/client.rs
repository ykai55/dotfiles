use crate::protocol::{ClientHello, ServerMessage, ServiceRequest};
use futures_util::{SinkExt, StreamExt};
use std::net::{SocketAddr, ToSocketAddrs};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

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
    #[error("--server must start with ws:// or wss://")]
    InvalidServerUrl,
    #[error("--local must be a host:port address, got {0:?}; try 127.0.0.1:{0}")]
    InvalidLocalAddress(String),
}

pub fn control_url(server: &str) -> Result<String, ClientError> {
    if !server.starts_with("ws://") && !server.starts_with("wss://") {
        return Err(ClientError::InvalidServerUrl);
    }

    Ok(format!("{}/_rproxy", server.trim_end_matches('/')))
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
    debug_assert_eq!(data_connection_log_level(), tracing::Level::DEBUG);
    tracing::debug!("{}", client_log_line(message));
}

fn log_client_warn(message: &str) {
    debug_assert_eq!(recoverable_failure_log_level(), tracing::Level::WARN);
    tracing::warn!("{}", client_log_line(message));
}

fn data_connection_log_level() -> tracing::Level {
    tracing::Level::DEBUG
}

fn recoverable_failure_log_level() -> tracing::Level {
    tracing::Level::WARN
}

pub async fn run(config: ClientConfig) -> anyhow::Result<()> {
    let control_url = control_url(&config.server)?;
    match &config.service {
        ClientServiceConfig::Http { local, .. } | ClientServiceConfig::Tcp { local, .. } => {
            validate_local_addr(local)?;
        }
    }
    log_client_info(&format!("connecting control websocket: {control_url}"));
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

    let (mut socket, _) = connect_async(&control_url).await?;
    log_client_info("control websocket connected");
    socket
        .send(Message::Text(serde_json::to_string(
            &ClientHello::Control {
                token: config.token.clone(),
                service,
            },
        )?))
        .await?;
    log_client_info("registration request sent");

    while let Some(message) = socket.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        match serde_json::from_str::<ServerMessage>(&text)? {
            ServerMessage::Registered { public, .. } => match &config.service {
                ClientServiceConfig::Http { local, .. } => {
                    log_client_info(&format!("registered HTTP tunnel: {public} -> {local}"));
                    println!("HTTP tunnel ready: {public} -> {local}");
                }
                ClientServiceConfig::Tcp { local, .. } => {
                    log_client_info(&format!("registered TCP tunnel: {public} -> {local}"));
                    println!("TCP tunnel ready: {public} -> {local}");
                }
            },
            ServerMessage::Open { connection_id } => {
                log_client_debug(&format!("opening data connection: {connection_id}"));
                let token = config.token.clone();
                let control_url = control_url.clone();
                let local = match &config.service {
                    ClientServiceConfig::Http { local, .. } => local.clone(),
                    ClientServiceConfig::Tcp { local, .. } => local.clone(),
                };
                tokio::spawn(async move {
                    let logged_connection_id = connection_id.clone();
                    if let Err(error) =
                        handle_data_connection(control_url, token, connection_id, local).await
                    {
                        log_client_warn(&format!(
                            "data connection {logged_connection_id} failed: {error}"
                        ));
                    }
                });
            }
            ServerMessage::Error { code, message } => {
                log_client_warn(&format!("server rejected request: {code:?}: {message}"));
                anyhow::bail!("server error {code:?}: {message}");
            }
        }
    }

    Ok(())
}

async fn handle_data_connection(
    control_url: String,
    token: String,
    connection_id: String,
    local: String,
) -> anyhow::Result<()> {
    log_client_debug(&format!("connecting data websocket: {connection_id}"));
    let (mut socket, _) = connect_async(&control_url).await?;
    let logged_connection_id = connection_id.clone();
    socket
        .send(Message::Text(serde_json::to_string(&ClientHello::Data {
            token,
            connection_id,
        })?))
        .await?;
    log_client_debug(&format!("connecting local target: {local}"));
    let local = TcpStream::connect(local).await?;
    log_client_debug(&format!("data connection ready: {logged_connection_id}"));
    proxy_local_with_websocket(local, socket).await
}

async fn proxy_local_with_websocket<S>(
    local: TcpStream,
    socket: tokio_tungstenite::WebSocketStream<S>,
) -> anyhow::Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (mut ws_sender, mut ws_receiver) = socket.split();
    let (mut local_reader, mut local_writer) = local.into_split();

    let ws_to_local = tokio::spawn(async move {
        while let Some(message) = ws_receiver.next().await {
            match message? {
                Message::Binary(data) => local_writer.write_all(&data).await?,
                Message::Close(_) => break,
                _ => {}
            }
        }
        anyhow::Ok(())
    });

    let local_to_ws = tokio::spawn(async move {
        let mut buffer = [0; 8192];
        loop {
            let n = local_reader.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            ws_sender
                .send(Message::Binary(buffer[..n].to_vec()))
                .await?;
        }
        anyhow::Ok(())
    });

    tokio::select! {
        result = ws_to_local => result??,
        result = local_to_ws => result??,
    }

    Ok(())
}

#[allow(dead_code)]
fn _assert_socket_addr_send_sync(_: SocketAddr) {}

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
    fn maps_wss_server_to_control_url() {
        assert_eq!(control_url("wss://a.com").unwrap(), "wss://a.com/_rproxy");
    }

    #[test]
    fn strips_trailing_slash_before_internal_path() {
        assert_eq!(control_url("wss://a.com/").unwrap(), "wss://a.com/_rproxy");
    }

    #[test]
    fn rejects_server_without_ws_scheme() {
        assert_eq!(
            control_url("a.com").unwrap_err(),
            ClientError::InvalidServerUrl
        );
    }

    #[test]
    fn rejects_local_address_without_host() {
        assert_eq!(
            validate_local_addr("9000").unwrap_err(),
            ClientError::InvalidLocalAddress("9000".into())
        );
    }

    #[test]
    fn accepts_local_address_with_host_and_port() {
        assert_eq!(validate_local_addr("127.0.0.1:9000").unwrap(), ());
    }

    #[test]
    fn formats_client_log_line() {
        assert_eq!(
            client_log_line("connecting to ws://127.0.0.1:7000/_rproxy"),
            "[rproxy client] connecting to ws://127.0.0.1:7000/_rproxy"
        );
    }

    #[test]
    fn data_connection_logs_are_debug() {
        assert_eq!(data_connection_log_level(), tracing::Level::DEBUG);
    }

    #[test]
    fn recoverable_failures_are_warn() {
        assert_eq!(recoverable_failure_log_level(), tracing::Level::WARN);
    }
}

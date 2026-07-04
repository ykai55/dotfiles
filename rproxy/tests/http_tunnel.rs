use futures_util::{SinkExt, StreamExt};
use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::protocol::{ClientHello, ServerMessage, ServiceRequest};
use rproxy::server::ServerConfig;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

fn free_addr() -> SocketAddr {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

async fn start_echo_http() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0; 4096];
                let Ok(n) = stream.read(&mut buffer).await else {
                    return;
                };
                let request = String::from_utf8_lossy(&buffer[..n]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");
                let body = format!("echo:{path}");
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });
    addr
}

#[tokio::test]
async fn proxies_http_request_by_host_header() {
    let local_http = start_echo_http().await;
    let control_listen = free_addr();
    let http_listen = free_addr();

    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: "secret".into(),
        control_listen,
        http_listen,
        tcp_port_range: "20000-20010".into(),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));

    sleep(Duration::from_millis(100)).await;

    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{control_listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Http {
            local: local_http.to_string(),
            subdomain: Some("foo".into()),
        },
    }));

    sleep(Duration::from_millis(200)).await;

    let mut stream = TcpStream::connect(http_listen).await.unwrap();
    stream
        .write_all(b"GET /hello HTTP/1.1\r\nHost: foo.test\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    let mut buffer = [0; 1024];
    timeout(Duration::from_secs(2), async {
        loop {
            let n = stream.read(&mut buffer).await.unwrap();
            if n == 0 {
                break;
            }
            response.extend_from_slice(&buffer[..n]);
            if response.ends_with(b"echo:/hello") {
                break;
            }
        }
    })
    .await
    .unwrap();
    let response = String::from_utf8(response).unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("echo:/hello"), "{response}");

    client.abort();
    server.abort();
}

#[tokio::test]
async fn registers_http_tunnel_with_configured_public_url() {
    let control_listen = free_addr();
    let http_listen = free_addr();

    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: "secret".into(),
        control_listen,
        http_listen,
        tcp_port_range: "20000-20010".into(),
        http_public_scheme: "https".into(),
        http_public_port: Some(444),
    }));

    sleep(Duration::from_millis(100)).await;

    let (mut socket, _) = connect_async(format!("ws://{control_listen}/_rproxy"))
        .await
        .unwrap();
    socket
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Control {
                token: "secret".into(),
                service: ServiceRequest::Http {
                    local: "127.0.0.1:9000".into(),
                    subdomain: Some("foo".into()),
                },
            })
            .unwrap(),
        ))
        .await
        .unwrap();

    let Some(Ok(Message::Text(text))) = timeout(Duration::from_secs(3), socket.next())
        .await
        .unwrap()
    else {
        panic!("expected registered message");
    };
    let ServerMessage::Registered { public, .. } = serde_json::from_str(&text).unwrap() else {
        panic!("expected registered message");
    };
    assert_eq!(public, "https://foo.test:444");

    server.abort();
}

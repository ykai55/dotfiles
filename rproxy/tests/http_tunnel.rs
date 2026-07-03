use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::server::ServerConfig;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout, Duration};

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

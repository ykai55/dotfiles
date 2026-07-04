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

async fn start_echo_tcp() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                loop {
                    let Ok(n) = stream.read(&mut buffer).await else {
                        break;
                    };
                    if n == 0 {
                        break;
                    }
                    if stream.write_all(&buffer[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    addr
}

#[tokio::test]
async fn proxies_tcp_bytes_through_requested_port() {
    let local_tcp = start_echo_tcp().await;
    let control_listen = free_addr();
    let http_listen = free_addr();
    let remote_addr = free_addr();
    let remote_port = remote_addr.port();

    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: "secret".into(),
        control_listen,
        http_listen,
        tcp_port_range: format!("{remote_port}-{remote_port}"),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));

    sleep(Duration::from_millis(100)).await;

    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{control_listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Tcp {
            local: local_tcp.to_string(),
            remote_port: Some(remote_port),
        },
    }));

    sleep(Duration::from_millis(200)).await;

    let mut stream = TcpStream::connect(("127.0.0.1", remote_port))
        .await
        .unwrap();
    stream.write_all(b"ping").await.unwrap();
    let mut response = [0; 4];
    timeout(Duration::from_secs(2), stream.read_exact(&mut response))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(&response, b"ping");

    client.abort();
    server.abort();
}

#[tokio::test]
async fn tcp_registration_fails_when_remote_port_cannot_bind() {
    let local_tcp = start_echo_tcp().await;
    let control_listen = free_addr();
    let http_listen = free_addr();
    let occupied = TcpListener::bind("0.0.0.0:0").await.unwrap();
    let remote_port = occupied.local_addr().unwrap().port();

    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: "secret".into(),
        control_listen,
        http_listen,
        tcp_port_range: format!("{remote_port}-{remote_port}"),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));

    sleep(Duration::from_millis(100)).await;

    let error = timeout(
        Duration::from_secs(2),
        rproxy::client::run(ClientConfig {
            server: format!("ws://{control_listen}"),
            token: "secret".into(),
            service: ClientServiceConfig::Tcp {
                local: local_tcp.to_string(),
                remote_port: Some(remote_port),
            },
        }),
    )
    .await
    .expect("client should fail registration instead of staying connected")
    .unwrap_err()
    .to_string();

    assert!(error.contains("server error PortUnavailable"), "{error}");

    drop(occupied);
    server.abort();
}

#[tokio::test]
async fn tcp_listener_is_released_after_client_disconnects() {
    let local_tcp = start_echo_tcp().await;
    let control_listen = free_addr();
    let http_listen = free_addr();
    let remote_addr = free_addr();
    let remote_port = remote_addr.port();

    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: "secret".into(),
        control_listen,
        http_listen,
        tcp_port_range: format!("{remote_port}-{remote_port}"),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));

    sleep(Duration::from_millis(100)).await;

    let first_client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{control_listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Tcp {
            local: local_tcp.to_string(),
            remote_port: Some(remote_port),
        },
    }));

    sleep(Duration::from_millis(200)).await;
    first_client.abort();
    sleep(Duration::from_millis(200)).await;

    let second_client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{control_listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Tcp {
            local: local_tcp.to_string(),
            remote_port: Some(remote_port),
        },
    }));

    sleep(Duration::from_millis(200)).await;

    let mut stream = TcpStream::connect(("127.0.0.1", remote_port))
        .await
        .unwrap();
    stream.write_all(b"pong").await.unwrap();
    let mut response = [0; 4];
    timeout(Duration::from_secs(2), stream.read_exact(&mut response))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(&response, b"pong");

    second_client.abort();
    server.abort();
}

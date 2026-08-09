use futures_util::{SinkExt, StreamExt};
use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::protocol::{ClientHello, DataFrame, ServerMessage, ServiceRequest};
use rproxy::server::ServerConfig;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

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

async fn start_counting_echo_tcp() -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_for_task = accepted.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            accepted_for_task.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let mut buffer = [0; 1024];
                loop {
                    let Ok(n) = stream.read(&mut buffer).await else {
                        break;
                    };
                    if n == 0 || stream.write_all(&buffer[..n]).await.is_err() {
                        break;
                    }
                }
            });
        }
    });
    (addr, accepted)
}

async fn start_counting_tcp_proxy(target: SocketAddr) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let accepted_for_task = accepted.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut incoming, _)) = listener.accept().await else {
                break;
            };
            accepted_for_task.fetch_add(1, Ordering::SeqCst);
            tokio::spawn(async move {
                let Ok(mut outgoing) = TcpStream::connect(target).await else {
                    return;
                };
                let _ = tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await;
            });
        }
    });
    (addr, accepted)
}

async fn start_eof_response_tcp() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        stream.read_to_end(&mut request).await.unwrap();
        stream.write_all(b"received:").await.unwrap();
        stream.write_all(&request).await.unwrap();
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
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
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
async fn limits_active_tcp_streams_per_tunnel() {
    let (local_tcp, accepted) = start_counting_echo_tcp().await;
    let control_listen = free_addr();
    let http_listen = free_addr();
    let remote_port = free_addr().port();
    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
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

    let mut active = Vec::new();
    for _ in 0..64 {
        let mut stream = TcpStream::connect(("127.0.0.1", remote_port))
            .await
            .unwrap();
        stream.write_all(b"x").await.unwrap();
        let mut response = [0; 1];
        timeout(Duration::from_secs(2), stream.read_exact(&mut response))
            .await
            .unwrap()
            .unwrap();
        active.push(stream);
    }
    assert_eq!(accepted.load(Ordering::SeqCst), 64);

    let mut excess = TcpStream::connect(("127.0.0.1", remote_port))
        .await
        .unwrap();
    excess.write_all(b"x").await.unwrap();
    let mut response = [0; 1];
    assert!(
        timeout(Duration::from_millis(250), excess.read_exact(&mut response))
            .await
            .is_err(),
        "the 65th active stream reached the local service"
    );
    assert_eq!(accepted.load(Ordering::SeqCst), 64);

    drop(active.pop());
    timeout(Duration::from_secs(2), excess.read_exact(&mut response))
        .await
        .expect("the queued stream should proceed after one active stream closes")
        .unwrap();
    assert_eq!(&response, b"x");

    drop(active);
    client.abort();
    server.abort();
}

#[tokio::test]
async fn multiplexes_concurrent_streams_over_one_data_websocket() {
    let local_tcp = start_echo_tcp().await;
    let control_backend = free_addr();
    let http_listen = free_addr();
    let remote_port = free_addr().port();
    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
        control_listen: control_backend,
        http_listen,
        tcp_port_range: format!("{remote_port}-{remote_port}"),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));
    sleep(Duration::from_millis(100)).await;
    let (control_proxy, accepted) = start_counting_tcp_proxy(control_backend).await;
    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{control_proxy}"),
        token: "secret".into(),
        service: ClientServiceConfig::Tcp {
            local: local_tcp.to_string(),
            remote_port: Some(remote_port),
        },
    }));
    sleep(Duration::from_millis(200)).await;

    let mut streams = Vec::new();
    for byte in 0_u8..8 {
        let mut stream = TcpStream::connect(("127.0.0.1", remote_port))
            .await
            .unwrap();
        stream.write_all(&[byte]).await.unwrap();
        let mut response = [0];
        timeout(Duration::from_secs(2), stream.read_exact(&mut response))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(response, [byte]);
        streams.push(stream);
    }
    assert_eq!(
        accepted.load(Ordering::SeqCst),
        2,
        "one control and one data websocket should serve all streams"
    );

    drop(streams);
    client.abort();
    server.abort();
}

#[tokio::test]
async fn propagates_tcp_half_close_in_both_directions() {
    let local_tcp = start_eof_response_tcp().await;
    let control_listen = free_addr();
    let http_listen = free_addr();
    let remote_port = free_addr().port();
    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
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
    stream.write_all(b"request").await.unwrap();
    stream.shutdown().await.unwrap();
    let mut response = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .expect("half-closed tunnel should return a response")
        .unwrap();

    assert_eq!(response, b"received:request");

    client.abort();
    server.abort();
}

#[tokio::test]
async fn control_disconnect_closes_active_tcp_data_connection() {
    let control_listen = free_addr();
    let http_listen = free_addr();
    let remote_port = free_addr().port();
    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
        control_listen,
        http_listen,
        tcp_port_range: format!("{remote_port}-{remote_port}"),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));
    let control_url = format!("ws://{control_listen}/_rproxy");
    let (mut control_socket, _) = timeout(Duration::from_secs(2), async {
        loop {
            if let Ok(socket) = connect_async(&control_url).await {
                break socket;
            }
            sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("server control listener should become ready");
    control_socket
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Control {
                token: "secret".into(),
                service: ServiceRequest::Tcp {
                    local: "127.0.0.1:9000".into(),
                    remote_port: Some(remote_port),
                },
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    let Some(Ok(Message::Text(text))) = control_socket.next().await else {
        panic!("expected registered message");
    };
    let ServerMessage::Registered { session_id, .. } =
        serde_json::from_str::<ServerMessage>(&text).unwrap()
    else {
        panic!("expected registered message");
    };

    let (mut data_socket, _) = connect_async(format!("ws://{control_listen}/_rproxy"))
        .await
        .unwrap();
    data_socket
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Data {
                token: "secret".into(),
                session_id,
            })
            .unwrap(),
        ))
        .await
        .unwrap();

    let mut external = TcpStream::connect(("127.0.0.1", remote_port))
        .await
        .unwrap();
    let Some(Ok(Message::Binary(frame))) = data_socket.next().await else {
        panic!("expected Open frame");
    };
    let DataFrame::Open { stream_id } = DataFrame::decode(&frame).unwrap() else {
        panic!("expected Open frame");
    };
    data_socket
        .send(Message::Binary(
            DataFrame::Ready { stream_id }.encode().unwrap(),
        ))
        .await
        .unwrap();
    external.write_all(b"ping").await.unwrap();
    let Some(Ok(Message::Binary(data))) = data_socket.next().await else {
        panic!("expected relayed TCP bytes");
    };
    assert_eq!(
        DataFrame::decode(&data).unwrap(),
        DataFrame::Data {
            stream_id,
            payload: b"ping".to_vec()
        }
    );

    control_socket.close(None).await.unwrap();
    let mut byte = [0; 1];
    let n = timeout(Duration::from_secs(2), external.read(&mut byte))
        .await
        .expect("control disconnect should close active data connection")
        .unwrap();
    assert_eq!(n, 0);
    let closed = timeout(Duration::from_secs(2), data_socket.next())
        .await
        .expect("control disconnect should close data websocket");
    assert!(
        closed.is_none() || matches!(closed, Some(Ok(Message::Close(_))) | Some(Err(_))),
        "unexpected data message after control close: {closed:?}"
    );

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
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
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
        token: Some("secret".into()),
        auth_db: None,
        configured_credentials: Default::default(),
        management_listen: free_addr(),
        management_token_file: None,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
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

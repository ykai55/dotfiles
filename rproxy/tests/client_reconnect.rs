use futures_util::{SinkExt, StreamExt};
use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::protocol::{ClientHello, DataFrame, ServerErrorCode, ServerMessage, ServiceRequest};
use rproxy::server::ServerConfig;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::{advance, sleep, timeout, Duration};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

fn free_addr() -> SocketAddr {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

#[tokio::test]
async fn retries_temporary_registration_conflicts() {
    let listen = free_addr();
    let listener = TcpListener::bind(listen).await.unwrap();
    let second_registration = Arc::new(Notify::new());
    let second_registration_for_server = second_registration.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut control = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(_))) = control.next().await else {
            panic!("expected first control hello");
        };
        control
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Error {
                    code: ServerErrorCode::SubdomainUnavailable,
                    message: "subdomain foo is unavailable".into(),
                })
                .unwrap(),
            ))
            .await
            .unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut control = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(_))) = control.next().await else {
            panic!("expected retried control hello");
        };
        second_registration_for_server.notify_one();
    });
    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Http {
            local: "127.0.0.1:1".into(),
            subdomain: Some("foo".into()),
        },
    }));

    timeout(Duration::from_secs(3), second_registration.notified())
        .await
        .expect("client should retry a temporarily unavailable subdomain");
    client.abort();
    server.abort();
}

#[tokio::test(start_paused = true)]
async fn server_releases_subdomain_when_control_heartbeat_times_out() {
    let control_listen = free_addr();
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
        http_listen: free_addr(),
        tcp_port_range: "20000-20010".into(),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));
    let url = format!("ws://{control_listen}/_rproxy");
    let (mut old_control, _) = loop {
        if let Ok(socket) = connect_async(&url).await {
            break socket;
        }
        tokio::task::yield_now().await;
    };
    old_control
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Control {
                token: "secret".into(),
                service: ServiceRequest::Http {
                    local: "127.0.0.1:1".into(),
                    subdomain: Some("foo".into()),
                },
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    let Some(Ok(Message::Text(text))) = old_control.next().await else {
        panic!("expected first registration");
    };
    let ServerMessage::Registered { session_id, .. } =
        serde_json::from_str::<ServerMessage>(&text).unwrap()
    else {
        panic!("expected first registration");
    };
    let (mut old_data, _) = connect_async(&url).await.unwrap();
    old_data
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Data {
                token: "secret".into(),
                session_id,
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    tokio::task::yield_now().await;

    advance(Duration::from_secs(30)).await;
    tokio::task::yield_now().await;
    advance(Duration::from_secs(11)).await;
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }

    let (mut replacement, _) = connect_async(&url).await.unwrap();
    replacement
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Control {
                token: "secret".into(),
                service: ServiceRequest::Http {
                    local: "127.0.0.1:1".into(),
                    subdomain: Some("foo".into()),
                },
            })
            .unwrap(),
        ))
        .await
        .unwrap();
    let Some(Ok(Message::Text(text))) = replacement.next().await else {
        panic!("expected replacement registration");
    };
    assert!(matches!(
        serde_json::from_str::<ServerMessage>(&text).unwrap(),
        ServerMessage::Registered { .. }
    ));

    server.abort();
}

#[tokio::test]
async fn reconnects_after_initial_connection_failure() {
    let listen = free_addr();
    let listener = TcpListener::bind(listen).await.unwrap();
    let first_attempt_failed = Arc::new(Notify::new());
    let first_attempt_failed_for_server = first_attempt_failed.clone();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);
        first_attempt_failed_for_server.notify_one();

        let (stream, _) = listener.accept().await.unwrap();
        let mut control = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(text))) = control.next().await else {
            panic!("expected control hello");
        };
        assert!(matches!(
            serde_json::from_str::<ClientHello>(&text).unwrap(),
            ClientHello::Control { .. }
        ));
        control
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Registered {
                    session_id: "session-after-outage".into(),
                    public: "http://foo.test".into(),
                    subdomain: Some("foo".into()),
                    remote_port: None,
                })
                .unwrap(),
            ))
            .await
            .unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut data = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(text))) = data.next().await else {
            panic!("expected data hello");
        };
        assert!(matches!(
            serde_json::from_str::<ClientHello>(&text).unwrap(),
            ClientHello::Data { .. }
        ));
    });

    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Http {
            local: "127.0.0.1:1".into(),
            subdomain: Some("foo".into()),
        },
    }));

    timeout(Duration::from_secs(1), first_attempt_failed.notified())
        .await
        .expect("client should make an initial connection attempt");
    timeout(Duration::from_secs(3), server)
        .await
        .expect("client should reconnect after the initial connection fails")
        .unwrap();
    client.abort();
}

#[tokio::test]
async fn reconnects_after_control_websocket_closes() {
    let listen = free_addr();
    let listener = TcpListener::bind(listen).await.unwrap();
    let second_registration = Arc::new(Notify::new());
    let second_registration_for_server = second_registration.clone();

    let server = tokio::spawn(async move {
        for attempt in 1..=2 {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            let Some(Ok(Message::Text(text))) = socket.next().await else {
                panic!("expected control hello");
            };
            let ClientHello::Control { .. } = serde_json::from_str(&text).unwrap() else {
                panic!("expected control hello");
            };
            socket
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Registered {
                        session_id: format!("session-{attempt}"),
                        public: "http://foo.test".into(),
                        subdomain: Some("foo".into()),
                        remote_port: None,
                    })
                    .unwrap(),
                ))
                .await
                .unwrap();
            let (stream, _) = listener.accept().await.unwrap();
            let mut data = accept_async(stream).await.unwrap();
            let Some(Ok(Message::Text(text))) = data.next().await else {
                panic!("expected data hello");
            };
            assert!(matches!(
                serde_json::from_str::<ClientHello>(&text).unwrap(),
                ClientHello::Data { .. }
            ));
            if attempt == 2 {
                second_registration_for_server.notify_one();
                sleep(Duration::from_secs(10)).await;
            }
        }
    });

    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Http {
            local: "127.0.0.1:1".into(),
            subdomain: Some("foo".into()),
        },
    }));

    timeout(Duration::from_secs(3), second_registration.notified())
        .await
        .unwrap();

    client.abort();
    server.abort();
}

#[tokio::test]
async fn closes_data_websocket_when_control_websocket_closes() {
    let listen = free_addr();
    let listener = TcpListener::bind(listen).await.unwrap();
    let local = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let local_addr = local.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut control = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(text))) = control.next().await else {
            panic!("expected control hello");
        };
        assert!(matches!(
            serde_json::from_str::<ClientHello>(&text).unwrap(),
            ClientHello::Control { .. }
        ));
        control
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Registered {
                    session_id: "session-1".into(),
                    public: "test:20000".into(),
                    subdomain: None,
                    remote_port: Some(20000),
                })
                .unwrap(),
            ))
            .await
            .unwrap();
        let (stream, _) = listener.accept().await.unwrap();
        let mut data = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(text))) = data.next().await else {
            panic!("expected data hello");
        };
        assert!(matches!(
            serde_json::from_str::<ClientHello>(&text).unwrap(),
            ClientHello::Data { .. }
        ));
        data.send(Message::Binary(
            DataFrame::Open { stream_id: 1 }.encode().unwrap(),
        ))
        .await
        .unwrap();
        let (mut local_stream, _) = local.accept().await.unwrap();
        let Some(Ok(Message::Binary(frame))) = data.next().await else {
            panic!("expected Ready frame");
        };
        assert_eq!(
            DataFrame::decode(&frame).unwrap(),
            DataFrame::Ready { stream_id: 1 }
        );

        control.close(None).await.unwrap();
        let closed = timeout(Duration::from_secs(2), data.next())
            .await
            .expect("client should close data websocket with control websocket");
        assert!(
            closed.is_none() || matches!(closed, Some(Ok(Message::Close(_))) | Some(Err(_))),
            "unexpected data message after control close: {closed:?}"
        );
        let mut byte = [0; 1];
        assert_eq!(
            timeout(Duration::from_secs(2), local_stream.read(&mut byte))
                .await
                .expect("client should close local TCP with control websocket")
                .unwrap(),
            0
        );
    });

    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Tcp {
            local: local_addr.to_string(),
            remote_port: Some(20000),
        },
    }));

    timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
    client.abort();
}

#[tokio::test]
async fn ignores_reset_for_an_unknown_stream() {
    let listen = free_addr();
    let listener = TcpListener::bind(listen).await.unwrap();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut control = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(_))) = control.next().await else {
            panic!("expected control hello");
        };
        control
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Registered {
                    session_id: "session-1".into(),
                    public: "test:20000".into(),
                    subdomain: None,
                    remote_port: Some(20000),
                })
                .unwrap(),
            ))
            .await
            .unwrap();

        let (stream, _) = listener.accept().await.unwrap();
        let mut data = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(_))) = data.next().await else {
            panic!("expected data hello");
        };
        data.send(Message::Binary(
            DataFrame::Reset { stream_id: 99 }.encode().unwrap(),
        ))
        .await
        .unwrap();

        assert!(
            timeout(Duration::from_millis(250), data.next())
                .await
                .is_err(),
            "an unknown Reset must not be echoed"
        );
    });
    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Tcp {
            local: "127.0.0.1:1".into(),
            remote_port: Some(20000),
        },
    }));

    timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    client.abort();
}

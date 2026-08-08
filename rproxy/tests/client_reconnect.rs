use futures_util::{SinkExt, StreamExt};
use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::protocol::{ClientHello, ServerMessage};
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;

fn free_addr() -> SocketAddr {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
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
                        public: "http://foo.test".into(),
                        subdomain: Some("foo".into()),
                        remote_port: None,
                    })
                    .unwrap(),
                ))
                .await
                .unwrap();
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
                    public: "test:20000".into(),
                    subdomain: None,
                    remote_port: Some(20000),
                })
                .unwrap(),
            ))
            .await
            .unwrap();
        control
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Open {
                    connection_id: "connection-1".into(),
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
        let (mut local_stream, _) = local.accept().await.unwrap();

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

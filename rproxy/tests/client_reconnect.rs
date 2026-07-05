use futures_util::{SinkExt, StreamExt};
use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::protocol::{ClientHello, ServerMessage};
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
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

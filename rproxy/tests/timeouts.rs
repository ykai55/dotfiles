use futures_util::{SinkExt, StreamExt};
use rproxy::client::{ClientConfig, ClientServiceConfig};
use rproxy::protocol::{ClientHello, ServerMessage};
use rproxy::server::ServerConfig;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{accept_async, connect_async};

fn free_addr() -> SocketAddr {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

#[tokio::test]
async fn server_closes_websocket_that_does_not_send_hello_before_deadline() {
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
    sleep(Duration::from_millis(100)).await;
    let (mut socket, _) = connect_async(format!("ws://{control_listen}/_rproxy"))
        .await
        .unwrap();

    let closed = timeout(Duration::from_secs(6), socket.next())
        .await
        .expect("server should enforce its hello deadline");
    assert!(
        closed.is_none() || matches!(closed, Some(Ok(Message::Close(_))) | Some(Err(_))),
        "unexpected message after hello deadline: {closed:?}"
    );

    server.abort();
}

#[tokio::test]
async fn client_retries_after_data_websocket_handshake_deadline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let listen = listener.local_addr().unwrap();
    let second_control = Arc::new(Notify::new());
    let second_control_for_server = second_control.clone();
    let fake_server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut control = accept_async(stream).await.unwrap();
        let Some(Ok(Message::Text(text))) = control.next().await else {
            panic!("expected first control hello");
        };
        assert!(matches!(
            serde_json::from_str::<ClientHello>(&text).unwrap(),
            ClientHello::Control { .. }
        ));
        control
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Registered {
                    session_id: "session-1".into(),
                    public: "http://foo.test".into(),
                    subdomain: Some("foo".into()),
                    remote_port: None,
                })
                .unwrap(),
            ))
            .await
            .unwrap();

        let (stalled_data_handshake, _) = listener.accept().await.unwrap();
        sleep(Duration::from_millis(5200)).await;
        drop(stalled_data_handshake);
        drop(control);

        let (stream, _) = listener.accept().await.unwrap();
        let _second_control_socket = accept_async(stream).await.unwrap();
        second_control_for_server.notify_one();
        sleep(Duration::from_secs(10)).await;
    });
    let client = tokio::spawn(rproxy::client::run(ClientConfig {
        server: format!("ws://{listen}"),
        token: "secret".into(),
        service: ClientServiceConfig::Http {
            local: "127.0.0.1:1".into(),
            subdomain: Some("foo".into()),
        },
    }));

    timeout(Duration::from_secs(8), second_control.notified())
        .await
        .expect("client should retry after the data handshake deadline");

    client.abort();
    fake_server.abort();
}

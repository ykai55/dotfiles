use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use rproxy::protocol::{ClientHello, ServerErrorCode, ServerMessage, ServiceRequest};
use rproxy::server::ServerConfig;
use serde_json::{json, Value};
use std::fs;
use std::net::{SocketAddr, TcpListener as StdTcpListener};
use std::path::PathBuf;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

fn free_addr() -> SocketAddr {
    let listener = StdTcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap()
}

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("rproxy-{name}-{}", Uuid::new_v4()))
}

async fn start_managed_server() -> (
    tokio::task::JoinHandle<anyhow::Result<()>>,
    SocketAddr,
    String,
    PathBuf,
    PathBuf,
) {
    let control_listen = free_addr();
    let management_listen = free_addr();
    let database = temp_path("auth.db");
    let token_file = temp_path("management-token");
    fs::write(&token_file, "management-secret\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let server = tokio::spawn(rproxy::server::run(ServerConfig {
        domain: "test".into(),
        token: None,
        auth_db: Some(database.to_string_lossy().into_owned()),
        configured_credentials: Default::default(),
        management_listen,
        management_token_file: Some(token_file.to_string_lossy().into_owned()),
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
        control_listen,
        http_listen: free_addr(),
        tcp_port_range: "20000-20010".into(),
        http_public_scheme: "http".into(),
        http_public_port: None,
    }));
    sleep(Duration::from_millis(150)).await;
    (
        server,
        control_listen,
        format!("http://{management_listen}"),
        database,
        token_file,
    )
}

async fn create_identity_and_token(base_url: &str) -> (String, String) {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base_url}/v1/client-identities"))
        .bearer_auth("management-secret")
        .json(&json!({
            "id": "build-agent",
            "subdomain_policy": { "rules": ["preview-*", "docs"] }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    let response = client
        .post(format!(
            "{base_url}/v1/client-identities/build-agent/tokens"
        ))
        .bearer_auth("management-secret")
        .json(&json!({ "label": "test", "expires_at": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body: Value = response.json().await.unwrap();
    (
        body["token"]["id"].as_str().unwrap().to_string(),
        body["secret"].as_str().unwrap().to_string(),
    )
}

async fn register_http(
    control_listen: SocketAddr,
    token: &str,
    subdomain: Option<&str>,
) -> (
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    ServerMessage,
) {
    let (mut socket, _) = connect_async(format!("ws://{control_listen}/_rproxy"))
        .await
        .unwrap();
    socket
        .send(Message::Text(
            serde_json::to_string(&ClientHello::Control {
                token: token.into(),
                service: ServiceRequest::Http {
                    local: "127.0.0.1:9000".into(),
                    subdomain: subdomain.map(str::to_owned),
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
        panic!("expected server message");
    };
    let message = serde_json::from_str(&text).unwrap();
    (socket, message)
}

#[tokio::test]
async fn manages_tokens_and_enforces_subdomain_policy() {
    let (server, control_listen, base_url, database, token_file) = start_managed_server().await;
    let client = reqwest::Client::new();

    let unauthorized = client
        .get(format!("{base_url}/v1/client-identities"))
        .send()
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    assert!(unauthorized.headers().contains_key("x-request-id"));

    let malformed = client
        .post(format!("{base_url}/v1/client-identities"))
        .bearer_auth("management-secret")
        .header("content-type", "application/json")
        .body("{")
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        malformed.json::<Value>().await.unwrap()["error"]["code"],
        "invalid_request"
    );

    let (token_id, secret) = create_identity_and_token(&base_url).await;
    assert!(secret.starts_with("rpt_"));

    let (mut preview_control, allowed) =
        register_http(control_listen, &secret, Some("preview-123")).await;
    assert!(matches!(allowed, ServerMessage::Registered { .. }));

    let (_, denied) = register_http(control_listen, &secret, Some("other")).await;
    assert!(matches!(
        denied,
        ServerMessage::Error {
            code: ServerErrorCode::SubdomainNotAllowed,
            ..
        }
    ));

    let (_, omitted) = register_http(control_listen, &secret, None).await;
    assert!(matches!(
        omitted,
        ServerMessage::Error {
            code: ServerErrorCode::SubdomainNotAllowed,
            ..
        }
    ));

    let tokens: Value = client
        .get(format!(
            "{base_url}/v1/client-identities/build-agent/tokens"
        ))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(tokens[0].get("secret").is_none());

    let token: Value = client
        .get(format!(
            "{base_url}/v1/client-identities/build-agent/tokens/{token_id}"
        ))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(token["id"], token_id);

    let response = client
        .patch(format!("{base_url}/v1/client-identities/build-agent"))
        .bearer_auth("management-secret")
        .json(&json!({ "subdomain_policy": { "rules": ["docs"] } }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let closed = timeout(Duration::from_secs(2), preview_control.next())
        .await
        .expect("policy update should close a disallowed tunnel");
    assert!(closed.is_none() || matches!(closed, Some(Ok(Message::Close(_))) | Some(Err(_))));

    let (mut docs_control, docs) = register_http(control_listen, &secret, Some("docs")).await;
    assert!(matches!(docs, ServerMessage::Registered { .. }));

    let response = client
        .delete(format!("{base_url}/v1/client-identities/build-agent"))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    let closed = timeout(Duration::from_secs(2), docs_control.next())
        .await
        .expect("identity deletion should close its tunnels");
    assert!(closed.is_none() || matches!(closed, Some(Ok(Message::Close(_))) | Some(Err(_))));

    let missing = client
        .get(format!("{base_url}/v1/client-identities/build-agent"))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);

    server.abort();
    let _ = fs::remove_file(database);
    let _ = fs::remove_file(token_file);
}

#[tokio::test]
async fn revoking_token_closes_its_active_tunnel() {
    let (server, control_listen, base_url, database, token_file) = start_managed_server().await;
    let (token_id, secret) = create_identity_and_token(&base_url).await;
    let (mut control, registered) = register_http(control_listen, &secret, Some("docs")).await;
    assert!(matches!(registered, ServerMessage::Registered { .. }));

    let response = reqwest::Client::new()
        .delete(format!(
            "{base_url}/v1/client-identities/build-agent/tokens/{token_id}"
        ))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);

    let closed = timeout(Duration::from_secs(2), control.next())
        .await
        .expect("revocation should close the control websocket");
    assert!(
        closed.is_none() || matches!(closed, Some(Ok(Message::Close(_))) | Some(Err(_))),
        "unexpected message after revocation: {closed:?}"
    );

    let (_, rejected) = register_http(control_listen, &secret, Some("docs")).await;
    assert!(matches!(
        rejected,
        ServerMessage::Error {
            code: ServerErrorCode::AuthFailed,
            ..
        }
    ));

    server.abort();
    let _ = fs::remove_file(database);
    let _ = fs::remove_file(token_file);
}

#[tokio::test]
async fn full_config_keeps_config_credentials_out_of_sqlite() {
    let control_listen = free_addr();
    let http_listen = free_addr();
    let management_listen = free_addr();
    let database = temp_path("full-config-auth.db");
    let token_file = temp_path("full-config-management-token");
    let config_file = temp_path("server.toml");
    fs::write(&token_file, "management-secret\n").unwrap();
    fs::write(
        &config_file,
        format!(
            r#"
[server]
domain = "test"
control_listen = "{control_listen}"
http_listen = "{http_listen}"
tcp_port_range = "20000-20010"
http_public_scheme = "http"

[authentication]
database = "{}"

[management]
listen = "{management_listen}"
token_file = "{}"

[[clients]]
id = "config-agent"
subdomains = ["configured"]

[[clients.tokens]]
name = "primary"
token = "config-secret"
"#,
            database.display(),
            token_file.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&token_file, fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&config_file, fs::Permissions::from_mode(0o600)).unwrap();
    }

    let server = tokio::spawn(rproxy::server::run(
        rproxy::config::load(&config_file)
            .unwrap()
            .into_server_config(),
    ));
    sleep(Duration::from_millis(150)).await;
    let base_url = format!("http://{management_listen}");

    let (_, registered) = register_http(control_listen, "config-secret", Some("configured")).await;
    assert!(matches!(registered, ServerMessage::Registered { .. }));

    let client = reqwest::Client::new();
    let identities: Value = client
        .get(format!("{base_url}/v1/client-identities"))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(identities[0]["id"], "config-agent");
    assert_eq!(identities[0]["managed_by"], "config");

    let response = client
        .delete(format!("{base_url}/v1/client-identities/config-agent"))
        .bearer_auth("management-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(
        response.json::<Value>().await.unwrap()["error"]["code"],
        "managed_by_config"
    );

    let response = client
        .post(format!(
            "{base_url}/v1/client-identities/config-agent/tokens"
        ))
        .bearer_auth("management-secret")
        .json(&json!({ "label": "forbidden", "expires_at": null }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);

    let response = client
        .post(format!("{base_url}/v1/client-identities"))
        .bearer_auth("management-secret")
        .json(&json!({
            "id": "api-agent",
            "subdomain_policy": { "rules": ["api"] }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);

    server.abort();
    let _ = server.await;
    let store = rproxy::auth::CredentialStore::open(&database).unwrap();
    let stored = store.list_identities().await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, "api-agent");

    let _ = fs::remove_file(database);
    let _ = fs::remove_file(token_file);
    let _ = fs::remove_file(config_file);
}

use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "rproxy")]
struct Cli {
    #[arg(long, action = ArgAction::SetTrue, help = "Print AI-agent oriented usage guide")]
    help_ai: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    Server(ServerArgs),
    Client(ClientArgs),
}

#[derive(Debug, Args)]
pub struct ServerArgs {
    #[arg(short = 'd', long, required_unless_present = "config")]
    pub domain: Option<String>,
    #[arg(
        short = 't',
        long,
        conflicts_with_all = ["config", "auth_db"],
        required_unless_present_any = ["config", "auth_db"],
        help = "Legacy single client token"
    )]
    pub token: Option<String>,
    #[arg(
        long,
        conflicts_with_all = [
            "domain",
            "token",
            "auth_db",
            "management_listen",
            "management_token_file",
            "control_listen",
            "http_listen",
            "tcp_port_range",
            "http_public_scheme",
            "http_public_port"
        ],
        required_unless_present_any = ["token", "auth_db"],
        help = "Complete TOML server configuration"
    )]
    pub config: Option<String>,
    #[arg(
        long,
        conflicts_with_all = ["token", "config"],
        required_unless_present_any = ["token", "config"],
        requires = "management_token_file",
        help = "SQLite authentication database managed through the management interface"
    )]
    pub auth_db: Option<String>,
    #[arg(long, default_value = "127.0.0.1:7001")]
    pub management_listen: SocketAddr,
    #[arg(long, requires = "auth_db")]
    pub management_token_file: Option<String>,
    #[arg(short = 'c', long, default_value = "127.0.0.1:7000")]
    pub control_listen: SocketAddr,
    #[arg(short = 'H', long, default_value = "0.0.0.0:8080")]
    pub http_listen: SocketAddr,
    #[arg(short = 'r', long, default_value = "20000-30000")]
    pub tcp_port_range: String,
    #[arg(long, default_value = "http")]
    pub http_public_scheme: String,
    #[arg(long)]
    pub http_public_port: Option<u16>,
}

#[derive(Debug, Args)]
pub struct ClientArgs {
    #[arg(short = 's', long)]
    pub server: String,
    #[arg(short = 't', long)]
    pub token: String,
    #[command(subcommand)]
    pub service: ClientService,
}

#[derive(Debug, Subcommand)]
pub enum ClientService {
    Http(HttpArgs),
    Tcp(TcpArgs),
}

#[derive(Debug, Args)]
pub struct HttpArgs {
    #[arg(short = 'l', long)]
    pub local: String,
    #[arg(short = 'S', long)]
    pub subdomain: Option<String>,
}

#[derive(Debug, Args)]
pub struct TcpArgs {
    #[arg(short = 'l', long)]
    pub local: String,
    #[arg(short = 'p', long)]
    pub remote_port: Option<u16>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_crypto_provider();
    let cli = Cli::parse();
    if cli.help_ai {
        println!("{}", ai_help_text());
        return Ok(());
    }

    init_logging();
    match cli.command {
        Some(Command::Server(args)) => rproxy::server::run(server_config(args)?).await,
        Some(Command::Client(args)) => rproxy::client::run(client_config(args)).await,
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
    }
}

fn ai_help_text() -> &'static str {
    r#"rproxy --help-ai

Purpose:
  rproxy exposes local HTTP or TCP services through a remote rproxy server.
  Use it when an agent needs a temporary public URL for a local service.

Core commands:
  rproxy server --domain <domain> --token <token> --control-listen 127.0.0.1:7000 --http-listen 0.0.0.0:8080
  rproxy server --config server.toml
  rproxy server --domain <domain> --auth-db auth.db --management-token-file management-token --control-listen 127.0.0.1:7000
  rproxy client --server wss://rp.example.com --token <token> http --local 127.0.0.1:8000 --subdomain <name>
  rproxy client --server wss://rp.example.com --token <token> tcp --local 127.0.0.1:22 --remote-port <port>

Server config:
  --config is a complete TOML server configuration and cannot be combined with other server options.
  It contains [server], [authentication], [management], and optional [[clients]] entries.
  Config client identities and tokens stay in memory and are never written to SQLite.
  server.toml example:
    [server]
    domain = "example.com"
    control_listen = "127.0.0.1:7000"
    http_listen = "0.0.0.0:8080"
    tcp_port_range = "20000-30000"
    http_public_scheme = "https"

    [authentication]
    database = "/var/lib/rproxy/auth.db"

    [management]
    listen = "127.0.0.1:7001"
    token_file = "/run/secrets/rproxy-management-token"
    requests_per_minute = 120
    body_limit_bytes = 16384

    [[clients]]
    id = "build-agent"
    subdomains = ["preview-*", "docs"]

    [[clients.tokens]]
    name = "primary"
    label = "production"
    token = "configured-secret"
  On Unix, server.toml and the management token file must have mode 0600.

Managed authentication:
  Start with --auth-db auth.db --management-token-file management-token.
  The management listener defaults to http://127.0.0.1:7001.
  Send this header on every management request:
    Authorization: Bearer <management-token>

Management API:
  POST   /v1/client-identities
    Body: {"id":"build-agent","subdomain_policy":{"rules":["preview-*","docs"]}}
  GET    /v1/client-identities
  GET    /v1/client-identities/{identity_id}
  PATCH  /v1/client-identities/{identity_id}
    Body: {"enabled":true,"subdomain_policy":{"rules":["docs"]}}
  DELETE /v1/client-identities/{identity_id}
  POST   /v1/client-identities/{identity_id}/tokens
    Body: {"label":"production","expires_at":null}
    Returns the token secret exactly once in the "secret" field.
  GET    /v1/client-identities/{identity_id}/tokens
  GET    /v1/client-identities/{identity_id}/tokens/{token_id}
  DELETE /v1/client-identities/{identity_id}/tokens/{token_id}

Management API rules:
  Use Content-Type: application/json for POST and PATCH requests.
  Subdomain rules are exact labels such as "docs", single-label wildcards such as "preview-*", "*-dev", "api-*-dev", or "*".
  Restricted identities must request an explicit subdomain.
  Token list/get responses never contain token secrets. Rotate by creating a new token, deploying it, then deleting the old token.
  Config-managed resources appear in reads with managed_by="config"; mutation attempts return 409 managed_by_config.
  The API cannot add tokens under a config-managed identity.
  Successful creates return 201, reads and updates return 200, and deletes return 204.
  Errors use {"error":{"code":"...","message":"..."}} and responses include X-Request-ID.

Important rules:
  --server must start with ws:// or wss://; the client appends /_rproxy automatically.
  --local must be host:port, for example 127.0.0.1:8000. Do not pass a URL.
  HTTP routing uses the Host header. HTTPS is handled by an external TLS terminator such as nginx or Caddy.
  Tunnel registrations live only while the client control WebSocket stays connected.
  For HTTP, the server prints the public URL after registration, for example https://demo.example.com.
  For TCP, the server prints the public host:port after registration.

Common deployment pattern:
  nginx terminates https://rp.example.com/_rproxy and forwards WebSocket traffic to rproxy --control-listen.
  nginx terminates https://*.example.com and forwards HTTP traffic to rproxy --http-listen with the original Host header.
  Start the server with --http-public-scheme https when TLS is terminated before rproxy.

Troubleshooting:
  404 on an HTTP tunnel usually means no connected client registered that subdomain.
  A redirect to https://*.domain/ means the nginx wildcard block is redirecting to its literal server_name.
  WebSocket reset warnings on data connections may be transient; the control connection owns tunnel lifetime.
"#
}

fn init_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_level(show_log_levels())
        .without_time()
        .init();
}

fn show_log_levels() -> bool {
    true
}

fn server_config(args: ServerArgs) -> anyhow::Result<rproxy::server::ServerConfig> {
    if let Some(path) = args.config {
        return Ok(rproxy::config::load(path)?.into_server_config());
    }
    Ok(rproxy::server::ServerConfig {
        domain: args
            .domain
            .expect("clap requires --domain without --config"),
        token: args.token,
        auth_db: args.auth_db,
        configured_credentials: Default::default(),
        management_listen: args.management_listen,
        management_token_file: args.management_token_file,
        management_requests_per_minute: 120,
        management_body_limit_bytes: 16 * 1024,
        control_listen: args.control_listen,
        http_listen: args.http_listen,
        tcp_port_range: args.tcp_port_range,
        http_public_scheme: args.http_public_scheme,
        http_public_port: args.http_public_port,
    })
}

fn client_config(args: ClientArgs) -> rproxy::client::ClientConfig {
    let service = match args.service {
        ClientService::Http(http) => rproxy::client::ClientServiceConfig::Http {
            local: http.local,
            subdomain: http.subdomain,
        },
        ClientService::Tcp(tcp) => rproxy::client::ClientServiceConfig::Tcp {
            local: tcp.local,
            remote_port: tcp.remote_port,
        },
    };

    rproxy::client::ClientConfig {
        server: args.server,
        token: args.token,
        service,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_client_http_short_flags() {
        let cli = Cli::parse_from([
            "rproxy",
            "client",
            "-s",
            "ws://127.0.0.1:7000",
            "-t",
            "secret",
            "http",
            "-l",
            "127.0.0.1:9000",
            "-S",
            "foo",
        ]);

        let Some(Command::Client(args)) = cli.command else {
            panic!("expected client command");
        };
        assert_eq!(args.server, "ws://127.0.0.1:7000");
        assert_eq!(args.token, "secret");
        let ClientService::Http(http) = args.service else {
            panic!("expected http service");
        };
        assert_eq!(http.local, "127.0.0.1:9000");
        assert_eq!(http.subdomain.as_deref(), Some("foo"));
    }

    #[test]
    fn parses_client_tcp_short_flags() {
        let cli = Cli::parse_from([
            "rproxy",
            "client",
            "-s",
            "wss://a.com",
            "-t",
            "secret",
            "tcp",
            "-l",
            "127.0.0.1:9000",
            "-p",
            "20000",
        ]);

        let Some(Command::Client(args)) = cli.command else {
            panic!("expected client command");
        };
        assert_eq!(args.server, "wss://a.com");
        assert_eq!(args.token, "secret");
        let ClientService::Tcp(tcp) = args.service else {
            panic!("expected tcp service");
        };
        assert_eq!(tcp.local, "127.0.0.1:9000");
        assert_eq!(tcp.remote_port, Some(20000));
    }

    #[test]
    fn parses_server_short_flags() {
        let cli = Cli::parse_from([
            "rproxy",
            "server",
            "-d",
            "a.com",
            "-t",
            "secret",
            "-c",
            "127.0.0.1:7000",
            "-H",
            "127.0.0.1:8080",
            "-r",
            "20000-20010",
        ]);

        let Some(Command::Server(args)) = cli.command else {
            panic!("expected server command");
        };
        assert_eq!(args.domain.as_deref(), Some("a.com"));
        assert_eq!(args.token.as_deref(), Some("secret"));
        assert_eq!(args.control_listen.to_string(), "127.0.0.1:7000");
        assert_eq!(args.http_listen.to_string(), "127.0.0.1:8080");
        assert_eq!(args.tcp_port_range, "20000-20010");
        assert_eq!(args.config, None);
        assert_eq!(args.http_public_scheme, "http");
        assert_eq!(args.http_public_port, None);
    }

    #[test]
    fn parses_server_config_file_flag() {
        let cli = Cli::parse_from(["rproxy", "server", "--config", "server.toml"]);

        let Some(Command::Server(args)) = cli.command else {
            panic!("expected server command");
        };
        assert_eq!(args.config.as_deref(), Some("server.toml"));
        assert_eq!(args.token, None);
    }

    #[test]
    fn parses_managed_server_flags() {
        let cli = Cli::parse_from([
            "rproxy",
            "server",
            "--domain",
            "a.com",
            "--auth-db",
            "auth.db",
            "--management-token-file",
            "management-token",
        ]);

        let Some(Command::Server(args)) = cli.command else {
            panic!("expected server command");
        };
        assert_eq!(args.auth_db.as_deref(), Some("auth.db"));
        assert_eq!(
            args.management_token_file.as_deref(),
            Some("management-token")
        );
        assert_eq!(args.management_listen.to_string(), "127.0.0.1:7001");
    }

    #[test]
    fn rejects_server_token_and_config_together() {
        let error = Cli::try_parse_from([
            "rproxy",
            "server",
            "--domain",
            "a.com",
            "--token",
            "secret",
            "--config",
            "clients.toml",
        ])
        .unwrap_err();

        assert!(error.to_string().contains("cannot be used with"));
    }

    #[test]
    fn rejects_full_config_with_server_option() {
        let error = Cli::try_parse_from([
            "rproxy",
            "server",
            "--config",
            "server.toml",
            "--control-listen",
            "127.0.0.1:7100",
        ])
        .unwrap_err();

        assert!(error.to_string().contains("cannot be used with"));
    }

    #[test]
    fn rejects_server_without_auth_source() {
        let error = Cli::try_parse_from(["rproxy", "server", "--domain", "a.com"]).unwrap_err();

        assert!(error.to_string().contains("required"));
    }

    #[test]
    fn parses_server_http_public_url_flags() {
        let cli = Cli::parse_from([
            "rproxy",
            "server",
            "--domain",
            "a.com",
            "--token",
            "secret",
            "--http-public-scheme",
            "https",
            "--http-public-port",
            "444",
        ]);

        let Some(Command::Server(args)) = cli.command else {
            panic!("expected server command");
        };
        assert_eq!(args.http_public_scheme, "https");
        assert_eq!(args.http_public_port, Some(444));
    }

    #[test]
    fn logging_shows_levels() {
        assert!(show_log_levels());
    }

    #[test]
    fn initializes_rustls_crypto_provider() {
        init_crypto_provider();
        init_crypto_provider();
    }

    #[test]
    fn ai_help_describes_agent_usage() {
        let help = ai_help_text();

        assert!(help.contains("rproxy --help-ai"));
        assert!(help.contains("rproxy server --domain <domain> --token <token>"));
        assert!(help.contains("rproxy server --config server.toml"));
        assert!(help.contains("rproxy server --domain <domain> --auth-db auth.db"));
        assert!(help.contains("POST   /v1/client-identities"));
        assert!(help.contains("DELETE /v1/client-identities/{identity_id}/tokens/{token_id}"));
        assert!(help.contains("Returns the token secret exactly once"));
        assert!(help.contains("Authorization: Bearer <management-token>"));
        assert!(help.contains(
            "rproxy client --server wss://rp.example.com --token <token> http --local 127.0.0.1:8000"
        ));
        assert!(help.contains("client appends /_rproxy automatically"));
        assert!(help.contains("HTTP routing uses the Host header"));
        assert!(help.contains(
            "Tunnel registrations live only while the client control WebSocket stays connected"
        ));
    }

    #[test]
    fn parses_help_ai_without_subcommand() {
        let cli = Cli::parse_from(["rproxy", "--help-ai"]);

        assert!(cli.help_ai);
        assert!(cli.command.is_none());
    }

    #[test]
    fn clap_help_explains_help_ai() {
        let mut output = Vec::new();

        Cli::command().write_help(&mut output).unwrap();
        let help = String::from_utf8(output).unwrap();

        assert!(help.contains("--help-ai"));
        assert!(help.contains("Print AI-agent oriented usage guide"));
    }
}

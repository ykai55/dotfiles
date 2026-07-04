use clap::{Args, Parser, Subcommand};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(name = "rproxy")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Server(ServerArgs),
    Client(ClientArgs),
}

#[derive(Debug, Args)]
pub struct ServerArgs {
    #[arg(short = 'd', long)]
    pub domain: String,
    #[arg(short = 't', long)]
    pub token: String,
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
    init_logging();
    let cli = Cli::parse();
    match cli.command {
        Command::Server(args) => rproxy::server::run(server_config(args)).await,
        Command::Client(args) => rproxy::client::run(client_config(args)).await,
    }
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

fn server_config(args: ServerArgs) -> rproxy::server::ServerConfig {
    rproxy::server::ServerConfig {
        domain: args.domain,
        token: args.token,
        control_listen: args.control_listen,
        http_listen: args.http_listen,
        tcp_port_range: args.tcp_port_range,
        http_public_scheme: args.http_public_scheme,
        http_public_port: args.http_public_port,
    }
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

        let Command::Client(args) = cli.command else {
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

        let Command::Client(args) = cli.command else {
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

        let Command::Server(args) = cli.command else {
            panic!("expected server command");
        };
        assert_eq!(args.domain, "a.com");
        assert_eq!(args.token, "secret");
        assert_eq!(args.control_listen.to_string(), "127.0.0.1:7000");
        assert_eq!(args.http_listen.to_string(), "127.0.0.1:8080");
        assert_eq!(args.tcp_port_range, "20000-20010");
        assert_eq!(args.http_public_scheme, "http");
        assert_eq!(args.http_public_port, None);
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

        let Command::Server(args) = cli.command else {
            panic!("expected server command");
        };
        assert_eq!(args.http_public_scheme, "https");
        assert_eq!(args.http_public_port, Some(444));
    }

    #[test]
    fn logging_shows_levels() {
        assert!(show_log_levels());
    }
}

use crate::alloc::PortAllocator;
use crate::auth::{ConfiguredClient, ConfiguredCredentials, ConfiguredToken, SubdomainPolicy};
use crate::server::ServerConfig;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::net::SocketAddr;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerConfigFile {
    server: ServerSection,
    authentication: AuthenticationSection,
    management: ManagementSection,
    #[serde(default)]
    clients: Vec<ClientSection>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerSection {
    domain: String,
    control_listen: SocketAddr,
    http_listen: SocketAddr,
    tcp_port_range: String,
    http_public_scheme: String,
    http_public_port: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AuthenticationSection {
    database: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManagementSection {
    listen: SocketAddr,
    token_file: String,
    #[serde(default = "default_requests_per_minute")]
    requests_per_minute: u32,
    #[serde(default = "default_body_limit_bytes")]
    body_limit_bytes: usize,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ClientSection {
    id: String,
    #[serde(default = "default_enabled")]
    enabled: bool,
    subdomains: Vec<String>,
    #[serde(default)]
    tokens: Vec<TokenSection>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TokenSection {
    name: String,
    label: Option<String>,
    token: String,
    expires_at: Option<String>,
}

#[derive(Debug)]
pub struct ValidatedServerConfig(ServerConfig);

impl ValidatedServerConfig {
    pub fn into_server_config(self) -> ServerConfig {
        self.0
    }
}

pub fn load(path: impl AsRef<Path>) -> anyhow::Result<ValidatedServerConfig> {
    let path = path.as_ref();
    let mut source = File::open(path)?;
    validate_permissions(&source)?;
    let mut text = String::new();
    source.read_to_string(&mut text)?;
    let file: ServerConfigFile = toml::from_str(&text)?;
    if file.management.requests_per_minute == 0 {
        anyhow::bail!("management.requests_per_minute must be greater than zero");
    }
    if file.management.body_limit_bytes == 0 {
        anyhow::bail!("management.body_limit_bytes must be greater than zero");
    }
    if file.server.domain.trim().is_empty() {
        anyhow::bail!("server.domain must not be empty");
    }
    PortAllocator::parse_range(&file.server.tcp_port_range)?;
    if !matches!(file.server.http_public_scheme.as_str(), "http" | "https") {
        anyhow::bail!("server.http_public_scheme must be http or https");
    }
    if file.authentication.database.trim().is_empty()
        || file.management.token_file.trim().is_empty()
    {
        anyhow::bail!("authentication.database and management.token_file must not be empty");
    }
    let configured_credentials = ConfiguredCredentials::new(
        file.clients
            .into_iter()
            .map(|client| ConfiguredClient {
                id: client.id,
                enabled: client.enabled,
                subdomain_policy: SubdomainPolicy {
                    rules: client.subdomains,
                },
                tokens: client
                    .tokens
                    .into_iter()
                    .map(|token| ConfiguredToken {
                        name: token.name,
                        label: token.label,
                        secret: token.token,
                        expires_at: token.expires_at,
                    })
                    .collect(),
            })
            .collect(),
    )?;
    Ok(ValidatedServerConfig(ServerConfig {
        domain: file.server.domain,
        token: None,
        auth_db: Some(file.authentication.database),
        configured_credentials,
        management_listen: file.management.listen,
        management_token_file: Some(file.management.token_file),
        management_requests_per_minute: file.management.requests_per_minute,
        management_body_limit_bytes: file.management.body_limit_bytes,
        control_listen: file.server.control_listen,
        http_listen: file.server.http_listen,
        tcp_port_range: file.server.tcp_port_range,
        http_public_scheme: file.server.http_public_scheme,
        http_public_port: file.server.http_public_port,
    }))
}

fn validate_permissions(file: &File) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.permissions().mode() & 0o777 != 0o600 {
            anyhow::bail!("server config must be a regular file with mode 0600");
        }
    }
    Ok(())
}

fn default_requests_per_minute() -> u32 {
    120
}

fn default_body_limit_bytes() -> usize {
    16 * 1024
}

fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn parses_complete_config_without_version() {
        let path = std::env::temp_dir().join(format!("rproxy-config-{}.toml", Uuid::new_v4()));
        fs::write(
            &path,
            r#"
[server]
domain = "example.com"
control_listen = "127.0.0.1:7000"
http_listen = "0.0.0.0:8080"
tcp_port_range = "20000-30000"
http_public_scheme = "https"

[authentication]
database = "/tmp/auth.db"

[management]
listen = "127.0.0.1:7001"
token_file = "/tmp/management-token"

[[clients]]
id = "build-agent"
subdomains = ["preview-*", "docs"]

[[clients.tokens]]
name = "primary"
token = "secret"
"#,
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        }

        let config = load(&path).unwrap().into_server_config();
        assert_eq!(config.domain, "example.com");
        assert!(config
            .configured_credentials
            .authenticate("secret")
            .is_some());
        let _ = fs::remove_file(path);
    }

    #[test]
    fn rejects_clients_only_and_versioned_formats() {
        assert!(toml::from_str::<ServerConfigFile>(
            r#"
[[clients]]
id = "legacy"
subdomains = ["*"]
"#,
        )
        .is_err());
        assert!(toml::from_str::<ServerConfigFile>(
            r#"
version = 1
[server]
domain = "example.com"
[authentication]
database = "/tmp/auth.db"
[management]
token_file = "/tmp/token"
"#,
        )
        .is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_insecure_config_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let path = std::env::temp_dir().join(format!("rproxy-config-{}.toml", Uuid::new_v4()));
        fs::write(&path, "").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let error = load(&path).unwrap_err();
        assert!(error.to_string().contains("mode 0600"));
        let _ = fs::remove_file(path);
    }
}

use crate::config::{normalize_mac, OpenWrtConfig};
use serde_json::Value;
use std::collections::HashSet;
use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpenWrtError {
    #[error("ssh command failed: {command}: {stderr}")]
    CommandFailed { command: String, stderr: String },
    #[error("failed to execute ssh command {command}: {source}")]
    Execute {
        command: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid get_clients JSON for {object}: {source}")]
    Json {
        object: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("get_clients JSON for {object} does not contain a clients object")]
    MissingClients { object: String },
    #[error("invalid ssh target component: {0:?}")]
    InvalidSshTarget(String),
}

pub struct OpenWrtClient {
    config: OpenWrtConfig,
}

impl OpenWrtClient {
    pub fn new(config: OpenWrtConfig) -> Self {
        Self { config }
    }

    pub fn connected_macs(&self) -> Result<HashSet<String>, OpenWrtError> {
        let objects_output = self.run_remote("ubus list")?;
        let objects = discover_hostapd_objects(&objects_output);
        let mut all_clients = HashSet::new();

        for object in objects {
            let output = self.run_remote(&format!("ubus call hostapd.{object} get_clients"))?;
            all_clients.extend(parse_clients_json(&object, &output)?);
        }

        Ok(all_clients)
    }

    fn run_remote(&self, remote_command: &str) -> Result<String, OpenWrtError> {
        validate_ssh_target_part(&self.config.user)?;
        validate_ssh_target_part(&self.config.host)?;

        let target = format!("{}@{}", self.config.user, self.config.host);
        let display_command =
            format!("ssh -o BatchMode=yes -o ConnectTimeout=5 {target} {remote_command}");
        let output = Command::new("ssh")
            .arg("-o")
            .arg("BatchMode=yes")
            .arg("-o")
            .arg("ConnectTimeout=5")
            .arg(&target)
            .arg(remote_command)
            .output()
            .map_err(|source| OpenWrtError::Execute {
                command: display_command.clone(),
                source,
            })?;

        if !output.status.success() {
            return Err(OpenWrtError::CommandFailed {
                command: display_command,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

pub fn discover_hostapd_objects(input: &str) -> Vec<String> {
    input
        .lines()
        .filter_map(|line| line.trim().strip_prefix("hostapd."))
        .filter(|name| is_safe_ubus_object_suffix(name))
        .map(ToOwned::to_owned)
        .collect()
}

fn is_safe_ubus_object_suffix(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn validate_ssh_target_part(value: &str) -> Result<(), OpenWrtError> {
    if value.is_empty()
        || value.starts_with('-')
        || value.contains('@')
        || value
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
    {
        return Err(OpenWrtError::InvalidSshTarget(value.to_string()));
    }

    Ok(())
}

pub fn parse_clients_json(object: &str, input: &str) -> Result<HashSet<String>, OpenWrtError> {
    let value: Value = serde_json::from_str(input).map_err(|source| OpenWrtError::Json {
        object: object.to_string(),
        source,
    })?;
    let clients = value
        .get("clients")
        .and_then(Value::as_object)
        .ok_or_else(|| OpenWrtError::MissingClients {
            object: object.to_string(),
        })?;

    Ok(clients
        .keys()
        .filter_map(|mac| normalize_mac(mac))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hostapd_objects_from_ubus_list() {
        let input = "network.interface\nhostapd.phy0-ap0\nhostapd.phy1-ap0\nservice\n";
        assert_eq!(
            discover_hostapd_objects(input),
            vec!["phy0-ap0", "phy1-ap0"]
        );
    }

    #[test]
    fn ignores_unsafe_hostapd_object_names() {
        let input = "hostapd.phy0-ap0\nhostapd.phy0-ap0;reboot\nhostapd.phy1.ap0\n";

        assert_eq!(
            discover_hostapd_objects(input),
            vec!["phy0-ap0", "phy1.ap0"]
        );
    }

    #[test]
    fn parses_client_macs_from_get_clients_json() {
        let input = r#"
{
  "freq": 5260,
  "clients": {
    "88:B9:51:EB:45:43": { "authorized": true },
    "1e:7e:02:7d:d2:1a": { "authorized": true }
  }
}
"#;

        let clients = parse_clients_json("phy0-ap0", input).expect("valid clients");

        assert!(clients.contains("88:b9:51:eb:45:43"));
        assert!(clients.contains("1e:7e:02:7d:d2:1a"));
    }

    #[test]
    fn rejects_invalid_get_clients_json() {
        let err = parse_clients_json("phy0-ap0", "not json").expect_err("invalid json");

        assert!(err
            .to_string()
            .contains("invalid get_clients JSON for phy0-ap0"));
    }

    #[test]
    fn rejects_get_clients_json_without_clients_object() {
        let err =
            parse_clients_json("phy0-ap0", r#"{"freq":5260}"#).expect_err("missing clients object");

        assert_eq!(
            err.to_string(),
            "get_clients JSON for phy0-ap0 does not contain a clients object"
        );
    }

    #[test]
    fn rejects_invalid_ssh_target_parts() {
        assert!(validate_ssh_target_part("root").is_ok());
        assert!(validate_ssh_target_part("op.example").is_ok());

        for invalid in ["", "-root", "op host", "op\n", "victim@attacker"] {
            let err = validate_ssh_target_part(invalid).expect_err("invalid target part");
            assert_eq!(
                err.to_string(),
                format!("invalid ssh target component: {invalid:?}")
            );
        }
    }
}

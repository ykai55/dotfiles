use serde::Deserialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse config: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to parse config {path}: {source}")]
    ParseFile {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("invalid MAC address for device {device}: {mac}")]
    InvalidMac { device: String, mac: String },
    #[error("scan_interval_secs must be greater than zero")]
    InvalidScanInterval,
    #[error("away_delay_secs must be greater than zero for device {0}")]
    InvalidAwayDelay(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub scan_interval_secs: u64,
    pub openwrt: OpenWrtConfig,
    pub mqtt: MqttConfig,
    pub devices: Vec<DeviceConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OpenWrtConfig {
    pub host: String,
    pub user: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceConfig {
    pub name: String,
    pub mac: String,
    pub away_delay_secs: u64,
    pub topic: String,
    pub payload_home: String,
    #[serde(default)]
    pub payload_pending_away: Option<String>,
    pub payload_away: String,
    #[serde(default)]
    pub retain: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct MessageConfig {
    pub topic: String,
    pub payload: String,
    #[serde(default)]
    pub retain: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let contents = fs::read_to_string(path).map_err(|source| ConfigError::Read {
            path: path.display().to_string(),
            source,
        })?;
        let mut config =
            toml::from_str::<Self>(&contents).map_err(|source| ConfigError::ParseFile {
                path: path.display().to_string(),
                source,
            })?;
        config.validate()?;
        Ok(config)
    }

    pub fn from_toml_str(input: &str) -> Result<Self, ConfigError> {
        let mut config = toml::from_str::<Self>(input)?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&mut self) -> Result<(), ConfigError> {
        if self.scan_interval_secs == 0 {
            return Err(ConfigError::InvalidScanInterval);
        }

        for device in &mut self.devices {
            if device.away_delay_secs == 0 {
                return Err(ConfigError::InvalidAwayDelay(device.name.clone()));
            }

            let mac = device.mac.clone();
            device.mac = normalize_mac(&mac).ok_or_else(|| ConfigError::InvalidMac {
                device: device.name.clone(),
                mac,
            })?;
        }

        Ok(())
    }
}

pub fn normalize_mac(input: &str) -> Option<String> {
    let separator = if input.contains(':') {
        ':'
    } else if input.contains('-') {
        '-'
    } else {
        return None;
    };

    let parts: Vec<&str> = input.split(separator).collect();
    if parts.len() != 6
        || parts
            .iter()
            .any(|part| part.len() != 2 || !part.chars().all(|ch| ch.is_ascii_hexdigit()))
    {
        return None;
    }

    Some(
        parts
            .iter()
            .map(|part| part.to_ascii_lowercase())
            .collect::<Vec<String>>()
            .join(":"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn valid_config() -> &'static str {
        r#"
scan_interval_secs = 10

[openwrt]
host = "op"
user = "root"

[mqtt]
host = "127.0.0.1"
port = 1883
client_id = "ha_helper"

[[devices]]
name = "xiaomi17"
mac = "88-B9-51-EB-45-43"
away_delay_secs = 120
topic = "home/presence/xiaomi17"
payload_home = "home"
payload_pending_away = "pending_away"
payload_away = "away"
retain = true
"#
    }

    #[test]
    fn parses_config_and_normalizes_device_mac() {
        let config = Config::from_toml_str(valid_config()).expect("valid config");

        assert_eq!(config.scan_interval_secs, 10);
        assert_eq!(config.openwrt.host, "op");
        assert_eq!(config.openwrt.user, "root");
        assert_eq!(config.mqtt.port, 1883);
        assert_eq!(config.devices[0].mac, "88:b9:51:eb:45:43");
        assert_eq!(config.devices[0].topic, "home/presence/xiaomi17");
        assert_eq!(config.devices[0].payload_home, "home");
        assert_eq!(
            config.devices[0].payload_pending_away.as_deref(),
            Some("pending_away")
        );
        assert_eq!(config.devices[0].payload_away, "away");
        assert!(config.devices[0].retain);
    }

    #[test]
    fn rejects_invalid_mac() {
        let input = valid_config().replace("88-B9-51-EB-45-43", "not-a-mac");

        let err = Config::from_toml_str(&input).expect_err("invalid mac should fail");

        assert!(err.to_string().contains("invalid MAC address"));
    }

    #[test]
    fn defaults_mqtt_credentials_pending_away_payload_and_retain() {
        let input = valid_config()
            .replace("payload_pending_away = \"pending_away\"\n", "")
            .replace("retain = true\n", "");

        let config = Config::from_toml_str(&input).expect("valid config");

        assert_eq!(config.mqtt.username, "");
        assert_eq!(config.mqtt.password, "");
        assert_eq!(config.devices[0].payload_pending_away, None);
        assert!(!config.devices[0].retain);
    }

    #[test]
    fn rejects_zero_scan_interval() {
        let input = valid_config().replace("scan_interval_secs = 10", "scan_interval_secs = 0");

        let err = Config::from_toml_str(&input).expect_err("zero scan interval should fail");

        assert_eq!(
            err.to_string(),
            "scan_interval_secs must be greater than zero"
        );
    }

    #[test]
    fn rejects_zero_away_delay() {
        let input = valid_config().replace("away_delay_secs = 120", "away_delay_secs = 0");

        let err = Config::from_toml_str(&input).expect_err("zero away delay should fail");

        assert_eq!(
            err.to_string(),
            "away_delay_secs must be greater than zero for device xiaomi17"
        );
    }

    #[test]
    fn loads_config_from_file() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("config.toml");
        fs::write(&path, valid_config()).expect("write config");

        let config = Config::load(&path).expect("valid config file");

        assert_eq!(config.devices[0].name, "xiaomi17");
    }

    #[test]
    fn load_parse_error_includes_file_path() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("bad-config.toml");
        fs::write(&path, "not valid toml =").expect("write config");

        let err = Config::load(&path).expect_err("invalid config should fail");
        let message = err.to_string();

        assert!(message.contains("failed to parse config"));
        assert!(message.contains(&path.display().to_string()));
    }

    #[test]
    fn normalizes_colon_and_dash_separated_mac_addresses() {
        assert_eq!(
            normalize_mac("88-B9-51-EB-45-43"),
            Some("88:b9:51:eb:45:43".to_string())
        );
        assert_eq!(
            normalize_mac("88:b9:51:EB:45:43"),
            Some("88:b9:51:eb:45:43".to_string())
        );
        assert_eq!(normalize_mac("not-a-mac"), None);
    }
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ClientHelloMode {
    Control { service: ServiceRequest },
    Data { connection_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHello {
    Control {
        token: String,
        service: ServiceRequest,
    },
    Data {
        token: String,
        connection_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServiceRequest {
    Http {
        local: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdomain: Option<String>,
    },
    Tcp {
        local: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        remote_port: Option<u16>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Registered {
        public: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdomain: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        remote_port: Option<u16>,
    },
    Open {
        connection_id: String,
    },
    Error {
        code: ServerErrorCode,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerErrorCode {
    AuthFailed,
    InvalidRequest,
    SubdomainUnavailable,
    PortUnavailable,
    PortNotAllowed,
    PortRangeExhausted,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClientHelloWire {
    #[serde(rename = "type")]
    message_type: String,
    token: String,
    #[serde(flatten)]
    mode: ClientHelloMode,
}

impl Serialize for ClientHello {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let wire = match self {
            ClientHello::Control { token, service } => ClientHelloWire {
                message_type: "hello".into(),
                token: token.clone(),
                mode: ClientHelloMode::Control {
                    service: service.clone(),
                },
            },
            ClientHello::Data {
                token,
                connection_id,
            } => ClientHelloWire {
                message_type: "hello".into(),
                token: token.clone(),
                mode: ClientHelloMode::Data {
                    connection_id: connection_id.clone(),
                },
            },
        };

        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ClientHello {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let wire = ClientHelloWire::deserialize(deserializer)?;
        if wire.message_type != "hello" {
            return Err(serde::de::Error::custom("expected hello message"));
        }

        Ok(match wire.mode {
            ClientHelloMode::Control { service } => ClientHello::Control {
                token: wire.token,
                service,
            },
            ClientHelloMode::Data { connection_id } => ClientHello::Data {
                token: wire.token,
                connection_id,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_http_control_hello() {
        let msg = ClientHello::Control {
            token: "secret".into(),
            service: ServiceRequest::Http {
                local: "127.0.0.1:3000".into(),
                subdomain: Some("foo".into()),
            },
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "hello");
        assert_eq!(json["mode"], "control");
        assert_eq!(json["service"]["kind"], "http");
        assert_eq!(json["service"]["subdomain"], "foo");
    }

    #[test]
    fn parses_data_hello() {
        let msg: ClientHello = serde_json::from_str(
            r#"{"type":"hello","token":"secret","mode":"data","connection_id":"abc"}"#,
        )
        .unwrap();

        assert_eq!(
            msg,
            ClientHello::Data {
                token: "secret".into(),
                connection_id: "abc".into()
            }
        );
    }

    #[test]
    fn serializes_registered_tcp_message() {
        let msg = ServerMessage::Registered {
            public: "a.com:25432".into(),
            subdomain: None,
            remote_port: Some(25432),
        };

        let json = serde_json::to_value(&msg).unwrap();
        assert_eq!(json["type"], "registered");
        assert_eq!(json["public"], "a.com:25432");
        assert_eq!(json["remote_port"], 25432);
    }
}

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_DATA_SIZE: usize = 16 * 1024;
pub const INITIAL_CREDIT: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ClientHelloMode {
    Control { service: ServiceRequest },
    Data { session_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientHello {
    Control {
        token: String,
        service: ServiceRequest,
    },
    Data {
        token: String,
        session_id: String,
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
        session_id: String,
        public: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        subdomain: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        remote_port: Option<u16>,
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
    SubdomainNotAllowed,
    SubdomainUnavailable,
    PortUnavailable,
    PortNotAllowed,
    PortRangeExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataFrame {
    Open { stream_id: u32 },
    Ready { stream_id: u32 },
    Data { stream_id: u32, payload: Vec<u8> },
    Credit { stream_id: u32, amount: u32 },
    Fin { stream_id: u32 },
    Reset { stream_id: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CodecError {
    #[error("unknown data frame opcode {0}")]
    UnknownOpcode(u8),
    #[error("invalid data frame length")]
    InvalidLength,
    #[error("stream id must be non-zero")]
    ZeroStreamId,
    #[error("data payload exceeds 16 KiB")]
    DataTooLarge,
    #[error("credit must be between 1 and {INITIAL_CREDIT}")]
    InvalidCredit,
}

impl DataFrame {
    pub fn stream_id(&self) -> u32 {
        match self {
            Self::Open { stream_id }
            | Self::Ready { stream_id }
            | Self::Data { stream_id, .. }
            | Self::Credit { stream_id, .. }
            | Self::Fin { stream_id }
            | Self::Reset { stream_id } => *stream_id,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, CodecError> {
        let stream_id = self.stream_id();
        if stream_id == 0 {
            return Err(CodecError::ZeroStreamId);
        }
        let (opcode, payload): (u8, &[u8]) = match self {
            Self::Open { .. } => (1, &[]),
            Self::Ready { .. } => (2, &[]),
            Self::Data { payload, .. } => {
                if payload.is_empty() {
                    return Err(CodecError::InvalidLength);
                }
                if payload.len() > MAX_DATA_SIZE {
                    return Err(CodecError::DataTooLarge);
                }
                (3, payload)
            }
            Self::Credit { amount, .. } => {
                if !(1..=INITIAL_CREDIT).contains(amount) {
                    return Err(CodecError::InvalidCredit);
                }
                (4, &[])
            }
            Self::Fin { .. } => (5, &[]),
            Self::Reset { .. } => (6, &[]),
        };
        let mut encoded = Vec::with_capacity(9 + payload.len());
        encoded.push(opcode);
        encoded.extend_from_slice(&stream_id.to_be_bytes());
        if let Self::Credit { amount, .. } = self {
            encoded.extend_from_slice(&amount.to_be_bytes());
        } else {
            encoded.extend_from_slice(payload);
        }
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, CodecError> {
        if encoded.len() < 5 {
            return Err(CodecError::InvalidLength);
        }
        let opcode = encoded[0];
        let stream_id = u32::from_be_bytes(encoded[1..5].try_into().unwrap());
        if stream_id == 0 {
            return Err(CodecError::ZeroStreamId);
        }
        let payload = &encoded[5..];
        match opcode {
            1 if payload.is_empty() => Ok(Self::Open { stream_id }),
            2 if payload.is_empty() => Ok(Self::Ready { stream_id }),
            3 if !payload.is_empty() && payload.len() <= MAX_DATA_SIZE => Ok(Self::Data {
                stream_id,
                payload: payload.to_vec(),
            }),
            3 if payload.is_empty() => Err(CodecError::InvalidLength),
            3 => Err(CodecError::DataTooLarge),
            4 if payload.len() == 4 => {
                let amount = u32::from_be_bytes(payload.try_into().unwrap());
                if !(1..=INITIAL_CREDIT).contains(&amount) {
                    Err(CodecError::InvalidCredit)
                } else {
                    Ok(Self::Credit { stream_id, amount })
                }
            }
            5 if payload.is_empty() => Ok(Self::Fin { stream_id }),
            6 if payload.is_empty() => Ok(Self::Reset { stream_id }),
            1..=6 => Err(CodecError::InvalidLength),
            _ => Err(CodecError::UnknownOpcode(opcode)),
        }
    }
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
            ClientHello::Data { token, session_id } => ClientHelloWire {
                message_type: "hello".into(),
                token: token.clone(),
                mode: ClientHelloMode::Data {
                    session_id: session_id.clone(),
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
            ClientHelloMode::Control { service } => Self::Control {
                token: wire.token,
                service,
            },
            ClientHelloMode::Data { session_id } => Self::Data {
                token: wire.token,
                session_id,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_every_data_frame() {
        for frame in [
            DataFrame::Open { stream_id: 1 },
            DataFrame::Ready { stream_id: 2 },
            DataFrame::Data {
                stream_id: 3,
                payload: b"data".to_vec(),
            },
            DataFrame::Credit {
                stream_id: 4,
                amount: 1,
            },
            DataFrame::Fin { stream_id: 5 },
            DataFrame::Reset { stream_id: 6 },
        ] {
            assert_eq!(DataFrame::decode(&frame.encode().unwrap()).unwrap(), frame);
        }
    }

    #[test]
    fn strictly_rejects_invalid_data_frames() {
        assert_eq!(DataFrame::decode(&[]), Err(CodecError::InvalidLength));
        assert_eq!(
            DataFrame::decode(&[99, 0, 0, 0, 1]),
            Err(CodecError::UnknownOpcode(99))
        );
        assert_eq!(
            DataFrame::decode(&[1, 0, 0, 0, 0]),
            Err(CodecError::ZeroStreamId)
        );
        assert_eq!(
            DataFrame::decode(&[1, 0, 0, 0, 1, 0]),
            Err(CodecError::InvalidLength)
        );
        assert_eq!(
            DataFrame::decode(&[3, 0, 0, 0, 1]),
            Err(CodecError::InvalidLength)
        );
        let mut oversized = vec![3, 0, 0, 0, 1];
        oversized.resize(5 + MAX_DATA_SIZE + 1, 0);
        assert_eq!(DataFrame::decode(&oversized), Err(CodecError::DataTooLarge));
        assert_eq!(
            DataFrame::decode(&[4, 0, 0, 0, 1, 0, 0, 0, 0]),
            Err(CodecError::InvalidCredit)
        );
        assert_eq!(
            DataFrame::decode(&[4, 0, 0, 0, 1, 0, 0, 0, 9]),
            Err(CodecError::InvalidCredit)
        );
    }

    #[test]
    fn parses_session_data_hello() {
        let msg: ClientHello = serde_json::from_str(
            r#"{"type":"hello","token":"secret","mode":"data","session_id":"abc"}"#,
        )
        .unwrap();
        assert_eq!(
            msg,
            ClientHello::Data {
                token: "secret".into(),
                session_id: "abc".into()
            }
        );
    }

    #[test]
    fn registered_message_contains_session_id() {
        let json = serde_json::to_value(ServerMessage::Registered {
            session_id: "session-1".into(),
            public: "a.com:25432".into(),
            subdomain: None,
            remote_port: Some(25432),
        })
        .unwrap();
        assert_eq!(json["session_id"], "session-1");
    }
}

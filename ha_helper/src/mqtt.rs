use crate::config::MqttConfig;
use crate::presence::PresenceEvent;
use rumqttc::{Client, Connection, Event, MqttOptions, Outgoing, QoS, RecvTimeoutError};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MqttError {
    #[error("MQTT connection failed: {0}")]
    Connection(String),
    #[error("failed to publish MQTT message: {0}")]
    Publish(#[from] rumqttc::ClientError),
}

pub struct MqttPublisher {
    config: MqttConfig,
    client: Client,
    connection: Connection,
}

impl MqttPublisher {
    pub fn new(config: &MqttConfig) -> Self {
        let (client, connection) = build_client(config);

        Self {
            config: config.clone(),
            client,
            connection,
        }
    }

    pub fn publish(&mut self, event: &PresenceEvent) -> Result<(), MqttError> {
        let message = event.message.as_ref().ok_or_else(|| {
            MqttError::Connection(format!(
                "no MQTT message configured for {} {:?}",
                event.device_name, event.state
            ))
        })?;

        self.client.publish(
            &message.topic,
            QoS::AtLeastOnce,
            message.retain,
            message.payload.as_bytes(),
        )?;
        let result = wait_for_publish_written(&mut self.connection);
        if should_reset_after_publish_wait(&result) {
            self.reset_connection();
        }
        result
    }

    fn reset_connection(&mut self) {
        let (client, connection) = build_client(&self.config);
        self.client = client;
        self.connection = connection;
    }
}

fn build_client(config: &MqttConfig) -> (Client, Connection) {
    let mut options = MqttOptions::new(&config.client_id, &config.host, config.port);
    options.set_keep_alive(Duration::from_secs(30));
    if !config.username.is_empty() || !config.password.is_empty() {
        options.set_credentials(&config.username, &config.password);
    }

    Client::new(options, 10)
}

fn wait_for_publish_written(connection: &mut Connection) -> Result<(), MqttError> {
    for _ in 0..100 {
        match connection.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(event)) if is_publish_written_event(&event) => return Ok(()),
            Ok(Ok(_)) => {}
            Ok(Err(err)) => return Err(MqttError::Connection(err.to_string())),
            Err(RecvTimeoutError::Timeout) => {
                return Err(MqttError::Connection(
                    "timed out waiting for MQTT publish".to_string(),
                ));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(MqttError::Connection(
                    "MQTT connection disconnected".to_string(),
                ));
            }
        }
    }

    Err(MqttError::Connection(
        "timed out waiting for MQTT publish".to_string(),
    ))
}

fn is_publish_written_event(event: &Event) -> bool {
    matches!(event, Event::Outgoing(Outgoing::Publish(_)))
}

fn should_reset_after_publish_wait(result: &Result<(), MqttError>) -> bool {
    matches!(result, Err(MqttError::Connection(_)))
}

pub fn format_dry_run(event: &PresenceEvent) -> String {
    match &event.message {
        Some(message) => format!(
            "dry-run: {} {:?} -> {} {} retain={}",
            event.device_name, event.state, message.topic, message.payload, message.retain
        ),
        None => format!(
            "presence changed: {} {:?} (mqtt skipped: no payload_pending_away)",
            event.device_name, event.state
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MessageConfig;
    use crate::presence::PresenceState;
    use rumqttc::{Event, Outgoing};

    #[test]
    fn formats_dry_run_event() {
        let event = PresenceEvent {
            device_name: "xiaomi17".to_string(),
            state: PresenceState::Home,
            message: Some(MessageConfig {
                topic: "home/presence/xiaomi17".to_string(),
                payload: "home".to_string(),
                retain: true,
            }),
        };

        assert_eq!(
            format_dry_run(&event),
            "dry-run: xiaomi17 Home -> home/presence/xiaomi17 home retain=true"
        );
    }

    #[test]
    fn formats_dry_run_skipped_pending_away_event() {
        let event = PresenceEvent {
            device_name: "xiaomi17".to_string(),
            state: PresenceState::PendingAway,
            message: None,
        };

        assert_eq!(
            format_dry_run(&event),
            "presence changed: xiaomi17 PendingAway (mqtt skipped: no payload_pending_away)"
        );
    }

    #[test]
    fn detects_publish_written_event() {
        assert!(is_publish_written_event(&Event::Outgoing(
            Outgoing::Publish(1)
        )));
        assert!(!is_publish_written_event(&Event::Outgoing(
            Outgoing::PingReq
        )));
    }

    #[test]
    fn resets_after_connection_class_publish_wait_failure() {
        let result = Err(MqttError::Connection(
            "timed out waiting for MQTT publish".to_string(),
        ));

        assert!(should_reset_after_publish_wait(&result));
    }

    #[test]
    fn does_not_reset_after_successful_publish_wait() {
        let result = Ok(());

        assert!(!should_reset_after_publish_wait(&result));
    }
}

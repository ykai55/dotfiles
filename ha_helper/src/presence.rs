use crate::config::{DeviceConfig, MessageConfig};
use std::collections::HashSet;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresenceState {
    Home,
    PendingAway,
    Away,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PresenceEvent {
    pub device_name: String,
    pub state: PresenceState,
    pub message: Option<MessageConfig>,
}

#[derive(Debug, Clone)]
struct DeviceRuntime {
    config: DeviceConfig,
    state: PresenceState,
    missing_since: Option<Instant>,
    initialized: bool,
}

pub struct PresenceTracker {
    devices: Vec<DeviceRuntime>,
}

impl PresenceTracker {
    pub fn new(devices: Vec<DeviceConfig>) -> Self {
        let devices = devices
            .into_iter()
            .map(|config| DeviceRuntime {
                config,
                state: PresenceState::Away,
                missing_since: None,
                initialized: false,
            })
            .collect();

        Self { devices }
    }

    pub fn evaluate(&mut self, online_macs: &HashSet<String>, now: Instant) -> Vec<PresenceEvent> {
        let mut events = Vec::new();

        for runtime in &mut self.devices {
            let present = online_macs.contains(&runtime.config.mac);
            if present {
                runtime.missing_since = None;
                if !runtime.initialized || runtime.state != PresenceState::Home {
                    runtime.initialized = true;
                    runtime.state = PresenceState::Home;
                    events.push(event(runtime, PresenceState::Home));
                }
                continue;
            }

            if !runtime.initialized {
                runtime.initialized = true;
                runtime.state = PresenceState::Away;
                continue;
            }

            match runtime.state {
                PresenceState::Home => {
                    runtime.state = PresenceState::PendingAway;
                    runtime.missing_since = Some(now);
                    events.push(event(runtime, PresenceState::PendingAway));
                }
                PresenceState::PendingAway => {
                    let missing_since = runtime.missing_since.unwrap_or(now);
                    if now.duration_since(missing_since)
                        >= Duration::from_secs(runtime.config.away_delay_secs)
                    {
                        runtime.state = PresenceState::Away;
                        runtime.missing_since = None;
                        events.push(event(runtime, PresenceState::Away));
                    }
                }
                PresenceState::Away => {}
            }
        }

        events
    }
}

fn event(runtime: &DeviceRuntime, state: PresenceState) -> PresenceEvent {
    let payload = match state {
        PresenceState::Home => Some(runtime.config.payload_home.clone()),
        PresenceState::PendingAway => runtime.config.payload_pending_away.clone(),
        PresenceState::Away => Some(runtime.config.payload_away.clone()),
    };

    PresenceEvent {
        device_name: runtime.config.name.clone(),
        state,
        message: payload.map(|payload| MessageConfig {
            topic: runtime.config.topic.clone(),
            payload,
            retain: runtime.config.retain,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> DeviceConfig {
        DeviceConfig {
            name: "xiaomi17".to_string(),
            mac: "88:b9:51:eb:45:43".to_string(),
            away_delay_secs: 120,
            topic: "t".to_string(),
            payload_home: "home".to_string(),
            payload_pending_away: Some("pending_away".to_string()),
            payload_away: "away".to_string(),
            retain: true,
        }
    }

    fn named_device(name: &str, mac: &str) -> DeviceConfig {
        DeviceConfig {
            name: name.to_string(),
            mac: mac.to_string(),
            away_delay_secs: 120,
            topic: "t".to_string(),
            payload_home: "home".to_string(),
            payload_pending_away: Some("pending_away".to_string()),
            payload_away: "away".to_string(),
            retain: true,
        }
    }

    #[test]
    fn publishes_home_when_device_first_appears() {
        let now = Instant::now();
        let mut tracker = PresenceTracker::new(vec![device()]);
        let online = HashSet::from(["88:b9:51:eb:45:43".to_string()]);

        let events = tracker.evaluate(&online, now);

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].device_name, "xiaomi17");
        assert_eq!(events[0].state, PresenceState::Home);
        assert_eq!(events[0].message.as_ref().unwrap().payload, "home");
    }

    #[test]
    fn publishes_events_in_config_order() {
        let now = Instant::now();
        let devices: Vec<DeviceConfig> = (1..=20)
            .map(|index| {
                named_device(
                    &format!("device-{index:02}"),
                    &format!("00:00:00:00:00:{index:02x}"),
                )
            })
            .collect();
        let online: HashSet<String> = devices.iter().map(|device| device.mac.clone()).collect();
        let expected: Vec<String> = devices.iter().map(|device| device.name.clone()).collect();
        let mut tracker = PresenceTracker::new(devices);

        let events = tracker.evaluate(&online, now);

        assert_eq!(events.len(), expected.len());
        assert_eq!(
            events
                .iter()
                .map(|event| event.device_name.clone())
                .collect::<Vec<String>>(),
            expected
        );
    }

    #[test]
    fn goes_pending_then_away_after_delay() {
        let now = Instant::now();
        let mut tracker = PresenceTracker::new(vec![device()]);
        let online = HashSet::from(["88:b9:51:eb:45:43".to_string()]);
        tracker.evaluate(&online, now);

        let missing = HashSet::new();
        let pending = tracker.evaluate(&missing, now + Duration::from_secs(10));
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].state, PresenceState::PendingAway);
        assert_eq!(pending[0].message.as_ref().unwrap().payload, "pending_away");

        let before_timeout = tracker.evaluate(&missing, now + Duration::from_secs(100));
        assert!(before_timeout.is_empty());

        let away = tracker.evaluate(&missing, now + Duration::from_secs(131));
        assert_eq!(away.len(), 1);
        assert_eq!(away[0].state, PresenceState::Away);
        assert_eq!(away[0].message.as_ref().unwrap().payload, "away");
    }

    #[test]
    fn reconnect_during_pending_cancels_away() {
        let now = Instant::now();
        let mut tracker = PresenceTracker::new(vec![device()]);
        let online = HashSet::from(["88:b9:51:eb:45:43".to_string()]);
        tracker.evaluate(&online, now);
        tracker.evaluate(&HashSet::new(), now + Duration::from_secs(10));

        let events = tracker.evaluate(&online, now + Duration::from_secs(30));

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, PresenceState::Home);
        assert_eq!(events[0].message.as_ref().unwrap().payload, "home");

        let missing_after_delay = tracker.evaluate(&HashSet::new(), now + Duration::from_secs(200));
        assert_eq!(missing_after_delay.len(), 1);
        assert_eq!(missing_after_delay[0].state, PresenceState::PendingAway);
    }

    #[test]
    fn pending_away_without_payload_does_not_emit_event() {
        let now = Instant::now();
        let mut device = device();
        device.payload_pending_away = None;
        let mut tracker = PresenceTracker::new(vec![device]);
        let online = HashSet::from(["88:b9:51:eb:45:43".to_string()]);
        tracker.evaluate(&online, now);

        let events = tracker.evaluate(&HashSet::new(), now + Duration::from_secs(10));

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, PresenceState::PendingAway);
        assert!(events[0].message.is_none());

        let away = tracker.evaluate(&HashSet::new(), now + Duration::from_secs(131));
        assert_eq!(away.len(), 1);
        assert_eq!(away[0].state, PresenceState::Away);
        assert_eq!(away[0].message.as_ref().unwrap().payload, "away");
    }

    #[test]
    fn first_scan_missing_initializes_away_silently() {
        let now = Instant::now();
        let mut tracker = PresenceTracker::new(vec![device()]);

        let events = tracker.evaluate(&HashSet::new(), now);

        assert!(events.is_empty());
    }

    #[test]
    fn suppresses_duplicate_home_events() {
        let now = Instant::now();
        let mut tracker = PresenceTracker::new(vec![device()]);
        let online = HashSet::from(["88:b9:51:eb:45:43".to_string()]);

        tracker.evaluate(&online, now);
        let events = tracker.evaluate(&online, now + Duration::from_secs(10));

        assert!(events.is_empty());
    }
}

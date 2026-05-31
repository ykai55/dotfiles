use clap::Parser;
use ha_helper::config::Config;
use ha_helper::mqtt::{format_dry_run, MqttPublisher};
use ha_helper::openwrt::OpenWrtClient;
use ha_helper::presence::{PresenceEvent, PresenceTracker};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Parser)]
#[command(name = "ha_helper")]
#[command(about = "Publish MQTT presence messages from OpenWrt WiFi clients")]
struct Args {
    #[arg(long)]
    config: PathBuf,

    #[arg(long)]
    once: bool,

    #[arg(long)]
    dry_run: bool,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let config = Config::load(&args.config)?;
    let openwrt = OpenWrtClient::new(config.openwrt.clone());
    let mut tracker = PresenceTracker::new(config.devices.clone());
    let mut publisher = if args.dry_run {
        None
    } else {
        Some(MqttPublisher::new(&config.mqtt))
    };
    let mut pending_events = Vec::new();

    loop {
        let online_macs = openwrt.connected_macs()?;
        println!("scan: online_macs={}", online_macs.len());
        let events = tracker.evaluate(&online_macs, Instant::now());

        if args.dry_run {
            for event in events {
                println!("{}", format_dry_run(&event));
            }
        } else {
            for event in events {
                println!("{}", format_presence_log(&event));
                queue_pending_event(&mut pending_events, event);
            }

            if let Some(publisher) = &mut publisher {
                let events_to_publish = std::mem::take(&mut pending_events);
                for event in events_to_publish {
                    println!("{}", format_mqtt_publish_log(&event));
                    if let Err(err) = publisher.publish(&event) {
                        eprintln!("mqtt publish failed for {}: {err}", event.device_name);
                        queue_pending_event(&mut pending_events, event);
                    } else {
                        println!("mqtt published: {} {:?}", event.device_name, event.state);
                    }
                }
            }
        }

        if args.once {
            break;
        }

        thread::sleep(Duration::from_secs(config.scan_interval_secs));
    }

    Ok(())
}

#[cfg(test)]
fn queue_pending_events(pending: &mut Vec<PresenceEvent>, events: Vec<PresenceEvent>) {
    for event in events {
        queue_pending_event(pending, event);
    }
}

fn queue_pending_event(pending: &mut Vec<PresenceEvent>, event: PresenceEvent) {
    if event.message.is_none() {
        return;
    }

    if let Some(existing) = pending
        .iter_mut()
        .find(|pending_event| pending_event.device_name == event.device_name)
    {
        *existing = event;
    } else {
        pending.push(event);
    }
}

fn format_presence_log(event: &PresenceEvent) -> String {
    match &event.message {
        Some(message) => format!(
            "presence changed: {} {:?} -> {} {} retain={}",
            event.device_name, event.state, message.topic, message.payload, message.retain
        ),
        None => format!(
            "presence changed: {} {:?} (mqtt skipped: no payload_pending_away)",
            event.device_name, event.state
        ),
    }
}

fn format_mqtt_publish_log(event: &PresenceEvent) -> String {
    let message = event
        .message
        .as_ref()
        .expect("queued MQTT events always have a message");
    format!(
        "mqtt publish: {} {:?} -> {} {} retain={}",
        event.device_name, event.state, message.topic, message.payload, message.retain
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;
    use ha_helper::config::MessageConfig;
    use ha_helper::presence::PresenceState;

    #[test]
    fn help_includes_runtime_flags() {
        let help = Args::command().render_help().to_string();

        assert!(help.contains("--config"));
        assert!(help.contains("--once"));
        assert!(help.contains("--dry-run"));
    }

    #[test]
    fn queue_pending_event_replaces_existing_device_event() {
        let mut pending = vec![event(
            "xiaomi17",
            PresenceState::PendingAway,
            "pending_away",
        )];

        queue_pending_event(&mut pending, event("xiaomi17", PresenceState::Away, "away"));

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].device_name, "xiaomi17");
        assert_eq!(pending[0].state, PresenceState::Away);
        assert_eq!(pending[0].message.as_ref().unwrap().payload, "away");
    }

    #[test]
    fn queue_pending_events_replaces_stale_away_with_fresh_home_before_publish() {
        let mut pending = vec![event("xiaomi17", PresenceState::Away, "away")];
        let fresh_events = vec![event("xiaomi17", PresenceState::Home, "home")];

        queue_pending_events(&mut pending, fresh_events);

        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].device_name, "xiaomi17");
        assert_eq!(pending[0].state, PresenceState::Home);
        assert_eq!(pending[0].message.as_ref().unwrap().payload, "home");
    }

    #[test]
    fn queue_pending_event_ignores_events_without_mqtt_message() {
        let mut pending = Vec::new();

        queue_pending_event(
            &mut pending,
            skipped_event("xiaomi17", PresenceState::PendingAway),
        );

        assert!(pending.is_empty());
    }

    #[test]
    fn formats_full_presence_and_publish_logs() {
        let event = event("xiaomi17", PresenceState::Home, "home");

        assert_eq!(
            format_presence_log(&event),
            "presence changed: xiaomi17 Home -> home/presence/xiaomi17 home retain=true"
        );
        assert_eq!(
            format_mqtt_publish_log(&event),
            "mqtt publish: xiaomi17 Home -> home/presence/xiaomi17 home retain=true"
        );
    }

    #[test]
    fn formats_skipped_pending_away_log() {
        let event = skipped_event("xiaomi17", PresenceState::PendingAway);

        assert_eq!(
            format_presence_log(&event),
            "presence changed: xiaomi17 PendingAway (mqtt skipped: no payload_pending_away)"
        );
    }

    fn event(name: &str, state: PresenceState, payload: &str) -> PresenceEvent {
        PresenceEvent {
            device_name: name.to_string(),
            state,
            message: Some(MessageConfig {
                topic: format!("home/presence/{name}"),
                payload: payload.to_string(),
                retain: true,
            }),
        }
    }

    fn skipped_event(name: &str, state: PresenceState) -> PresenceEvent {
        PresenceEvent {
            device_name: name.to_string(),
            state,
            message: None,
        }
    }
}

# AGENTS

This directory is an independent Rust crate for `ha_helper`, a small daemon that
monitors WiFi client presence from OpenWrt and publishes MQTT presence messages.

## Build / Test / Run

Run commands from the repository root unless noted otherwise.

Build and test:

- `cargo test --manifest-path ha_helper/Cargo.toml`
- `cargo run --manifest-path ha_helper/Cargo.toml -- --help`

Manual dry-run against the local OpenWrt test host:

- `cargo run --manifest-path ha_helper/Cargo.toml -- --config ha_helper/config.toml --once --dry-run`

Example dry-run config:

- `cargo run --manifest-path ha_helper/Cargo.toml -- --config ha_helper/examples/config.toml --once --dry-run`

After any Rust code change, run the full crate test command. After config shape
or OpenWrt collection changes, also run one `--once --dry-run` check when
`root@op` is reachable.

## Project Layout

- `src/config.rs`: TOML config structs, loading, validation, and MAC normalization.
- `src/openwrt.rs`: SSH-based OpenWrt collection, `hostapd.*` discovery, and client parsing.
- `src/presence.rs`: per-device `home`, `pending_away`, and `away` state machine.
- `src/mqtt.rs`: MQTT publishing and dry-run message formatting.
- `src/main.rs`: CLI flags, polling loop, retry queue, and runtime logging.
- `config.toml`: local runtime config. Treat values as private.
- `examples/config.toml`: example config for documentation and dry-run checks.

## Configuration Model

Use the current device-level MQTT config shape:

```toml
[[devices]]
name = "xiaomi17"
mac = "88:b9:51:eb:45:43"
away_delay_secs = 120
topic = "home/presence/yingkai"
payload_home = "home"
payload_away = "not_home"
payload_pending_away = "pending_away" # optional
retain = true
```

Rules:

- Each device has exactly one `topic`.
- `payload_home` and `payload_away` are required.
- `payload_pending_away` is optional. If omitted, the internal state still changes to `pending_away`, but no MQTT message is published for that state.
- `retain` is device-level and defaults to `false`.
- Device matching is by normalized MAC address.

## OpenWrt Behavior

- Do not require users to configure WiFi interface names.
- Discover AP objects with `ubus list` and entries matching `hostapd.*`.
- Query clients with `ubus call hostapd.<object> get_clients`.
- Keep object-name filtering strict before building remote shell commands.
- Keep SSH target validation strict. Reject unsafe user/host values rather than trying to quote them.

## Development Guidelines

- Prefer small, direct changes with focused tests.
- Add or update tests before changing behavior.
- Keep unit tests independent of real OpenWrt and real MQTT brokers.
- Use helper functions for testable formatting, queueing, parsing, and state transitions.
- Do not silently drop state changes. If a state change has no MQTT message, log that MQTT was skipped.
- Do not let MQTT publish failures permanently lose events; preserve retry behavior and stale-event replacement by device.
- Keep runtime logs useful and stable enough for manual debugging.

## Safety And Privacy

- `config.toml` can contain real MQTT credentials, MAC addresses, hostnames, and personal topics. Do not copy these into docs or examples unless explicitly requested.
- MAC addresses are stable identifiers. Use fake values in generic examples.
- Never commit `target/` or other build artifacts.
- Do not modify unrelated dotfiles or opencode skill files while working in this crate.

## Rust Style

- Keep using Rust 2021.
- Prefer standard library plus existing dependencies in `Cargo.toml`.
- Keep error messages explicit and user-facing where errors cross the CLI boundary.
- Preserve public behavior of CLI flags: `--config`, `--once`, and `--dry-run`.
- Run `cargo fmt --manifest-path ha_helper/Cargo.toml` after edits.

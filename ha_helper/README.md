# ha_helper

`ha_helper` monitors WiFi clients on OpenWrt and publishes MQTT presence messages for configured devices.

## OpenWrt Collection

The program connects to OpenWrt over SSH, runs `ubus list`, discovers every `hostapd.*` object, then runs `ubus call hostapd.<object> get_clients` for each object. Users do not need to configure WiFi interface names.

## Configuration

Use `examples/config.toml` as a starting point. Devices are matched by MAC address, such as the Xiaomi 17 Pro MAC `88:b9:51:eb:45:43` in the example config.

Privacy note: MAC addresses are stable device identifiers. Replace example MAC addresses with your own private values before sharing or publishing this repository.

Each device configures one MQTT `topic`, required `payload_home` and `payload_away` values, and an optional `payload_pending_away`. If `payload_pending_away` is omitted, the device still enters the internal `pending_away` state, but no MQTT message is published for that state. The device-level `retain` flag applies to all published messages and defaults to `false`.

## Test With root@op

Run one dry-run scan to verify the config parses and OpenWrt collection works:

```sh
cargo run --manifest-path ha_helper/Cargo.toml -- --config ha_helper/examples/config.toml --once --dry-run
```

If `root@op` is reachable, output includes `online_macs=` and any dry-run presence event for the configured Xiaomi 17 Pro when it is present. If `root@op` is unreachable, the command reports the SSH error.

## Run Continuously

Run continuously without publishing MQTT messages:

```sh
cargo run --manifest-path ha_helper/Cargo.toml -- --config ha_helper/examples/config.toml --dry-run
```

Remove `--dry-run` to publish real MQTT messages.

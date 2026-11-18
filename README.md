# ESP RGB LED room over MQTT in `no_std` Rust

Assignment 2 of the [Getting started with esp-hal](https://developer.espressif.com/workshops/rust-esp-hal-intro/)
workshop. Boards join a "room" on a public MQTT broker, show up on a web page, and anyone in the room
can set their RGB LED from the page.

Written in `no_std` Rust with [esp-hal](https://github.com/esp-rs/esp-hal),
[esp-radio](https://github.com/esp-rs/esp-hal/tree/main/esp-radio), [Embassy](https://embassy.dev),
[embassy-net](https://docs.embassy.dev/embassy-net/) and [rust-mqtt](https://crates.io/crates/rust-mqtt).
The Wi-Fi and MQTT parts follow the
[Embedded Rust Training for ESP32-C3](https://docs.espressif.com/projects/rust/no_std-training/application/index.html).

| Folder | What it is |
|---|---|
| [`firmware/`](firmware/) | The finished firmware, for all workshop boards |
| [`web/`](web/) | The web page (published to GitHub Pages) |
| [`broker/`](broker/) | Offline backup: a local MQTT broker that also serves the web page, and a fake board for testing |

## Run the firmware

```bash
cd firmware
SSID=MyWifi PASSWORD=secret ROOM=lobby cargo run-c6
```

| Board | Command |
|---|---|
| ESP32-S31-Function-CoreBoard-1 | `cargo run-s31` |
| ESP32-S31-Korvo-1 | `cargo run-korvo` |
| ESP32-C6-DevKitC-1 | `cargo run-c6` |
| ESP32-C5-DevKitC-1 | `cargo run-c5` |
| ESP32-C3-DevKit-RUST-1 | `cargo run-c3` |
| ESP32-C3-LCDkit | `cargo run-c3-lcdkit` |

Every `run-*` alias has a matching `build-*` alias that only compiles.

Settings, all read at build time:

| Variable | Default | Meaning |
|---|---|---|
| `SSID`, `PASSWORD` | required | Wi-Fi network (2.4 GHz, WPA2-Personal or open). Leave `PASSWORD` empty for an open network. |
| `ROOM` | `lobby` | The room to join |
| `MQTT_HOST` | `broker.hivemq.com` | MQTT broker, host name or IPv4 address |
| `MQTT_PORT` | `1883` | MQTT broker port (plain MQTT 5 over TCP) |

Compiling the Wi-Fi password into the firmware is a workshop shortcut. For a real product,
provision the credentials at run time; the
[Wi-Fi Provisioning](https://docs.espressif.com/projects/rust/no_std-training/application/wifi-provisioning.html)
chapter of the training shows how.

## The web page

Open the published page, enter the room name, and press **Join room**. Every board in the room
shows up with its LED color; pick a color or a preset to change it, or use **All boards in the
room**.

To work on the page locally, serve the `web/` folder with any static file server, or run the backup
broker below.

## MQTT topics

Everything lives below `esp-hal-workshop/<room>/<board name>/`:

| Topic | Direction | Payload | Retained |
|---|---|---|---|
| `status` | board -> page | `online`, or `offline` (the board's last will) | yes |
| `color` | board -> page | the LED color, `#rrggbb` | yes |
| `set` | page -> board | a new color, `#rrggbb` | no |

`esp-hal-workshop/<room>/all/set` sets every board in the room.

Board names come from the MAC address: a 4-letter animal plus the last two MAC bytes in hex,
for example `bear83e8`, like in the ESP-NOW assignment.

Public brokers are public: anyone can read and send these messages. Don't send anything private.

## Offline backup: the local broker

If the public broker is unreachable (no internet, or the network blocks port 1883), run the broker
on a laptop in the room:

```bash
cd broker
cargo run --release
```

It prints its address, for example:

```
  Web page:  http://192.168.1.10:8080/
  Boards:    MQTT_HOST=192.168.1.10 SSID=... PASSWORD=... cargo run --release
```

- Boards: rebuild with `MQTT_HOST=<that address>`.
- Browsers: open `http://<that address>:8080/`. The page then uses the laptop's broker by default.
  It has to come from the laptop: the GitHub Pages copy is served over https, and browsers don't let
  an https page open an unencrypted `ws://` connection to another computer.

| Port | What |
|---|---|
| 1883 | MQTT 5 over TCP, for the boards |
| 1884 | MQTT 3.1.1 over TCP, for other tools |
| 8083 | MQTT 3.1.1 over WebSocket, for the web page |
| 8080 | The web page |

The broker is [rumqttd](https://crates.io/crates/rumqttd). It prints every message below
`esp-hal-workshop/`. Everything it serves, including the MQTT JavaScript library, is compiled in,
so it works without internet.

### A board without a board

`fake-device` speaks the same MQTT protocol as the firmware and prints the LED color, which is
handy for testing the page:

```bash
cd broker
ROOM=lobby NAME=fake0001 cargo run --release --bin fake-device                         # public broker
MQTT_HOST=localhost ROOM=lobby NAME=fake0001 cargo run --release --bin fake-device     # local broker
```

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
`web/vendor/mqtt.min.js` is [MQTT.js](https://github.com/mqttjs/MQTT.js) 5.16.1, MIT licensed
(see `web/vendor/mqtt.LICENSE.md`).

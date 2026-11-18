//! MQTT: joins a room on the broker and lets the web page set the LED color.
//!
//! Topics, below `esp-hal-workshop/<room>/<board name>/`:
//!
//! | Topic    | Direction       | Payload                | Retained |
//! |----------|-----------------|------------------------|----------|
//! | `status` | board -> page   | `online` / `offline`   | yes      |
//! | `color`  | board -> page   | the LED color, `#rrggbb` | yes    |
//! | `set`    | page -> board   | a new color, `#rrggbb` | no       |
//!
//! The board also listens on `esp-hal-workshop/<room>/all/set`, which sets
//! every board in the room at once.

use core::{fmt::Write, num::NonZero};

use embassy_futures::select::{Either, select};
use embassy_net::{IpAddress, Stack, dns::DnsQueryType, tcp::TcpSocket};
use embassy_time::{Duration, Timer};
use log::{info, warn};
use rust_mqtt::{
    Bytes,
    buffer::AllocBuffer,
    client::{
        Client,
        event::Event,
        options::{
            ConnectOptions, PublicationOptions, SubscriptionOptions, TopicReference, WillOptions,
        },
    },
    config::KeepAlive,
    types::{MqttBinary, MqttString, TopicFilter, TopicName},
};
use smart_leds::RGB8;

use crate::led::LED_COLOR;

/// The broker, compiled in from `MQTT_HOST` and `MQTT_PORT` (optional).
const MQTT_HOST: &str = match option_env!("MQTT_HOST") {
    Some(host) => host,
    None => "broker.hivemq.com",
};
const MQTT_PORT: &str = match option_env!("MQTT_PORT") {
    Some(port) => port,
    None => "1883",
};
/// The room to join, compiled in from `ROOM` (optional).
pub const ROOM: &str = match option_env!("ROOM") {
    Some(room) => room,
    None => "lobby",
};
/// Every topic of this workshop starts with this.
const PREFIX: &str = "esp-hal-workshop";

/// How often the board pings the broker, so it knows the board is still there.
const PING_INTERVAL: Duration = Duration::from_secs(30);

type Topic = heapless::String<96>;

fn topic(name: &str, leaf: &str) -> Topic {
    let mut topic = Topic::new();
    write!(topic, "{PREFIX}/{ROOM}/{name}/{leaf}").expect("topic fits");
    topic
}

/// `#rrggbb` (or `rrggbb`) to a color.
fn parse_color(text: &[u8]) -> Option<RGB8> {
    let text = core::str::from_utf8(text).ok()?.trim();
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    Some(RGB8::new(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

fn format_color(color: RGB8) -> heapless::String<7> {
    let mut text = heapless::String::new();
    write!(text, "#{:02x}{:02x}{:02x}", color.r, color.g, color.b).expect("7 characters fit");
    text
}

/// Connects to the broker, and connects again whenever something fails.
#[embassy_executor::task]
pub async fn mqtt_task(stack: Stack<'static>, name: &'static str) {
    let mut rx_buffer = [0u8; 2048];
    let mut tx_buffer = [0u8; 2048];
    let mut color = RGB8::default();

    loop {
        stack.wait_config_up().await;
        if let Err(e) = session(stack, name, &mut rx_buffer, &mut tx_buffer, &mut color).await {
            warn!("MQTT: {e}, reconnecting in 5 s");
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

/// One connection to the broker. Returns only when something fails.
async fn session(
    stack: Stack<'static>,
    name: &str,
    rx_buffer: &mut [u8],
    tx_buffer: &mut [u8],
    color: &mut RGB8,
) -> Result<(), &'static str> {
    // 1. Find the broker's IP address.
    let address = match MQTT_HOST.parse::<core::net::Ipv4Addr>() {
        Ok(ip) => IpAddress::Ipv4(ip),
        Err(_) => *stack
            .dns_query(MQTT_HOST, DnsQueryType::A)
            .await
            .map_err(|_| "DNS lookup failed")?
            .first()
            .ok_or("no address for the broker")?,
    };
    let port: u16 = MQTT_PORT.parse().map_err(|_| "MQTT_PORT is not a number")?;

    // 2. Open a TCP connection.
    let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(90)));
    info!("MQTT: connecting to {MQTT_HOST}:{port}...");
    socket
        .connect((address, port))
        .await
        .map_err(|_| "TCP connection failed")?;

    // 3. Connect MQTT. If the board disappears, the broker publishes the will:
    //    `offline` on our status topic.
    let status_topic = topic(name, "status");
    let color_topic = topic(name, "color");
    let set_topic = topic(name, "set");
    let all_set_topic = topic("all", "set");

    let mut client_id = heapless::String::<32>::new();
    write!(client_id, "esp-hal-{name}").expect("client ID fits");

    let mut buffer = AllocBuffer;
    let mut client = MqttClient::new(&mut buffer);
    let will = WillOptions::new(
        TopicName::new(MqttString::try_from(status_topic.as_str()).unwrap()).unwrap(),
        MqttBinary::try_from("offline").unwrap(),
    )
    .retain();
    let options = ConnectOptions::new()
        .clean_start()
        .keep_alive(KeepAlive::Seconds(NonZero::new(60).unwrap()))
        .will(will);
    client
        .connect(
            socket,
            &options,
            Some(MqttString::try_from(client_id.as_str()).unwrap()),
        )
        .await
        .map_err(|_| "MQTT connection refused")?;
    info!("MQTT: connected, room '{ROOM}', board '{name}'");

    // 4. Tell the room we are here, and which color we show.
    publish(&mut client, &status_topic, "online").await?;
    publish(&mut client, &color_topic, &format_color(*color)).await?;

    // 5. Listen for new colors.
    for filter in [&set_topic, &all_set_topic] {
        let filter = TopicFilter::new(MqttString::try_from(filter.as_str()).unwrap()).unwrap();
        client
            .subscribe(filter, &SubscriptionOptions::new())
            .await
            .map_err(|_| "subscribe failed")?;
    }

    // 6. Wait for messages; ping the broker when it's quiet.
    loop {
        let header = match select(client.poll_header(), Timer::after(PING_INTERVAL)).await {
            Either::First(header) => header.map_err(|_| "connection lost")?,
            Either::Second(()) => {
                client.ping().await.map_err(|_| "ping failed")?;
                continue;
            }
        };
        let event = client
            .poll_body(header)
            .await
            .map_err(|_| "connection lost")?;
        let Event::Publish(publish_event) = event else {
            continue;
        };
        match parse_color(&publish_event.message) {
            Some(new_color) => {
                *color = new_color;
                LED_COLOR.signal(new_color);
                let text = format_color(new_color);
                info!("MQTT: LED set to {text}");
                publish(&mut client, &color_topic, &text).await?;
            }
            None => warn!("MQTT: ignored a message that is not a #rrggbb color"),
        }
    }
}

/// The MQTT client type: a TCP socket underneath, buffers from the heap, and
/// small limits on how many requests may be in flight at once.
type MqttClient<'a, 's> = Client<'a, 's, TcpSocket<'s>, AllocBuffer, 2, 1, 1, 1, 0, 0, 0>;

/// Publishes a retained message, so the web page sees it even if it connects later.
async fn publish(
    client: &mut MqttClient<'_, '_>,
    topic: &str,
    payload: &str,
) -> Result<(), &'static str> {
    let topic = TopicName::new(MqttString::try_from(topic).unwrap()).unwrap();
    let options = PublicationOptions::new(TopicReference::Name(topic)).retain();
    client
        .publish(&options, Bytes::from(payload.as_bytes()))
        .await
        .map_err(|_| "publish failed")?;
    Ok(())
}

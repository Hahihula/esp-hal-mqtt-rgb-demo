//! A pretend board for testing the web page and the broker without hardware.
//!
//! It speaks the same MQTT protocol as the firmware (MQTT 5, rust-mqtt) and
//! prints the LED color instead of showing it:
//!
//! ```text
//! MQTT_HOST=localhost ROOM=lobby NAME=fake0001 cargo run --bin fake-device
//! ```
//!
//! Without `MQTT_HOST` it uses broker.hivemq.com.

use std::{env, num::NonZero, time::Duration};

use embedded_io_adapters::tokio_1::FromTokio;
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
use tokio::{net::TcpStream, select, time::sleep};

const PREFIX: &str = "esp-hal-workshop";

type MqttClient<'c> = Client<'static, 'c, FromTokio<TcpStream>, AllocBuffer, 2, 1, 1, 1, 0, 0, 0>;

#[tokio::main]
async fn main() {
    let host = env::var("MQTT_HOST").unwrap_or_else(|_| "broker.hivemq.com".into());
    let port: u16 = env::var("MQTT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(1883);
    let room = env::var("ROOM").unwrap_or_else(|_| "lobby".into());
    let name = env::var("NAME").unwrap_or_else(|_| "fake0001".into());

    loop {
        if let Err(e) = session(&host, port, &room, &name).await {
            eprintln!("{name}: {e}, reconnecting in 5 s");
        }
        sleep(Duration::from_secs(5)).await;
    }
}

async fn session(host: &str, port: u16, room: &str, name: &str) -> Result<(), String> {
    let status_topic = format!("{PREFIX}/{room}/{name}/status");
    let color_topic = format!("{PREFIX}/{room}/{name}/color");
    let set_topic = format!("{PREFIX}/{room}/{name}/set");
    let all_set_topic = format!("{PREFIX}/{room}/all/set");
    let client_id = format!("esp-hal-{name}");

    let stream = TcpStream::connect((host, port))
        .await
        .map_err(|e| format!("TCP: {e}"))?;

    let mut buffer = AllocBuffer;
    let mut client: MqttClient<'_> = Client::new(&mut buffer);
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
            FromTokio::new(stream),
            &options,
            Some(MqttString::try_from(client_id.as_str()).unwrap()),
        )
        .await
        .map_err(|e| format!("MQTT connect: {e:?}"))?;
    println!("{name}: connected to {host}:{port}, room '{room}'");

    let mut color = String::from("#000000");
    publish(&mut client, &status_topic, "online").await?;
    publish(&mut client, &color_topic, &color).await?;
    for filter in [&set_topic, &all_set_topic] {
        let filter = TopicFilter::new(MqttString::try_from(filter.as_str()).unwrap()).unwrap();
        client
            .subscribe(filter, &SubscriptionOptions::new())
            .await
            .map_err(|e| format!("subscribe: {e:?}"))?;
    }

    loop {
        let header = select! {
            header = client.poll_header() => header.map_err(|e| format!("poll: {e:?}"))?,
            () = sleep(Duration::from_secs(30)) => {
                client.ping().await.map_err(|e| format!("ping: {e:?}"))?;
                continue;
            }
        };
        let event = client
            .poll_body(header)
            .await
            .map_err(|e| format!("poll: {e:?}"))?;
        let Event::Publish(publish_event) = event else {
            continue;
        };
        let text = String::from_utf8_lossy(&publish_event.message)
            .trim()
            .to_lowercase();
        let hex = text.strip_prefix('#').unwrap_or(&text);
        if hex.len() == 6 && u32::from_str_radix(hex, 16).is_ok() {
            color = format!("#{hex}");
            println!("{name}: LED {color}");
            publish(&mut client, &color_topic, &color).await?;
        } else {
            println!("{name}: ignored '{text}'");
        }
    }
}

async fn publish(client: &mut MqttClient<'_>, topic: &str, payload: &str) -> Result<(), String> {
    let topic = TopicName::new(MqttString::try_from(topic).unwrap()).unwrap();
    let options = PublicationOptions::new(TopicReference::Name(topic)).retain();
    client
        .publish(&options, Bytes::from(payload.as_bytes()))
        .await
        .map(|_| ())
        .map_err(|e| format!("publish: {e:?}"))
}

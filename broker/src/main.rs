//! Offline backup for the esp-hal workshop.
//!
//! Runs an MQTT broker and serves the web page, so the workshop works without
//! the public broker (or without internet at all):
//!
//! | Port | What                                                   |
//! |------|--------------------------------------------------------|
//! | 1883 | MQTT 5 over TCP, for the boards                        |
//! | 1884 | MQTT 3.1.1 over TCP, for other tools                   |
//! | 8083 | MQTT 3.1.1 over WebSocket, for the web page            |
//! | 8080 | The web page itself, over plain HTTP                   |
//!
//! The page has to come from here, not from GitHub Pages: a page served over
//! https may not open an unencrypted `ws://` connection to another computer.
//!
//! Every message below `esp-hal-workshop/` is printed.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    thread,
};

use rumqttd::{Broker, Config, ConnectionSettings, Notification, RouterConfig, ServerSettings};

const MQTT5_PORT: u16 = 1883;
const MQTT4_PORT: u16 = 1884;
const WS_PORT: u16 = 8083;
const HTTP_PORT: u16 = 8080;

/// The web page, compiled into this program so it works offline.
const FILES: &[(&str, &str, &[u8])] = &[
    (
        "/",
        "text/html; charset=utf-8",
        include_bytes!("../../web/index.html"),
    ),
    (
        "/index.html",
        "text/html; charset=utf-8",
        include_bytes!("../../web/index.html"),
    ),
    (
        "/app.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/app.js"),
    ),
    (
        "/style.css",
        "text/css; charset=utf-8",
        include_bytes!("../../web/style.css"),
    ),
    (
        "/vendor/mqtt.min.js",
        "text/javascript; charset=utf-8",
        include_bytes!("../../web/vendor/mqtt.min.js"),
    ),
];

fn main() {
    let mut broker = Broker::new(config());
    let (mut link_tx, mut link_rx) = broker.link("workshop-logger").expect("logger link");
    thread::spawn(move || broker.start().expect("MQTT broker failed"));
    thread::spawn(serve_page);

    link_tx
        .subscribe("esp-hal-workshop/#")
        .expect("logger subscription");

    let ip = host_ipv4()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "<this computer's IP>".to_owned());
    println!("esp-hal workshop broker");
    println!();
    println!("  Web page:  http://{ip}:{HTTP_PORT}/");
    println!("  Boards:    MQTT_HOST={ip} SSID=... PASSWORD=... cargo run --release");
    println!();
    println!("  MQTT 5 on {MQTT5_PORT}, MQTT 3.1.1 on {MQTT4_PORT}, WebSocket on {WS_PORT}");
    println!("  Printing every message below esp-hal-workshop/ (Ctrl+C to stop)");
    println!();

    loop {
        if let Ok(Some(Notification::Forward(forward))) = link_rx.recv() {
            let topic = String::from_utf8_lossy(&forward.publish.topic);
            let payload = String::from_utf8_lossy(&forward.publish.payload);
            println!("{topic} = {payload}");
        }
    }
}

fn config() -> Config {
    let listener = |name: &str, port: u16| {
        let settings = ServerSettings {
            name: name.to_owned(),
            listen: SocketAddr::from(([0, 0, 0, 0], port)),
            tls: None,
            next_connection_delay_ms: 1,
            connections: ConnectionSettings {
                connection_timeout_ms: 60_000,
                max_payload_size: 20 * 1024,
                max_inflight_count: 100,
                auth: None,
                external_auth: None,
                dynamic_filters: true,
            },
        };
        Some(HashMap::from([(name.to_owned(), settings)]))
    };

    Config {
        id: 0,
        router: RouterConfig {
            max_connections: 1_024,
            max_outgoing_packet_count: 200,
            max_segment_size: 1_024 * 1_024,
            max_segment_count: 10,
            custom_segment: None,
            initialized_filters: None,
            shared_subscriptions_strategy: Default::default(),
        },
        v4: listener("mqtt4", MQTT4_PORT),
        v5: listener("mqtt5", MQTT5_PORT),
        ws: listener("ws", WS_PORT),
        cluster: None,
        console: None,
        bridge: None,
        prometheus: None,
        metrics: None,
    }
}

/// A tiny HTTP server for the files in [`FILES`].
fn serve_page() {
    let listener = TcpListener::bind(("0.0.0.0", HTTP_PORT)).expect("HTTP port in use");
    for stream in listener.incoming().flatten() {
        thread::spawn(move || {
            let _ = handle(stream);
        });
    }
}

fn handle(mut stream: TcpStream) -> std::io::Result<()> {
    let mut request_line = String::new();
    BufReader::new(&stream).read_line(&mut request_line)?;
    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    let path = path.split('?').next().unwrap_or("/");
    match FILES.iter().find(|(p, _, _)| *p == path) {
        Some((_, content_type, body)) => {
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )?;
            stream.write_all(body)
        }
        None => stream.write_all(
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found",
        ),
    }
}

/// This computer's IP address on the local network (best effort): the address
/// of the interface that would carry traffic to the internet.
fn host_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    match socket.local_addr().ok()?.ip() {
        IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_unspecified() => Some(ip),
        _ => None,
    }
}

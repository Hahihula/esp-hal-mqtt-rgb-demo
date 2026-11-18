//! Wi-Fi: keeps the station connected, and runs the network stack.

use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config, Interface, WifiController, sta::StationConfig,
};
use log::{info, warn};

/// The Wi-Fi network, compiled in from the `SSID` and `PASSWORD` environment variables.
pub const SSID: &str = env!("SSID");
pub const PASSWORD: &str = env!("PASSWORD");

/// The station configuration for `SSID` and `PASSWORD`.
pub fn station_config() -> Config {
    let auth = if PASSWORD.is_empty() {
        AuthenticationMethodConfig::Open
    } else {
        AuthenticationMethodConfig::Wpa2Personal(PASSWORD.try_into().expect("password too long"))
    };
    Config::Station(
        StationConfig::default()
            .with_ssid(SSID.try_into().expect("SSID too long"))
            .with_authentication(auth),
    )
}

/// Connects to the access point, and connects again whenever the connection drops.
#[embassy_executor::task]
pub async fn connection_task(mut controller: WifiController<'static>) {
    loop {
        info!("Connecting to Wi-Fi '{SSID}'...");
        match controller.connect_async().await {
            Ok(_) => {
                info!("Wi-Fi connected");
                let _ = controller.wait_for_disconnect_async().await;
                warn!("Wi-Fi disconnected");
            }
            Err(e) => warn!("Wi-Fi connection failed: {e:?}"),
        }
        Timer::after(Duration::from_secs(5)).await;
    }
}

/// Runs the network stack: DHCP, DNS, TCP. It must run all the time.
#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await
}

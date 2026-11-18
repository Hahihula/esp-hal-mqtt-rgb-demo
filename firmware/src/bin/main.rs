#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::{clock::CpuClock, rmt::Rmt, rng::Rng, time::Rate, timer::timg::TimerGroup};
use esp_hal_smartled::{RmtSmartLeds, WS2812B_TIMING};
use esp_radio::wifi::{ControllerConfig, Interface, WifiController};
use log::info;
use mqtt_led::{led, mqtt, name, wifi};
use static_cell::StaticCell;

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

// The RGB LED pin of each board. In a project generated for one board, this is
// just `peripherals.GPIO8` (or your board's pin) in `main`.
#[cfg(not(any(
    feature = "s31-function-coreboard",
    feature = "s31-korvo",
    feature = "c6-devkitc",
    feature = "c5-devkitc",
    feature = "c3-rust-board",
    feature = "c3-lcdkit",
)))]
compile_error!(
    "Select a board, for example `cargo run-c6`. See the [alias] table in .cargo/config.toml."
);

macro_rules! led_pin {
    ($p:ident) => {{
        #[cfg(feature = "s31-function-coreboard")]
        let pin = $p.GPIO60;
        #[cfg(feature = "s31-korvo")]
        let pin = $p.GPIO37;
        #[cfg(any(feature = "c6-devkitc", feature = "c3-lcdkit"))]
        let pin = $p.GPIO8;
        #[cfg(feature = "c5-devkitc")]
        let pin = $p.GPIO27;
        #[cfg(feature = "c3-rust-board")]
        let pin = $p.GPIO2;
        pin
    }};
}

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    esp_println::logger::init_logger_from_env();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // The Wi-Fi driver and the MQTT client need a heap.
    esp_alloc::heap_allocator!(size: 72 * 1024);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    // The RGB LED, now in async mode, owned by its own task.
    let freq = Rate::from_mhz(80);
    let rmt = Rmt::new(peripherals.RMT, freq).unwrap().into_async();
    let led = RmtSmartLeds::new(WS2812B_TIMING, rmt.channel0, led_pin!(peripherals), freq).unwrap();
    spawner.spawn(led::led_task(led).unwrap());

    // Wi-Fi: the controller connects to the access point, the interface
    // carries the network traffic.
    let controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(wifi::station_config()),
    )
    .expect("Failed to initialize Wi-Fi controller");
    let interface = Interface::station();

    // The network stack: DHCP for an IP address, DNS, and TCP sockets.
    static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
    let rng = Rng::new();
    let seed = u64::from(rng.random()) << 32 | u64::from(rng.random());
    let (stack, runner) = embassy_net::new(
        interface,
        embassy_net::Config::dhcpv4(Default::default()),
        RESOURCES.init(StackResources::new()),
        seed,
    );

    // The board's name in the room, for example `bear83e8`.
    static NAME: StaticCell<heapless::String<8>> = StaticCell::new();
    let name = NAME.init(name::board_name());
    info!("This board is '{name}', room '{}'", mqtt::ROOM);

    spawner.spawn(wifi::connection_task(controller).unwrap());
    spawner.spawn(wifi::net_task(runner).unwrap());
    spawner.spawn(mqtt::mqtt_task(stack, name.as_str()).unwrap());

    stack.wait_config_up().await;
    if let Some(config) = stack.config_v4() {
        info!("Got IP address {}", config.address);
    }

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

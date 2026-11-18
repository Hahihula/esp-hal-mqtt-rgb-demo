//! The onboard RGB LED, owned by one task. Other tasks send it new colors.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use esp_hal::Async;
use esp_hal_smartled::{RmtSmartLeds, buffer_size, color_order};
use log::warn;
use smart_leds::{RGB8, SmartLedsWriteAsync, gamma};

/// The LED driver: one WS2812 LED, async RMT channel.
pub type Led = RmtSmartLeds<'static, { buffer_size::<RGB8>(1) }, Async, RGB8, color_order::Grb>;

/// The newest color for the LED. A `Signal` keeps only the latest value, so a
/// burst of colors from the web page never queues up.
pub static LED_COLOR: Signal<CriticalSectionRawMutex, RGB8> = Signal::new();

/// Waits for colors and writes each one to the LED.
#[embassy_executor::task]
pub async fn led_task(mut led: Led) {
    loop {
        let color = LED_COLOR.wait().await;
        // Gamma correction makes the LED look like the color picked on the web page.
        if let Err(e) = led.write(gamma([color].into_iter())).await {
            warn!("LED write failed: {e:?}");
        }
    }
}

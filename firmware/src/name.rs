//! A short, readable name for this board, made from its MAC address.

use core::fmt::Write;

use esp_hal::efuse::{InterfaceMacAddress, interface_mac_address};

const ANIMALS: [&str; 16] = [
    "bear", "wolf", "lynx", "seal", "crab", "frog", "duck", "goat", "hawk", "mole", "newt", "swan",
    "toad", "moth", "deer", "orca",
];

/// A 4-letter animal picked from the MAC address, then its last two bytes in
/// hex, for example `bear83e8`.
pub fn board_name() -> heapless::String<8> {
    let mac = interface_mac_address(InterfaceMacAddress::Station);
    let mac = mac.as_bytes();
    let animal = ANIMALS[usize::from(mac[3] ^ mac[4] ^ mac[5]) % ANIMALS.len()];
    let mut name = heapless::String::new();
    write!(name, "{animal}{:02x}{:02x}", mac[4], mac[5]).expect("4 + 4 characters fit");
    name
}

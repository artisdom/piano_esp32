//! VCC-GND Studio YD-ESP32-S3 (schematic YD-ESP32-S3-COREBOARD V1.4) —
//! ESP32-S3-WROOM-1 N16R8 (16 MB flash, 8 MB octal PSRAM), BOOT (GPIO0) and
//! RST keys, a WS2812 RGB LED on GPIO48, two USB-C ports: "COM" through a
//! CH343 (UART0 console, auto-reset for flashing) and "USB" on the S3's
//! native USB (GPIO19/20), used here as OTG host for the piano. No display:
//! the headless UI drives it with the BOOT key, the RGB LED and a Wi-Fi web
//! page; songs come from the flash song pack.
//!
//! The OTG port's VBUS only reaches the piano with the "USB-OTG" solder
//! jumper bridged (it bypasses the diode that otherwise lets that port only
//! power the board). USB host and Bluetooth run at the same time; USB wins
//! when both reach the piano (src/dual.rs). USB host and WS2812 bring-up
//! are in components/yds3_bsp (C).

use std::sync::Mutex;

use esp_idf_svc::sys::{bsp, esp};
use log::{info, warn};

use super::key::Key;
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "YD-ESP32-S3",
    backlight: false,
    power_latch: false,
    usb_midi: true,
    ble_midi: true,
    buttons: true,
    wake_hint: "BOOT or RST",
    key_hint: "",
    touch: false,
};

const BOOT_KEY: i32 = 0;

static BOOT: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));

pub fn early_init() {
    let err = unsafe { bsp::yds3_led_init() };
    if err != 0 {
        warn!("board: WS2812 init failed ({err})");
    }
    // Dim white while booting.
    status_led(true, [8, 8, 8]);
    info!("board: {}", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn usb_host_start() -> anyhow::Result<()> {
    unsafe { esp!(bsp::yds3_usb_host_start())? };
    Ok(())
}

/// The WS2812, colour as given (the headless UI keeps it dim).
pub fn status_led(on: bool, rgb: [u8; 3]) {
    let [r, g, b] = if on { rgb } else { [0; 3] };
    unsafe { bsp::yds3_led_set(r, g, b) };
}

pub fn set_backlight(_percent: i32) {}

/// No power switch: LED off and deep sleep; BOOT (GPIO0, an RTC pin) or
/// RST wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, BOOT or RST wakes");
    status_led(false, [0; 3]);
    unsafe {
        while bsp::gpio_get_level(BOOT_KEY) == 0 {
            bsp::vTaskDelay(10);
        }
        bsp::esp_sleep_enable_ext0_wakeup(BOOT_KEY as bsp::gpio_num_t, 0);
        bsp::esp_deep_sleep_start()
    }
}

pub fn reboot() -> ! {
    unsafe { bsp::esp_restart() }
}

pub fn bt_controller_start(_status: &Status) -> anyhow::Result<()> {
    Ok(())
}

/// BOOT: tap, double tap, hold (`KeyTap` / `KeyDoubleTap` / `KeyHold`).
pub fn poll_buttons() -> Vec<Button> {
    super::key::poll(&BOOT)
}

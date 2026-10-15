//! ESP32-1732S019 ("1.9 inch ESP32-S3") — ESP32-S3-WROOM-1 N16R8 (16 MB
//! flash, 8 MB octal PSRAM), 1.9″ 170×320 IPS with an ST7789 on SPI, no
//! touch, BOOT key (GPIO0), CH340 serial. No microSD, USB host or speaker:
//! songs come from the flash song pack, the piano over BLE.
//!
//! Pins (vendor schematic ESP32-1732S019-LCM-V1.0 and its Arduino demos):
//! LCD SCLK 12, MOSI 13, CS 10, DC 11, RST 1; backlight GPIO14 (high = on,
//! through an AO3402); the panel's 170 columns sit at offset 35 in the
//! ST7789's 240-column memory. Used in landscape (320×170).

use std::sync::Mutex;

use esp_idf_svc::sys::{bsp, esp};
use log::info;

use super::key::{self, Key};
use super::spi_lcd::{self, Pins, SpiLcd};
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "ESP32-1732S019",
    backlight: true,
    power_latch: false,
    usb_midi: false,
    ble_midi: true,
    buttons: true,
    wake_hint: "BOOT or RESET",
    key_hint: "tap ›  2× ‹  hold OK",
    touch: false,
};

pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 170;
/// Logical UI pixels per panel pixel: 1 = the layout's native size.
pub const UI_SCALE: f32 = 1.0;
pub const ROUND: bool = false;

const BOOT_KEY: i32 = 0;
const BACKLIGHT: i32 = 14;
const LCD_RST: i32 = 1;

static KEY: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));

pub fn early_init() {
    info!("board: {}", CAPS.name);
}

/// Nothing switchable: the radio is on-chip.
pub fn init_power(_transport: Transport) {}

pub fn lcd_init(bg565: u16) -> anyhow::Result<SpiLcd> {
    let pins = Pins {
        host: bsp::spi_host_device_t_SPI2_HOST,
        sclk: 12,
        mosi: 13,
        miso: -1,
        cs: 10,
        dc: 11,
        pclk_hz: 40_000_000,
        max_transfer: LCD_WIDTH * 10 * 2,
    };
    let (io, done) = spi_lcd::open_io(&pins)?;
    unsafe {
        let mut cfg: bsp::esp_lcd_panel_dev_config_t = core::mem::zeroed();
        cfg.reset_gpio_num = LCD_RST;
        cfg.__bindgen_anon_1.rgb_ele_order = bsp::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB;
        cfg.bits_per_pixel = 16;
        let mut panel = core::ptr::null_mut();
        esp!(bsp::esp_lcd_new_panel_st7789(io, &cfg, &mut panel))?;
        esp!(bsp::esp_lcd_panel_reset(panel))?;
        esp!(bsp::esp_lcd_panel_init(panel))?;
        // IPS panel: colours are inverted in the controller.
        esp!(bsp::esp_lcd_panel_invert_color(panel, true))?;
        // Landscape: rows/columns swapped; the 170-pixel side is offset.
        esp!(bsp::esp_lcd_panel_swap_xy(panel, true))?;
        esp!(bsp::esp_lcd_panel_mirror(panel, false, true))?;
        esp!(bsp::esp_lcd_panel_set_gap(panel, 0, 35))?;
        let lcd = SpiLcd::finish(panel, io, done, LCD_WIDTH, LCD_HEIGHT, bg565)?;
        spi_lcd::backlight_init(BACKLIGHT, false)?;
        Ok(lcd)
    }
}

pub fn lcd_draw(lcd: &SpiLcd, x: usize, y: usize, w: usize, h: usize, px: &mut [u16]) {
    lcd.draw(x, y, w, h, px);
}

pub fn touch_point() -> Option<(u16, u16)> {
    None
}

pub fn set_backlight(percent: i32) {
    spi_lcd::backlight_set(percent);
}

/// No power switch: backlight off and deep sleep; BOOT (GPIO0, an RTC pin)
/// or RESET wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, BOOT or RESET wakes");
    set_backlight(0);
    unsafe {
        // Wait for the key to be released, or it wakes us at once.
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

/// Native controller: `nimble_port_init` brings it up itself.
pub fn bt_controller_start(_status: &Status) -> anyhow::Result<()> {
    Ok(())
}

pub fn poll_buttons() -> Vec<Button> {
    key::poll(&KEY)
}

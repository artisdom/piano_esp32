//! ESP32-2424S012C ("1.28 inch ESP32-C3") — ESP32-C3-MINI-1U (4 MB flash,
//! 400 KB SRAM, no PSRAM), 1.28″ round 240×240 IPS with a GC9A01 on SPI,
//! CST816D capacitive touch, BOOT key (GPIO9), Li-ion charger, console on
//! the C3's USB Serial/JTAG. No microSD, USB host or speaker: songs come
//! from the flash song pack, the piano over BLE.
//!
//! Pins (vendor schematic ESP32-2424S012-V1.0 and its factory demo):
//! LCD SCLK 6, MOSI 7, CS 10, DC 2, no reset line; backlight GPIO3 (high =
//! on); touch I2C SDA 4, SCL 5, RST 1, INT 0 (address 0x15).

use std::sync::Mutex;

use esp_idf_svc::sys::{bsp, esp};
use log::{info, warn};

use super::cst816::{self, Cst816};
use super::key::{self, Key};
use super::spi_lcd::{self, Pins, SpiLcd};
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "ESP32-2424S012",
    backlight: true,
    power_latch: false,
    usb_midi: false,
    ble_midi: true,
    buttons: true,
    wake_hint: "a touch or RESET",
    key_hint: "",
    touch: true,
};

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 240;
/// Logical UI pixels per panel pixel: 1 = the layout's native size.
pub const UI_SCALE: f32 = 1.0;
pub const ROUND: bool = true;
/// The GC9A01 panels on this board differ in scan direction between
/// production runs: one bench unit is right with no mirroring, another
/// shows everything mirrored left-right. Settings → Mirror display flips
/// it (saved per unit); the touch panel follows the glass, not the scan, so
/// it needs no change.
pub const LCD_MIRROR: Option<fn(&SpiLcd, bool)> = Some(lcd_mirror);

fn lcd_mirror(lcd: &SpiLcd, on: bool) {
    unsafe {
        let _ = bsp::esp_lcd_panel_mirror(lcd.panel, on, false);
    }
}

const BOOT_KEY: i32 = 9;
const BACKLIGHT: i32 = 3;
const TP_SDA: i32 = 4;
const TP_SCL: i32 = 5;
const TP_RST: i32 = 1;
const TP_INT: i32 = 0;

static KEY: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));

/// The CST816D (None until `lcd_init`, or if it is absent).
static TOUCH: Mutex<Option<Cst816>> = Mutex::new(None);

pub fn early_init() {
    info!("board: {}", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn lcd_init(bg565: u16) -> anyhow::Result<SpiLcd> {
    let pins = Pins {
        host: bsp::spi_host_device_t_SPI2_HOST,
        sclk: 6,
        mosi: 7,
        miso: -1,
        cs: 10,
        dc: 2,
        // GPIO6/7/10 are the C3's FSPI IOMUX pins: 80 MHz works.
        pclk_hz: 80_000_000,
        max_transfer: LCD_WIDTH * 10 * 2,
    };
    let (io, done) = spi_lcd::open_io(&pins)?;
    let lcd = unsafe {
        let mut cfg: bsp::esp_lcd_panel_dev_config_t = core::mem::zeroed();
        cfg.reset_gpio_num = -1;
        cfg.__bindgen_anon_1.rgb_ele_order = bsp::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_BGR;
        cfg.bits_per_pixel = 16;
        let mut panel = core::ptr::null_mut();
        esp!(bsp::esp_lcd_new_panel_gc9a01(io, &cfg, &mut panel))?;
        esp!(bsp::esp_lcd_panel_reset(panel))?;
        esp!(bsp::esp_lcd_panel_init(panel))?;
        esp!(bsp::esp_lcd_panel_invert_color(panel, true))?;
        esp!(bsp::esp_lcd_panel_mirror(panel, false, false))?;
        SpiLcd::finish(panel, io, done, LCD_WIDTH, LCD_HEIGHT, bg565)?
    };
    spi_lcd::backlight_init(BACKLIGHT, false)?;
    match Cst816::open(&cst816::Pins { sda: TP_SDA, scl: TP_SCL, rst: TP_RST }) {
        Ok(t) => *TOUCH.lock().unwrap() = Some(t),
        Err(e) => warn!("touch: CST816D not found ({e}) — BOOT key only"),
    }
    Ok(lcd)
}

pub fn lcd_draw(lcd: &SpiLcd, x: usize, y: usize, w: usize, h: usize, px: &mut [u16]) {
    lcd.draw(x, y, w, h, px);
}

/// Finger position in panel pixels (the CST816D's axes match the panel).
pub fn touch_point() -> Option<(u16, u16)> {
    let (x, y) = TOUCH.lock().unwrap().as_ref()?.read()?;
    (x < LCD_WIDTH as u16 && y < LCD_HEIGHT as u16).then_some((x, y))
}

pub fn set_backlight(percent: i32) {
    spi_lcd::backlight_set(percent);
}

/// No power switch: backlight off and deep sleep. The C3 can only wake from
/// GPIO0–5, so a touch (TP_INT, GPIO0, pulses low) or RESET wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, touch or RESET wakes");
    set_backlight(0);
    unsafe {
        bsp::vTaskDelay(300);
        bsp::esp_deep_sleep_enable_gpio_wakeup(
            1u64 << TP_INT,
            bsp::esp_deepsleep_gpio_wake_up_mode_t_ESP_GPIO_WAKEUP_GPIO_LOW,
        );
        bsp::esp_deep_sleep_start()
    }
}

pub fn reboot() -> ! {
    unsafe { bsp::esp_restart() }
}

pub fn bt_controller_start(_status: &Status) -> anyhow::Result<()> {
    Ok(())
}

pub fn poll_buttons() -> Vec<Button> {
    key::poll(&KEY)
}

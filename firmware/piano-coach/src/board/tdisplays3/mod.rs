//! LILYGO T-Display-S3 — ESP32-S3R8 (16 MB flash, 8 MB octal PSRAM), 1.9″
//! 170×320 IPS with an ST7789 on an 8-bit Intel-8080 bus, two keys (BOOT =
//! GPIO0, KEY = GPIO14), optional CST816 touch (T-Display-S3 Touch), Li-ion
//! charger, console on the S3's USB Serial/JTAG. No microSD, USB host or
//! speaker: songs come from the flash song pack, the piano over BLE.
//!
//! Pins (LILYGO pin map and examples/lv_demos): LCD D0–D7 = 39, 40, 41, 42,
//! 45, 46, 47, 48; WR 8, RD 9 (held high), DC 7, CS 6, RST 5; backlight
//! GPIO38; GPIO15 powers the LCD and peripherals (must be high); touch I2C
//! SDA 18, SCL 17, RST 21, INT 16. Landscape 320×170 like the vendor demos
//! (swap XY, mirror Y, 35-pixel gap) plus their ST7789 init sequence.
//!
//! Keys: KEY ▶ / hold OK, BOOT ◀ / hold back — two quick presses move five
//! rows. On the play screen KEY pauses / resumes.

use std::sync::Mutex;

use esp_idf_svc::sys::{bsp, esp};
use log::info;

use super::cst816::{self, Cst816};
use super::key::Key;
use super::spi_lcd::{self, I80Pins, SpiLcd};
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "LILYGO T-Display-S3",
    backlight: true,
    power_latch: false,
    usb_midi: false,
    ble_midi: true,
    buttons: true,
    wake_hint: "BOOT or RESET",
    key_hint: "KEY ›  BOOT ‹  hold: OK / back",
    touch: true,
};

pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 170;
pub const ROUND: bool = false;

const BOOT_KEY: i32 = 0;
const USER_KEY: i32 = 14;
const POWER_ON: i32 = 15;
const BACKLIGHT: i32 = 38;
const LCD_RST: i32 = 5;
const LCD_RD: i32 = 9;

static BOOT: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));
static KEY: Mutex<Key> = Mutex::new(Key::new(USER_KEY));
static TOUCH: Mutex<Option<Cst816>> = Mutex::new(None);

/// The vendor's ST7789 settings for this panel (examples/lv_demos,
/// LCD_MODULE_CMD_1: newer panels need them): porch, gate, VCOM, power and
/// gamma. (command, parameters)
const INIT: &[(u8, &[u8])] = &[
    (0xB2, &[0x0B, 0x0B, 0x00, 0x33, 0x33]),
    (0xB7, &[0x75]),
    (0xBB, &[0x28]),
    (0xC0, &[0x2C]),
    (0xC2, &[0x01]),
    (0xC3, &[0x1F]),
    (0xC6, &[0x13]),
    (0xD0, &[0xA7]),
    (0xD0, &[0xA4, 0xA1]),
    (0xD6, &[0xA1]),
    (0xE0, &[0xF0, 0x05, 0x0A, 0x06, 0x06, 0x03, 0x2B, 0x32, 0x43, 0x36, 0x11, 0x10, 0x2B, 0x32]),
    (0xE1, &[0xF0, 0x08, 0x0C, 0x0B, 0x09, 0x24, 0x2B, 0x22, 0x43, 0x38, 0x15, 0x16, 0x2F, 0x37]),
];

fn output(pin: i32, level: u32) {
    unsafe {
        let mut c: bsp::gpio_config_t = core::mem::zeroed();
        c.pin_bit_mask = 1u64 << pin;
        c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
        bsp::gpio_config(&c);
        bsp::gpio_set_level(pin, level);
    }
}

/// LCD + peripheral power on before anything talks to them.
pub fn early_init() {
    output(POWER_ON, 1);
    output(LCD_RD, 1);
    info!("board: {} (peripheral power on)", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn lcd_init(bg565: u16) -> anyhow::Result<SpiLcd> {
    let pins = I80Pins {
        dc: 7,
        wr: 8,
        cs: 6,
        data: [39, 40, 41, 42, 45, 46, 47, 48],
        pclk_hz: 16_000_000,
        max_transfer: LCD_WIDTH * 10 * 2,
    };
    let (io, done) = spi_lcd::open_io_i80(&pins)?;
    let lcd = unsafe {
        let mut cfg: bsp::esp_lcd_panel_dev_config_t = core::mem::zeroed();
        cfg.reset_gpio_num = LCD_RST;
        cfg.__bindgen_anon_1.rgb_ele_order = bsp::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB;
        cfg.bits_per_pixel = 16;
        let mut panel = core::ptr::null_mut();
        esp!(bsp::esp_lcd_new_panel_st7789(io, &cfg, &mut panel))?;
        esp!(bsp::esp_lcd_panel_reset(panel))?;
        esp!(bsp::esp_lcd_panel_init(panel))?;
        esp!(bsp::esp_lcd_panel_invert_color(panel, true))?;
        esp!(bsp::esp_lcd_panel_swap_xy(panel, true))?;
        esp!(bsp::esp_lcd_panel_mirror(panel, false, true))?;
        esp!(bsp::esp_lcd_panel_set_gap(panel, 0, 35))?;
        for (cmd, params) in INIT {
            esp!(bsp::esp_lcd_panel_io_tx_param(io, *cmd as i32, params.as_ptr().cast(), params.len()))?;
        }
        SpiLcd::finish(panel, io, done, LCD_WIDTH, LCD_HEIGHT, bg565)?
    };
    spi_lcd::backlight_init(BACKLIGHT)?;
    match Cst816::open(&cst816::Pins { sda: 18, scl: 17, rst: 21 }) {
        Ok(t) => *TOUCH.lock().unwrap() = Some(t),
        Err(_) => info!("touch: none (T-Display-S3 without touch) — keys only"),
    }
    Ok(lcd)
}

pub fn lcd_draw(lcd: &SpiLcd, x: usize, y: usize, w: usize, h: usize, px: &mut [u16]) {
    lcd.draw(x, y, w, h, px);
}

/// Finger position in landscape panel pixels. The CST816 reports the
/// portrait panel (x 0–169 across, y 0–319 down).
pub fn touch_point() -> Option<(u16, u16)> {
    let (x, y) = TOUCH.lock().unwrap().as_ref()?.read()?;
    let (lx, ly) = (y, (LCD_HEIGHT as u16 - 1).checked_sub(x)?);
    (lx < LCD_WIDTH as u16 && ly < LCD_HEIGHT as u16).then_some((lx, ly))
}

pub fn set_backlight(percent: i32) {
    spi_lcd::backlight_set(percent);
}

/// No power switch: backlight and peripheral power off, deep sleep; BOOT
/// (GPIO0, an RTC pin) or RESET wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, BOOT or RESET wakes");
    set_backlight(0);
    unsafe {
        while bsp::gpio_get_level(BOOT_KEY) == 0 {
            bsp::vTaskDelay(10);
        }
        bsp::gpio_set_level(POWER_ON, 0);
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

/// KEY: tap ▶ (double: five rows), hold OK. BOOT: tap ◀ (double: five
/// rows back), hold back.
pub fn poll_buttons() -> Vec<Button> {
    let mut v = Vec::new();
    match KEY.lock().unwrap().poll() {
        Some(Button::KeyTap) => v.push(Button::NavNext),
        Some(Button::KeyDoubleTap) => v.extend([Button::NavNext; 5]),
        Some(Button::KeyHold) => v.push(Button::NavOk),
        _ => {}
    }
    match BOOT.lock().unwrap().poll() {
        Some(Button::KeyTap) => v.push(Button::NavPrev),
        Some(Button::KeyDoubleTap) => v.extend([Button::NavPrev; 5]),
        Some(Button::KeyHold) => v.push(Button::NavBack),
        _ => {}
    }
    v
}

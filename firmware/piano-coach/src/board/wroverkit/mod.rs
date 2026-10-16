//! ESP-WROVER-KIT v4.1 — ESP32-WROVER (ESP32 rev 1, dual Xtensa 240 MHz,
//! 4 MB flash), 3.2″ 320×240 SPI LCD (ILI9341 or ST7789V, told apart by
//! the panel ID like ESP-IDF's spi_master/lcd example), BOOT key (GPIO0),
//! FT2232 USB bridge (channel A JTAG, channel B UART0 console), RGB LED,
//! microSD, camera header.
//!
//! LCD (user guide, "LCD/U5"): RST 18, SCL 19, D/C 21, CS 22, SDA 23,
//! SDO 25, backlight GPIO5 (active low). The microSD card-detect switch
//! shares GPIO21 with the LCD's D/C line, so the card slot is not used:
//! songs come from the flash song pack, the piano over BLE. The module's
//! PSRAM stays off — ESP32 rev 1 needs a cache workaround in all code
//! touching it, which Rust code is not built with — so RAM is the ~300 KB
//! internal SRAM, as on the ESP32-C3 (same song limits).
//!
//! The UI is drawn at 1.25× (the minimal UI's bar layout is designed for
//! 170 rows; this panel has 240).

use std::sync::Mutex;

use esp_idf_svc::sys::{bsp, esp};
use log::{info, warn};

use super::key::{self, Key};
use super::spi_lcd::{self, Pins, SpiLcd};
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "ESP-WROVER-KIT v4.1",
    backlight: true,
    power_latch: false,
    usb_midi: false,
    ble_midi: true,
    buttons: true,
    wake_hint: "BOOT or EN",
    key_hint: "tap ›  2× ‹  hold OK",
    touch: false,
};

pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 240;
/// Logical UI pixels per panel pixel: 256×192 logical.
pub const UI_SCALE: f32 = 1.25;
pub const ROUND: bool = false;
/// The panel's orientation is the same on every unit.
pub const LCD_MIRROR: Option<fn(&SpiLcd, bool)> = None;

const BOOT_KEY: i32 = 0;
const BACKLIGHT: i32 = 5;
const LCD_RST: i32 = 18;

static KEY: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));

/// Power, VCOM and gamma settings per controller, from ESP-IDF's
/// spi_master/lcd example (orientation and pixel format are set through
/// esp_lcd instead). (command, parameters)
const ST7789V: &[(u8, &[u8])] = &[
    (0xB2, &[0x0C, 0x0C, 0x00, 0x33, 0x33]),
    (0xB7, &[0x45]),
    (0xBB, &[0x2B]),
    (0xC0, &[0x2C]),
    (0xC2, &[0x01, 0xFF]),
    (0xC3, &[0x11]),
    (0xC4, &[0x20]),
    (0xC6, &[0x0F]),
    (0xD0, &[0xA4, 0xA1]),
    (0xE0, &[0xD0, 0x00, 0x05, 0x0E, 0x15, 0x0D, 0x37, 0x43, 0x47, 0x09, 0x15, 0x12, 0x16, 0x19]),
    (0xE1, &[0xD0, 0x00, 0x05, 0x0D, 0x0C, 0x06, 0x2D, 0x44, 0x40, 0x0E, 0x1C, 0x18, 0x16, 0x19]),
];
const ILI9341: &[(u8, &[u8])] = &[
    (0xCF, &[0x00, 0x83, 0x30]),
    (0xED, &[0x64, 0x03, 0x12, 0x81]),
    (0xE8, &[0x85, 0x01, 0x79]),
    (0xCB, &[0x39, 0x2C, 0x00, 0x34, 0x02]),
    (0xF7, &[0x20]),
    (0xEA, &[0x00, 0x00]),
    (0xC0, &[0x26]),
    (0xC1, &[0x11]),
    (0xC5, &[0x35, 0x3E]),
    (0xC7, &[0xBE]),
    (0xB1, &[0x00, 0x1B]),
    (0xF2, &[0x08]),
    (0x26, &[0x01]),
    (0xE0, &[0x1F, 0x1A, 0x18, 0x0A, 0x0F, 0x06, 0x45, 0x87, 0x32, 0x0A, 0x07, 0x02, 0x07, 0x05, 0x00]),
    (0xE1, &[0x00, 0x25, 0x27, 0x05, 0x10, 0x09, 0x3A, 0x78, 0x4D, 0x05, 0x18, 0x0D, 0x38, 0x3A, 0x1F]),
    (0xB7, &[0x07]),
    (0xB6, &[0x0A, 0x82, 0x27, 0x00]),
];

pub fn early_init() {
    info!("board: {}", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn lcd_init(bg565: u16) -> anyhow::Result<SpiLcd> {
    // A card in the microSD slot closes its card-detect switch, which
    // holds GPIO21 — the LCD's D/C line — low: the panel then stays white.
    unsafe {
        let mut c: bsp::gpio_config_t = core::mem::zeroed();
        c.pin_bit_mask = 1u64 << 21;
        c.mode = bsp::gpio_mode_t_GPIO_MODE_INPUT;
        c.pull_up_en = bsp::gpio_pullup_t_GPIO_PULLUP_ENABLE;
        bsp::gpio_config(&c);
        bsp::vTaskDelay(2);
        if bsp::gpio_get_level(21) == 0 {
            warn!("lcd: GPIO21 (D/C) is held low — a microSD card is in the slot; remove it, the LCD cannot work with it");
        }
    }
    let pins = Pins {
        host: bsp::spi_host_device_t_SPI2_HOST,
        sclk: 19,
        mosi: 23,
        miso: 25,
        cs: 22,
        dc: 21,
        // The clock pin is routed through the GPIO matrix: 26.7 MHz max.
        pclk_hz: 26_000_000,
        max_transfer: LCD_WIDTH * 10 * 2,
    };
    let (io, done) = spi_lcd::open_io(&pins)?;
    let lcd = unsafe {
        // Hardware reset first so the ID read sees a fresh controller.
        let mut c: bsp::gpio_config_t = core::mem::zeroed();
        c.pin_bit_mask = 1u64 << LCD_RST;
        c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
        bsp::gpio_config(&c);
        bsp::gpio_set_level(LCD_RST, 0);
        bsp::vTaskDelay(20);
        bsp::gpio_set_level(LCD_RST, 1);
        bsp::vTaskDelay(120);
        // RDDID: the ILI9341 answers zeros, the ST7789V its ID.
        let mut id = [0u8; 3];
        let _ = bsp::esp_lcd_panel_io_rx_param(io, 0x04, id.as_mut_ptr().cast(), id.len());
        let ili = id == [0, 0, 0];
        info!("lcd: ID {:02x}{:02x}{:02x} → {}", id[0], id[1], id[2], if ili { "ILI9341" } else { "ST7789V" });

        // Both speak the MIPI DCS commands esp_lcd's ST7789 driver uses.
        let mut cfg: bsp::esp_lcd_panel_dev_config_t = core::mem::zeroed();
        cfg.reset_gpio_num = -1;
        cfg.__bindgen_anon_1.rgb_ele_order = if ili {
            bsp::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_BGR
        } else {
            bsp::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_RGB
        };
        cfg.bits_per_pixel = 16;
        let mut panel = core::ptr::null_mut();
        esp!(bsp::esp_lcd_new_panel_st7789(io, &cfg, &mut panel))?;
        esp!(bsp::esp_lcd_panel_init(panel))?;
        for (cmd, params) in if ili { ILI9341 } else { ST7789V } {
            esp!(bsp::esp_lcd_panel_io_tx_param(io, *cmd as i32, params.as_ptr().cast(), params.len()))?;
        }
        // Landscape as in the example: MV (+ MX on the ST7789V).
        esp!(bsp::esp_lcd_panel_swap_xy(panel, true))?;
        esp!(bsp::esp_lcd_panel_mirror(panel, !ili, false))?;
        SpiLcd::finish(panel, io, done, LCD_WIDTH, LCD_HEIGHT, bg565)?
    };
    spi_lcd::backlight_init(BACKLIGHT, true)?;
    Ok(lcd)
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

/// No power switch under software control: backlight off and deep sleep;
/// BOOT (GPIO0, an RTC pin) or EN wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, BOOT or EN wakes");
    set_backlight(0);
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

pub fn poll_buttons() -> Vec<Button> {
    key::poll(&KEY)
}

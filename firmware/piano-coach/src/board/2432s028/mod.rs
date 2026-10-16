//! ESP32-2432S028R ("Cheap Yellow Display", schematic V0.1 2022) —
//! ESP32-WROOM-32 (ESP32 rev 3, dual Xtensa 240 MHz, 4 MB flash, no PSRAM),
//! 2.8″ 320×240 ILI9341 on SPI, XPT2046 resistive touch, BOOT key (GPIO0),
//! RGB LED, microSD, SC8002B speaker amplifier on the DAC (GPIO26), light
//! sensor (GPIO34), CH340 USB console.
//!
//! LCD: SCK 14, MOSI 13, MISO 12, CS 15, D/C 2 (HSPI's own IOMUX pins:
//! 40 MHz), reset tied to the chip's EN (software reset here), backlight
//! GPIO21 (active high). Touch: CLK 25, DIN 32, DOUT 39, CS 33, PENIRQ 36
//! (bit-banged, `board::xpt2046`). RGB LED: GPIO4 / 16 / 17, active low —
//! kept off. microSD (CS 5, MOSI 23, CLK 18, MISO 19) and the speaker are
//! not used yet: songs come from the flash song pack (as on the
//! ESP-WROVER-KIT: no PSRAM, so the same song limits), the piano over BLE.
//!
//! The UI is drawn at 1.25× (the minimal UI's bar layout is designed for
//! 170 rows; this panel has 240).

use std::sync::Mutex;

use esp_idf_svc::sys::{bsp, esp};
use log::info;

use super::key::{self, Key};
use super::spi_lcd::{self, Pins, SpiLcd};
use super::xpt2046::{self, Xpt2046};
use super::{Button, Caps};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "ESP32-2432S028",
    backlight: true,
    power_latch: false,
    usb_midi: false,
    ble_midi: true,
    buttons: true,
    wake_hint: "BOOT or RST",
    key_hint: "tap ›  2× ‹  hold OK",
    touch: true,
};

pub const LCD_WIDTH: usize = 320;
pub const LCD_HEIGHT: usize = 240;
/// Logical UI pixels per panel pixel: 256×192 logical.
pub const UI_SCALE: f32 = 1.25;
pub const ROUND: bool = false;
/// The panel's orientation is the same on every unit (so far one).
pub const LCD_MIRROR: Option<fn(&SpiLcd, bool)> = None;

const BOOT_KEY: i32 = 0;
const BACKLIGHT: i32 = 21;
const RGB_LED: [i32; 3] = [4, 16, 17];

/// Touch calibration, measured on the bench unit (taps at the four corners
/// and the centre, extrapolated to the edges). In this orientation (USB
/// connector on the left) the controller's Y channel runs along the screen's
/// x axis and its X channel along y, both falling: raw value at screen
/// x = 0 / 320 and at y = 0 / 240.
const TOUCH_X: (i32, i32) = (3750, 230);
const TOUCH_Y: (i32, i32) = (3795, 340);

static KEY: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));
static TOUCH: Mutex<Option<Xpt2046>> = Mutex::new(None);

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
    // The RGB LED's cathodes sit on GPIOs: drive them high (off).
    unsafe {
        let mut c: bsp::gpio_config_t = core::mem::zeroed();
        c.pin_bit_mask = RGB_LED.iter().fold(0u64, |m, &g| m | 1u64 << g);
        c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
        bsp::gpio_config(&c);
        for g in RGB_LED {
            bsp::gpio_set_level(g, 1);
        }
    }
    info!("board: {}", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn lcd_init(bg565: u16) -> anyhow::Result<SpiLcd> {
    let pins = Pins {
        host: bsp::spi_host_device_t_SPI2_HOST,
        sclk: 14,
        mosi: 13,
        miso: 12,
        cs: 15,
        dc: 2,
        // HSPI's IOMUX pins: no GPIO-matrix limit.
        pclk_hz: 40_000_000,
        max_transfer: LCD_WIDTH * 10 * 2,
    };
    let (io, done) = spi_lcd::open_io(&pins)?;
    let lcd = unsafe {
        // Both speak the MIPI DCS commands esp_lcd's ST7789 driver uses.
        let mut cfg: bsp::esp_lcd_panel_dev_config_t = core::mem::zeroed();
        cfg.reset_gpio_num = -1;
        cfg.__bindgen_anon_1.rgb_ele_order = bsp::lcd_rgb_element_order_t_LCD_RGB_ELEMENT_ORDER_BGR;
        cfg.bits_per_pixel = 16;
        let mut panel = core::ptr::null_mut();
        esp!(bsp::esp_lcd_new_panel_st7789(io, &cfg, &mut panel))?;
        // No reset line (it is the chip's EN): software reset.
        esp!(bsp::esp_lcd_panel_reset(panel))?;
        bsp::vTaskDelay(120);
        // RDDID: the ILI9341 answers zeros, an ST7789 (some later runs of
        // this board) its ID.
        let mut id = [0u8; 3];
        let _ = bsp::esp_lcd_panel_io_rx_param(io, 0x04, id.as_mut_ptr().cast(), id.len());
        let ili = id == [0, 0, 0];
        info!("lcd: ID {:02x}{:02x}{:02x} → {}", id[0], id[1], id[2], if ili { "ILI9341" } else { "ST7789" });
        esp!(bsp::esp_lcd_panel_init(panel))?;
        for (cmd, params) in if ili { ILI9341 } else { ST7789V } {
            esp!(bsp::esp_lcd_panel_io_tx_param(io, *cmd as i32, params.as_ptr().cast(), params.len()))?;
        }
        // Landscape, USB connector on the left: MV + MY.
        esp!(bsp::esp_lcd_panel_swap_xy(panel, true))?;
        esp!(bsp::esp_lcd_panel_mirror(panel, false, true))?;
        SpiLcd::finish(panel, io, done, LCD_WIDTH, LCD_HEIGHT, bg565)?
    };
    spi_lcd::backlight_init(BACKLIGHT, false)?;
    *TOUCH.lock().unwrap() = Some(Xpt2046::open(xpt2046::Pins {
        clk: 25,
        mosi: 32,
        miso: 39,
        cs: 33,
        irq: 36,
    }));
    Ok(lcd)
}

pub fn lcd_draw(lcd: &SpiLcd, x: usize, y: usize, w: usize, h: usize, px: &mut [u16]) {
    lcd.draw(x, y, w, h, px);
}

/// Finger (or stylus) position in panel pixels.
pub fn touch_point() -> Option<(u16, u16)> {
    let (rx, ry) = TOUCH.lock().unwrap().as_ref()?.read()?;
    let map = |v: i32, (at0, atn): (i32, i32), n: usize| ((v - at0) * n as i32 / (atn - at0)).clamp(0, n as i32 - 1) as u16;
    let p = (map(ry, TOUCH_X, LCD_WIDTH), map(rx, TOUCH_Y, LCD_HEIGHT));
    log::debug!("touch: raw {rx} {ry} → {p:?}");
    Some(p)
}

pub fn set_backlight(percent: i32) {
    spi_lcd::backlight_set(percent);
}

/// No power switch under software control: backlight off and deep sleep;
/// BOOT (GPIO0, an RTC pin) or RST wakes it.
pub fn power_off() -> ! {
    info!("power off: deep sleep, BOOT or RST wakes");
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

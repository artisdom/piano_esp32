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
    touch: true,
};

pub const LCD_WIDTH: usize = 240;
pub const LCD_HEIGHT: usize = 240;
pub const ROUND: bool = true;

const BOOT_KEY: i32 = 9;
const BACKLIGHT: i32 = 3;
const TP_SDA: i32 = 4;
const TP_SCL: i32 = 5;
const TP_RST: i32 = 1;
const TP_INT: i32 = 0;
const TP_ADDR: u16 = 0x15;

static KEY: Mutex<Key> = Mutex::new(Key::new(BOOT_KEY));

/// The CST816D on its I2C bus (None until `lcd_init`, or if it is absent).
struct Touch(bsp::i2c_master_dev_handle_t);
unsafe impl Send for Touch {}
static TOUCH: Mutex<Option<Touch>> = Mutex::new(None);

pub fn early_init() {
    info!("board: {}", CAPS.name);
}

pub fn init_power(_transport: Transport) {}

pub fn lcd_init(bg565: u16) -> anyhow::Result<SpiLcd> {
    let pins = Pins {
        host: bsp::spi_host_device_t_SPI2_HOST,
        sclk: 6,
        mosi: 7,
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
    spi_lcd::backlight_init(BACKLIGHT)?;
    match touch_init() {
        Ok(t) => *TOUCH.lock().unwrap() = Some(t),
        Err(e) => warn!("touch: CST816D not found ({e}) — BOOT key only"),
    }
    Ok(lcd)
}

fn touch_init() -> anyhow::Result<Touch> {
    unsafe {
        // Reset pulse, then give the controller time to boot.
        for (pin, level, ms) in [(TP_RST, 0, 10), (TP_RST, 1, 60)] {
            if level == 0 {
                let mut c: bsp::gpio_config_t = core::mem::zeroed();
                c.pin_bit_mask = 1u64 << pin;
                c.mode = bsp::gpio_mode_t_GPIO_MODE_OUTPUT;
                bsp::gpio_config(&c);
            }
            bsp::gpio_set_level(pin, level);
            bsp::vTaskDelay(ms);
        }
        let mut bc: bsp::i2c_master_bus_config_t = core::mem::zeroed();
        bc.i2c_port = 0;
        bc.sda_io_num = TP_SDA;
        bc.scl_io_num = TP_SCL;
        bc.__bindgen_anon_1.clk_source = bsp::soc_periph_i2c_clk_src_t_I2C_CLK_SRC_DEFAULT;
        bc.glitch_ignore_cnt = 7;
        bc.flags.set_enable_internal_pullup(1);
        let mut bus = core::ptr::null_mut();
        esp!(bsp::i2c_new_master_bus(&bc, &mut bus))?;
        let mut dc: bsp::i2c_device_config_t = core::mem::zeroed();
        dc.dev_addr_length = bsp::i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7;
        dc.device_address = TP_ADDR;
        dc.scl_speed_hz = 400_000;
        let mut dev = core::ptr::null_mut();
        esp!(bsp::i2c_master_bus_add_device(bus, &dc, &mut dev))?;
        // Keep it awake (it stops answering in its auto low-power mode).
        esp!(bsp::i2c_master_transmit(dev, [0xFEu8, 0xFF].as_ptr(), 2, 50))?;
        info!("touch: CST816D at 0x{TP_ADDR:02x}");
        Ok(Touch(dev))
    }
}

pub fn lcd_draw(lcd: &SpiLcd, x: usize, y: usize, w: usize, h: usize, px: &mut [u16]) {
    lcd.draw(x, y, w, h, px);
}

/// Finger position in panel pixels. Registers 0x02.. = finger count, X
/// high/low, Y high/low.
pub fn touch_point() -> Option<(u16, u16)> {
    let guard = TOUCH.lock().unwrap();
    let t = guard.as_ref()?;
    let mut b = [0u8; 5];
    let r = unsafe { bsp::i2c_master_transmit_receive(t.0, [0x02u8].as_ptr(), 1, b.as_mut_ptr(), b.len(), 20) };
    if r != 0 || b[0] & 0x0f == 0 {
        return None;
    }
    let x = ((b[1] as u16 & 0x0f) << 8) | b[2] as u16;
    let y = ((b[3] as u16 & 0x0f) << 8) | b[4] as u16;
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

//! M5Stack Tab5 bring-up: system I2C recovery, the two PI4IOE5V6408 IO
//! expanders that gate the board rails, microSD, and the MIPI-DSI panel with
//! its touch controller (ST7123 integrated display+touch on current boards,
//! ILI9881C + GT911 on early ones — the BSP probes which).
//!
//! Ported from the herdline Tab5 client (~/w/rust/herdline/clients/tab5,
//! src/board/tab5/*), which documents the failure behind each step.
//!
//! Board quirks: the panel is portrait, so the UI renders rotated 270°; BLE
//! goes through the ESP32-C6 (ESP-Hosted over SDIO), which cannot run
//! together with the USB-HS host, so only one MIDI transport runs per boot.

use std::time::Duration;

use esp_idf_svc::sys::bsp::{
    self, i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7, i2c_device_config_t, i2c_master_dev_handle_t,
};
use esp_idf_svc::sys::esp;
use log::{info, warn};
use slint::platform::software_renderer::RenderingRotation;
use slint::LogicalPosition;

use super::{adopt, Button, Caps, Display, MSYNC_DIR_C2M, SD_MOUNT};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "M5Stack Tab5",
    backlight: true,
    power_latch: true,
    usb_midi: true,
    ble_midi: true,
    buttons: false,
    wake_hint: "the side power button",
    key_hint: "",
    touch: true,
};

/// Physical (portrait) scanout geometry.
pub const FRAME_WIDTH: usize = 720;
pub const FRAME_HEIGHT: usize = 1280;
/// The UI is landscape; the renderer rotates into the portrait framebuffer.
pub const WINDOW_WIDTH: u32 = 1280;
pub const WINDOW_HEIGHT: u32 = 720;
pub const UI_SCALE: f32 = 1.0;
pub const RENDERING_ROTATION: RenderingRotation = RenderingRotation::Rotate270;
/// ES8388 + NS4150B (mono speaker).
pub const AUDIO_OUT_RATE: u32 = 24_000;
pub const AUDIO_CHANNELS: u32 = 1;

/// Rotated 270°: landscape x runs up the portrait panel (panel y = 1279 − x),
/// landscape y runs across it (panel x = y). Same mapping as touch.
#[inline]
pub fn fb_index(x: usize, y: usize) -> usize {
    (FRAME_HEIGHT - 1 - x) * FRAME_WIDTH + y
}

pub fn fb_rows(x: usize, _y: usize, w: usize, _h: usize) -> (usize, usize) {
    (FRAME_HEIGHT.saturating_sub(x + w), FRAME_HEIGHT - x.min(FRAME_HEIGHT))
}

pub fn touch_to_logical(x: u16, y: u16) -> LogicalPosition {
    LogicalPosition::new((FRAME_HEIGHT as f32 - 1.0) - y as f32, x as f32)
}

/// NS4150B enable is on IO expander 0x43 P1.
pub fn speaker_enable(on: bool) {
    unsafe {
        let _ = bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_SPEAKER, on);
    }
}

pub fn poll_buttons() -> Vec<Button> {
    Vec::new()
}

/// Power the USB-A port (5 V via the expander) and install the host stack.
pub fn usb_host_start() -> anyhow::Result<()> {
    unsafe {
        esp!(bsp::bsp_usb_host_start(
            bsp::bsp_usb_host_power_mode_t_BSP_USB_HOST_POWER_MODE_USB_DEV,
            false,
        ))?;
    }
    Ok(())
}

/// The BLE controller lives on the ESP32-C6: connect ESP-Hosted over SDIO
/// (the C6 was powered by `init_power`), then start the controller by RPC.
/// Needs C6 slave firmware with BT (factory 0.0.0 has none).
pub fn bt_controller_start(status: &Status) -> anyhow::Result<()> {
    std::thread::sleep(Duration::from_millis(600)); // let the C6 boot
    unsafe {
        let rc = bsp::esp_hosted_connect_to_slave();
        anyhow::ensure!(rc == 0, "ESP-Hosted cannot reach the C6 (rc {rc})");
        let mut v: bsp::esp_hosted_coprocessor_fwver_t = core::mem::zeroed();
        if bsp::esp_hosted_get_coprocessor_fwversion(&mut v) == 0 {
            info!("bt: C6 ESP-Hosted firmware {}.{}.{}", v.major1, v.minor1, v.patch1);
            if v.major1 == 0 && v.minor1 == 0 && v.patch1 == 0 {
                warn!("bt: factory C6 firmware 0.0.0 has no hosted Bluetooth");
            }
        }
        *status.name.lock().unwrap() = "starting C6 Bluetooth…".into();
        esp!(bsp::esp_hosted_bt_controller_init())
            .map_err(|e| anyhow::anyhow!("C6 Bluetooth unavailable (update its ESP-Hosted firmware): {e}"))?;
        esp!(bsp::esp_hosted_bt_controller_enable())
            .map_err(|e| anyhow::anyhow!("C6 Bluetooth enable failed: {e}"))?;
    }
    Ok(())
}

/// Clock a wedged system I2C bus free (SCL 32 / SDA 31) and create both
/// expander handles exactly once: the driver's one-time reset clears every
/// output, and the expanders keep their state across reboots.
pub fn early_init() {
    unsafe { i2c_bus_clear() };
    unsafe {
        for attempt in 0..12 {
            if !bsp::bsp_io_expander_init().is_null() && !bsp::bsp_io_expander1_init().is_null() {
                return;
            }
            warn!("expander probe failed (attempt {attempt}); retrying");
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    warn!("IO expanders never answered — power rails uncontrolled");
}

const REG_IO_DIR: u8 = 0x03;
const REG_OUT_SET: u8 = 0x05;
const REG_OUT_HIGH_Z: u8 = 0x07;
const REG_IN_DEF_STA: u8 = 0x09;
const REG_PULL_EN: u8 = 0x0B;
const REG_PULL_SEL: u8 = 0x0D;
const REG_INT_MASK: u8 = 0x11;

fn expander_dev(addr: u16) -> Option<i2c_master_dev_handle_t> {
    unsafe {
        let bus = bsp::bsp_i2c_get_handle();
        if bus.is_null() {
            return None;
        }
        let cfg = i2c_device_config_t {
            dev_addr_length: i2c_addr_bit_len_t_I2C_ADDR_BIT_LEN_7,
            device_address: addr,
            scl_speed_hz: 400_000,
            ..Default::default()
        };
        let mut d: i2c_master_dev_handle_t = core::ptr::null_mut();
        (bsp::i2c_master_bus_add_device(bus, &cfg, &mut d) == 0 && !d.is_null()).then_some(d)
    }
}

fn wr(d: i2c_master_dev_handle_t, reg: u8, val: u8) -> bool {
    let buf = [reg, val];
    unsafe { bsp::i2c_master_transmit(d, buf.as_ptr(), buf.len(), 100) == 0 }
}

/// Program both expanders (register values from M5Tab5-UserDemo).
///
/// 0x43: P0 ext antenna, P1 speaker amp, P2 EXT5V, P4 LCD_RST, P5 TP_RST,
///       P6 CAM_RST, P7 headphone detect (in).
/// 0x44: P0 WLAN_PWR_EN, P3 USB5V_EN, P4 power-off latch, P5 QC_CHG_EN
///       (active low), P6 USB-C detect (in), P7 CHG_EN.
///
/// Per transport (only one runs per boot — the USB-HS host and the C6's
/// SDIO link interfere): USB mode powers the USB-A 5 V rail (the piano is
/// bus powered and detects the host by VBUS) and leaves the C6 off; BLE mode
/// powers the C6 and leaves USB-A off. Speaker amp off, charger on.
pub fn init_power(transport: Transport) {
    let usb = transport == Transport::Usb;
    match expander_dev(0x43) {
        Some(d) => {
            let ok = wr(d, REG_IO_DIR, 0b0111_1111)
                && wr(d, REG_OUT_HIGH_Z, 0)
                && wr(d, REG_PULL_SEL, 0b0111_1111)
                && wr(d, REG_PULL_EN, 0b0111_1111)
                && wr(d, REG_OUT_SET, 0b0111_0000); // LCD/TP/CAM out of reset
            if !ok {
                warn!("expander 0x43 programming failed");
            }
        }
        None => warn!("expander 0x43 unreachable"),
    }
    match expander_dev(0x44) {
        Some(d) => {
            let ok = wr(d, REG_IO_DIR, 0b1011_1001)
                && wr(d, REG_OUT_HIGH_Z, 0b0000_0110)
                && wr(d, REG_PULL_SEL, 0b1011_1001)
                && wr(d, REG_PULL_EN, 0b1011_1001)
                && wr(d, REG_IN_DEF_STA, 0b0100_0000)
                && wr(d, REG_INT_MASK, 0b1011_1111)
                // CHG_EN + (USB5V_EN | WLAN_PWR_EN)
                && wr(d, REG_OUT_SET, if usb { 0b1000_1000 } else { 0b1000_0001 });
            if ok {
                info!(
                    "expanders: charger on, {}",
                    if usb { "USB-A 5V on, C6 off" } else { "C6 on, USB-A off" }
                );
            } else {
                warn!("expander 0x44 programming failed");
            }
        }
        None => warn!("expander 0x44 unreachable"),
    }
}

pub fn mount_sd() -> bool {
    let err = unsafe { bsp::bsp_sdcard_mount() };
    if err == 0 {
        info!("microSD mounted at {SD_MOUNT}");
        true
    } else {
        warn!("no microSD (mount error {err})");
        false
    }
}

unsafe fn i2c_bus_clear() {
    use esp_idf_svc::sys::{
        esp_rom_delay_us, gpio_get_level, gpio_mode_t_GPIO_MODE_INPUT_OUTPUT_OD, gpio_reset_pin,
        gpio_set_direction, gpio_set_level,
    };
    const SCL: i32 = 32;
    const SDA: i32 = 31;
    gpio_reset_pin(SCL);
    gpio_reset_pin(SDA);
    gpio_set_direction(SCL, gpio_mode_t_GPIO_MODE_INPUT_OUTPUT_OD);
    gpio_set_direction(SDA, gpio_mode_t_GPIO_MODE_INPUT_OUTPUT_OD);
    gpio_set_level(SCL, 1);
    gpio_set_level(SDA, 1);
    esp_rom_delay_us(10);
    if gpio_get_level(SDA) == 0 {
        for _ in 0..16 {
            gpio_set_level(SCL, 0);
            esp_rom_delay_us(10);
            gpio_set_level(SCL, 1);
            esp_rom_delay_us(10);
            if gpio_get_level(SDA) == 1 {
                break;
            }
        }
        gpio_set_level(SDA, 0);
        esp_rom_delay_us(10);
        gpio_set_level(SDA, 1);
        esp_rom_delay_us(10);
        warn!("i2c: bus was wedged — clocked out + STOP");
    }
    gpio_reset_pin(SCL);
    gpio_reset_pin(SDA);
}

pub fn display_init(bg565: u16) -> anyhow::Result<Display> {
    unsafe {
        esp!(bsp::bsp_i2c_init())?;
        // Power-cycle the LCD rail: a warm reset can leave the panel half
        // initialised and the DSI link will not re-train over that.
        esp!(bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_LCD, false))?;
        std::thread::sleep(Duration::from_millis(400));
        esp!(bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_LCD, true))?;
        std::thread::sleep(Duration::from_millis(250));

        let mut panel = core::ptr::null_mut();
        let mut io = core::ptr::null_mut();
        let mut cfg: bsp::bsp_display_config_t = core::mem::zeroed();
        cfg.dsi_bus.lane_bit_rate_mbps = 1000;
        esp!(bsp::bsp_display_new(&cfg, &mut panel, &mut io))?;
        esp!(bsp::esp_lcd_panel_disp_on_off(panel, true))?;
        esp!(bsp::bsp_display_brightness_init())?;
        esp!(bsp::bsp_display_brightness_set(80))?;

        let mut touch = core::ptr::null_mut();
        esp!(bsp::bsp_touch_new(core::ptr::null(), &mut touch))?;

        let mut fb0: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut fb1: *mut core::ffi::c_void = core::ptr::null_mut();
        esp!(bsp::esp_lcd_dpi_panel_get_frame_buffer(panel, 2, &mut fb0, &mut fb1))?;
        adopt(panel, touch, fb0, fb1, bg565, "MIPI-DSI")
    }
}

/// Write back rows `y0..y1` of `fb[back]` and switch scanout to it.
pub fn present(d: &Display, back: usize, y0: usize, y1: usize) -> anyhow::Result<()> {
    // esp_cache_msync needs 64-byte alignment; one row is 1440 B, two are
    // 2880 B (= 45 × 64), so round the band out to even rows.
    let y0 = y0 & !1;
    let y1 = ((y1 + 1) & !1).min(FRAME_HEIGHT);
    let ptr = d.fb[back];
    unsafe {
        esp!(bsp::esp_cache_msync(
            ptr.add(y0 * FRAME_WIDTH) as *mut core::ffi::c_void,
            (y1 - y0) * FRAME_WIDTH * 2,
            MSYNC_DIR_C2M,
        ))?;
        esp!(bsp::esp_lcd_panel_draw_bitmap(
            d.panel,
            0,
            0,
            FRAME_WIDTH as i32,
            FRAME_HEIGHT as i32,
            ptr as *const core::ffi::c_void,
        ))?;
    }
    Ok(())
}

pub fn set_backlight(percent: i32) {
    unsafe {
        let _ = bsp::bsp_display_brightness_set(percent.clamp(0, 100));
    }
}

/// True power-off through the Tab5's hardware power latch: pulsing
/// POWEROFF_GEN (expander 0x44 P4, "PWROFF_PULSE" in the schematic, per
/// M5Tab5-UserDemo `bsp_generate_poweroff_signal` and herdline) drops the
/// power path; the side power button turns the Tab5 back on. If we are
/// still running afterwards (an external supply holds the rail up), park
/// in deep sleep with the screen, USB-A and C6 off — the reset or power
/// button restarts it.
pub fn power_off() -> ! {
    info!("power off: pulsing the Tab5 power latch");
    unsafe {
        set_backlight(0);
        let _ = bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_LCD, false);
        let _ = bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_USB, false);
        let _ = bsp::bsp_feature_enable(bsp::bsp_feature_t_BSP_FEATURE_WIFI, false);
        std::thread::sleep(Duration::from_millis(120));
        let exp1 = bsp::bsp_io_expander1_init();
        if !exp1.is_null() {
            const POWEROFF_GEN: u32 = 1 << 4;
            for _ in 0..3 {
                let _ = bsp::esp_io_expander_set_level(exp1, POWEROFF_GEN, 1);
                std::thread::sleep(Duration::from_millis(100));
                let _ = bsp::esp_io_expander_set_level(exp1, POWEROFF_GEN, 0);
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        warn!("power off: still powered (external supply) — entering deep sleep");
        std::thread::sleep(Duration::from_millis(50));
        esp_idf_svc::sys::esp_deep_sleep_start();
    }
}

pub fn reboot() -> ! {
    unsafe { esp_idf_svc::sys::esp_restart() }
}

//! ESP32-S31-Korvo-1 V1.1 — ESP32-S31-WROOM-3 (16 MB flash, 16 MB PSRAM),
//! ESP32-S3-LCD-EV-Board-SUB3 sub-board (800×480 parallel RGB, GT1151
//! touch), ES8389 codec with two NS4150B speaker amps, microSD, a USB 2.0 HS
//! Type-A host port with an always-on 500 mA switch, native BLE, and a
//! four-key ADC button ladder (VOL+, VOL−, PLAY/MODE, SET).
//!
//! Everything hangs off the Korvo BSP in components/esp32_s31_korvo (the
//! herdline project's no-LVGL fork of the esp-dev-kits component); the board
//! bring-up facts come from herdline's validated port
//! (~/w/rust/herdline/clients/tab5/src/board/esp32_s31_korvo_1).
//!
//! Board quirks: no backlight control on SUB3, no power latch (shut down =
//! deep sleep, reset to wake), no rotation; the 1280-wide UI is drawn at
//! 0.625 scale (1280×768 logical on the 800×480 panel).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Once;

use esp_idf_svc::sys::{bsp, esp};
use log::{info, warn};
use slint::platform::software_renderer::RenderingRotation;
use slint::LogicalPosition;

use super::{adopt, Button, Caps, Display};
use crate::link::{Status, Transport};

pub const CAPS: Caps = Caps {
    name: "ESP32-S31-Korvo-1",
    backlight: false,
    power_latch: false,
    usb_midi: true,
    ble_midi: true,
    buttons: true,
    touch: true,
};

pub const FRAME_WIDTH: usize = 800;
pub const FRAME_HEIGHT: usize = 480;
pub const WINDOW_WIDTH: u32 = 800;
pub const WINDOW_HEIGHT: u32 = 480;
/// 1280 logical pixels across the 800-pixel panel (logical height 768).
pub const UI_SCALE: f32 = 0.625;
pub const RENDERING_ROTATION: RenderingRotation = RenderingRotation::NoRotation;
/// ES8389 in the BSP's default 48 kHz stereo I2S setup; both speakers get the
/// mono synth.
pub const AUDIO_OUT_RATE: u32 = 48_000;
pub const AUDIO_CHANNELS: u32 = 2;

#[inline]
pub fn fb_index(x: usize, y: usize) -> usize {
    y * FRAME_WIDTH + x
}

pub fn fb_rows(_x: usize, y: usize, _w: usize, h: usize) -> (usize, usize) {
    (y.min(FRAME_HEIGHT), (y + h).min(FRAME_HEIGHT))
}

pub fn touch_to_logical(x: u16, y: u16) -> LogicalPosition {
    log::debug!("touch raw ({x}, {y})");
    LogicalPosition::new(x as f32 / UI_SCALE, y as f32 / UI_SCALE)
}

/// Keep internal SRAM for the BLE controller, USB and DMA: application
/// thread stacks (the UI's is 96 KB) go to PSRAM. With PSRAM-XIP the cache
/// stays available during flash operations, so that is safe here.
pub fn early_init() {
    unsafe {
        let mut config = esp_idf_svc::sys::esp_pthread_get_default_config();
        config.stack_alloc_caps = esp_idf_svc::sys::MALLOC_CAP_SPIRAM | esp_idf_svc::sys::MALLOC_CAP_8BIT;
        config.inherit_cfg = true;
        if let Err(e) = esp!(esp_idf_svc::sys::esp_pthread_set_cfg(&config)) {
            warn!("board: cannot move thread stacks to PSRAM: {e}");
        }
    }
    info!("board: {} — application thread stacks in PSRAM", CAPS.name);
}

/// No switchable rails: USB VBUS is always on, the radio is on-chip.
pub fn init_power(_transport: Transport) {}

pub fn mount_sd() -> bool {
    // NULL config = BSP defaults: 4-bit SDMMC, mounted at /sdcard.
    let err = unsafe { bsp::bsp_sdcard_mount(core::ptr::null(), core::ptr::null_mut()) };
    if err == 0 {
        info!("microSD mounted at /sdcard");
        true
    } else {
        warn!("no microSD (mount error {err})");
        false
    }
}

pub fn display_init(bg565: u16) -> anyhow::Result<Display> {
    unsafe {
        // NULL config takes the BSP defaults and resets the panel.
        let mut panel = core::ptr::null_mut();
        esp!(bsp::bsp_display_new(core::ptr::null(), &mut panel))?;
        let mut touch = core::ptr::null_mut();
        esp!(bsp::bsp_touch_new(bsp::bsp_display_rotation_t_BSP_DISPLAY_ROTATE_0, &mut touch))?;
        let mut fb0: *mut core::ffi::c_void = core::ptr::null_mut();
        let mut fb1: *mut core::ffi::c_void = core::ptr::null_mut();
        esp!(bsp::esp_lcd_rgb_panel_get_frame_buffer(panel, 2, &mut fb0, &mut fb1))?;
        adopt(panel, touch, fb0, fb1, bg565, "parallel RGB")
    }
}

/// The RGB driver switches scanout when handed one of its own framebuffers
/// and does the cache writeback for the band itself.
pub fn present(d: &Display, back: usize, y0: usize, y1: usize) -> anyhow::Result<()> {
    unsafe {
        esp!(bsp::bsp_display_present(y0 as i32, y1 as i32, d.fb[back] as *const core::ffi::c_void))?;
    }
    Ok(())
}

/// SUB3 has no backlight-control GPIO: only on/off.
pub fn set_backlight(percent: i32) {
    unsafe {
        let _ = if percent <= 0 {
            bsp::bsp_display_backlight_off()
        } else {
            bsp::bsp_display_backlight_on()
        };
    }
}

/// No power latch: screen off and deep sleep; the reset button restarts.
pub fn power_off() -> ! {
    info!("power off: Korvo has no power latch — display off, deep sleep");
    set_backlight(0);
    unsafe { esp_idf_svc::sys::esp_deep_sleep_start() }
}

pub fn reboot() -> ! {
    unsafe { esp_idf_svc::sys::esp_restart() }
}

/// The BSP installs the host stack on its own task pinned to one core
/// (install, IRQs and teardown on the same core — herdline found cross-core
/// teardown racing a root-port IRQ).
pub fn usb_host_start() -> anyhow::Result<()> {
    unsafe { esp!(bsp::bsp_usb_host_start())? };
    Ok(())
}

/// The ES8389 driver switches the PA pin (GPIO7) with the codec.
pub fn speaker_enable(_on: bool) {}

/// Native controller: `nimble_port_init` brings it up itself.
pub fn bt_controller_start(_status: &Status) -> anyhow::Result<()> {
    Ok(())
}

// ---- buttons ----------------------------------------------------------------

/// Pending clicks, one bit per BSP button index (set from the button task).
static PENDING: AtomicU8 = AtomicU8::new(0);
static BUTTONS: Once = Once::new();

unsafe extern "C" fn on_click(_handle: *mut core::ffi::c_void, user: *mut core::ffi::c_void) {
    PENDING.fetch_or(1 << (user as usize & 7), Ordering::Relaxed);
}

fn start_buttons() {
    unsafe {
        let mut handles: [bsp::button_handle_t; bsp::bsp_button_t_BSP_BUTTON_NUM as usize] =
            [core::ptr::null_mut(); bsp::bsp_button_t_BSP_BUTTON_NUM as usize];
        let mut n: i32 = 0;
        if bsp::bsp_iot_button_create(handles.as_mut_ptr(), &mut n, handles.len() as i32) != 0 {
            warn!("buttons: ADC ladder init failed");
            return;
        }
        for (i, h) in handles.iter().take(n.max(0) as usize).enumerate() {
            let _ = bsp::iot_button_register_cb(
                *h,
                bsp::button_event_t_BUTTON_SINGLE_CLICK,
                core::ptr::null_mut(),
                Some(on_click),
                i as *mut core::ffi::c_void,
            );
        }
        info!("buttons: {n} keys on the ADC ladder");
    }
}

/// Clicks since the last call. VOL+ / VOL− / PLAY (BSP "MODE") / SET.
pub fn poll_buttons() -> Vec<Button> {
    BUTTONS.call_once(start_buttons);
    let bits = PENDING.swap(0, Ordering::Relaxed);
    let mut v = Vec::new();
    for (idx, b) in [
        (bsp::bsp_button_t_BSP_BUTTON_VOLUP, Button::VolumeUp),
        (bsp::bsp_button_t_BSP_BUTTON_VOLDOWN, Button::VolumeDown),
        (bsp::bsp_button_t_BSP_BUTTON_MODE, Button::PlayPause),
        (bsp::bsp_button_t_BSP_BUTTON_SET, Button::Select),
    ] {
        if bits & (1 << idx) != 0 {
            v.push(b);
        }
    }
    v
}

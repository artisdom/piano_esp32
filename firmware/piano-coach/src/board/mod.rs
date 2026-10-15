//! The board boundary.
//!
//! Everything the application needs from hardware goes through this module;
//! nothing outside `src/board/` may name a board or a BSP-specific call. A
//! board is one directory, `src/board/<board>/mod.rs`, plus its build profile
//! `boards/<board>/` (see boards/README.md). That module provides the items
//! re-exported below, and `contract.rs` pins their signatures, so an
//! incomplete port fails to compile here instead of somewhere in the UI.
//!
//! Conventions:
//! * "Logical" coordinates are Slint's (the 1280-wide design). The board's
//!   `UI_SCALE` maps them to physical landscape pixels.
//! * "Physical" (landscape) pixels are the panel seen the way the UI is laid
//!   out; [`fb_index`] maps them into the shadow framebuffer, whose rows are
//!   what [`present`] publishes (rotated boards differ here).

#[cfg(feature = "board-tab5")]
#[path = "tab5/mod.rs"]
mod imp;

#[cfg(feature = "board-korvo")]
#[path = "korvo/mod.rs"]
mod imp;

mod contract;

use std::sync::atomic::AtomicBool;

use esp_idf_svc::sys::bsp;
use log::info;
use slint::platform::software_renderer::Rgb565Pixel;
use slint::LogicalPosition;

pub use imp::{
    bt_controller_start, display_init, early_init, fb_index, fb_rows, init_power, mount_sd,
    poll_buttons, power_off, present, reboot, set_backlight, speaker_enable, touch_to_logical,
    usb_host_start,
    AUDIO_CHANNELS, AUDIO_OUT_RATE, CAPS, FRAME_HEIGHT, FRAME_WIDTH, RENDERING_ROTATION, UI_SCALE,
    WINDOW_HEIGHT, WINDOW_WIDTH,
};

/// What a board has; the UI adapts to these instead of naming boards.
pub struct Caps {
    /// Shown in Settings and logs.
    pub name: &'static str,
    /// Display brightness is adjustable.
    pub backlight: bool,
    /// A true power-off exists (else shut down = deep sleep, reset to wake).
    pub power_latch: bool,
    /// Can reach the piano over USB host / Bluetooth LE.
    pub usb_midi: bool,
    pub ble_midi: bool,
    /// Physical buttons reported by [`poll_buttons`].
    pub buttons: bool,
}

/// Board buttons, as far as a board has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    PlayPause,
    Select,
    VolumeUp,
    VolumeDown,
}

pub const SD_MOUNT: &str = "/sdcard";

/// Set once the display (and with it any shared I2C bus) is up; the audio
/// codec waits for it.
pub static DISPLAY_UP: AtomicBool = AtomicBool::new(false);

/// Panel + touch handles and the two scanout buffers.
pub struct Display {
    pub panel: bsp::esp_lcd_panel_handle_t,
    pub touch: bsp::esp_lcd_touch_handle_t,
    pub fb: [*mut Rgb565Pixel; 2],
}

/// Fill both framebuffers with the background and wrap the handles (called
/// by each board's `display_init`).
pub(crate) unsafe fn adopt(
    panel: bsp::esp_lcd_panel_handle_t,
    touch: bsp::esp_lcd_touch_handle_t,
    fb0: *mut core::ffi::c_void,
    fb1: *mut core::ffi::c_void,
    bg565: u16,
    kind: &str,
) -> anyhow::Result<Display> {
    anyhow::ensure!(!fb0.is_null() && !fb1.is_null(), "BSP did not allocate two framebuffers");
    for &fb in &[fb0, fb1] {
        let px = fb as *mut u16;
        for i in 0..FRAME_WIDTH * FRAME_HEIGHT {
            *px.add(i) = bg565;
        }
        let _ = bsp::esp_cache_msync(fb, FRAME_WIDTH * FRAME_HEIGHT * 2, MSYNC_DIR_C2M);
    }
    info!("display up: {FRAME_WIDTH}x{FRAME_HEIGHT} RGB565 ({kind}), fb0={fb0:p} fb1={fb1:p}");
    DISPLAY_UP.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(Display {
        panel,
        touch,
        fb: [fb0.cast(), fb1.cast()],
    })
}

/// `ESP_CACHE_MSYNC_FLAG_DIR_C2M` (a macro bindgen cannot emit).
pub(crate) const MSYNC_DIR_C2M: core::ffi::c_int = 1 << 2;

/// Current touch point in logical coordinates.
pub fn poll_touch(touch: bsp::esp_lcd_touch_handle_t) -> Option<LogicalPosition> {
    unsafe {
        bsp::esp_lcd_touch_read_data(touch);
        let (mut x, mut y, mut count) = (0u16, 0u16, 0u8);
        let pressed = bsp::esp_lcd_touch_get_coordinates(
            touch,
            &mut x,
            &mut y,
            core::ptr::null_mut(),
            &mut count,
            1,
        );
        if !pressed || count == 0 {
            return None;
        }
        Some(touch_to_logical(x, y))
    }
}

/// (total, free) bytes on the microSD card.
pub fn sd_usage() -> Option<(u64, u64)> {
    let (mut total, mut free) = (0u64, 0u64);
    let r = unsafe { bsp::esp_vfs_fat_info(c"/sdcard".as_ptr(), &mut total, &mut free) };
    (r == 0).then_some((total, free))
}

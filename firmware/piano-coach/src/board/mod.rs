//! The board boundary.
//!
//! Everything the application needs from hardware goes through this module;
//! nothing outside `src/board/` may name a board or a BSP-specific call. A
//! board is one directory, `src/board/<board>/mod.rs`, plus its build profile
//! `boards/<board>/` (see boards/README.md). That module provides the items
//! re-exported below, and `contract.rs` pins their signatures, so an
//! incomplete port fails to compile here instead of somewhere in the UI.
//!
//! Two display classes, chosen by the profile's `UI=`:
//! * `full` (`cfg(ui_full)`): a panel with framebuffers in memory, the
//!   1280-wide UI scaled onto it, touch, microSD, speaker and USB host.
//!   "Logical" coordinates are Slint's (the 1280-wide design); `UI_SCALE`
//!   maps them to physical landscape pixels, and [`fb_index`] maps those into
//!   the shadow framebuffer, whose rows [`present`] publishes.
//! * `mini` (`cfg(ui_mini)`): a small SPI panel written band by band
//!   ([`lcd_init`], [`lcd_draw`]) at its native resolution, optional touch,
//!   one key; songs come from a pack in flash and the piano is reached over
//!   BLE.

#[cfg(feature = "board-tab5")]
#[path = "tab5/mod.rs"]
mod imp;

#[cfg(feature = "board-korvo")]
#[path = "korvo/mod.rs"]
mod imp;

#[cfg(feature = "board-1732s019")]
#[path = "1732s019/mod.rs"]
mod imp;

#[cfg(feature = "board-2424s012")]
#[path = "2424s012/mod.rs"]
mod imp;

#[cfg(feature = "board-tdisplays3")]
#[path = "tdisplays3/mod.rs"]
mod imp;

#[cfg(feature = "board-wroverkit")]
#[path = "wroverkit/mod.rs"]
mod imp;

#[cfg(feature = "board-audiokit")]
#[path = "audiokit/mod.rs"]
mod imp;

mod contract;
#[cfg(ui_mini)]
pub mod cst816;
#[cfg(any(ui_mini, ui_headless))]
pub mod key;
#[cfg(ui_mini)]
pub mod spi_lcd;

#[allow(unused_imports)] // the headless UI has no backlight or shut-down item
pub use imp::{bt_controller_start, early_init, init_power, poll_buttons, power_off, set_backlight, CAPS};
// Only the full UI restarts (transport switch); every board provides it.
#[cfg(ui_full)]
pub use imp::reboot;

#[cfg(has_sd)]
pub use imp::mount_sd;
#[cfg(has_speaker)]
pub use imp::{speaker_codec, speaker_enable, AUDIO_CHANNELS, AUDIO_OUT_RATE, SYNTH_GAIN};

#[cfg(ui_full)]
pub use imp::{
    display_init, fb_index, fb_rows, present, touch_to_logical, usb_host_start, FRAME_HEIGHT, FRAME_WIDTH,
    RENDERING_ROTATION, UI_SCALE, WINDOW_HEIGHT, WINDOW_WIDTH,
};

#[cfg(ui_headless)]
pub use imp::status_led;

#[cfg(ui_mini)]
pub use imp::{lcd_draw, lcd_init, touch_point, LCD_HEIGHT, LCD_WIDTH, ROUND, UI_SCALE as MINI_SCALE};

/// What a board has; the UI adapts to these instead of naming boards.
/// Declarative: each UI class reads only the fields it has a use for.
#[allow(dead_code)]
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
    /// A touch panel (full boards always have one; small boards may only
    /// find out at run time — `touch_point` then returns None).
    pub touch: bool,
    /// How the small boards' keys work, for the menu footer.
    pub key_hint: &'static str,
    /// What wakes the board from its shut-down / sleep.
    pub wake_hint: &'static str,
}

/// Board buttons, as far as a board has them (each board reports a subset).
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    PlayPause,
    Select,
    VolumeUp,
    VolumeDown,
    /// The single user key of the small boards (BOOT): a short press, two
    /// quick presses, a press held ≥ 0.6 s.
    KeyTap,
    KeyDoubleTap,
    KeyHold,
    /// Boards with two keys report menu actions directly.
    NavPrev,
    NavNext,
    NavOk,
    NavBack,
    /// Boards with a row of numbered keys (KEY1…): a press / a hold.
    Keypad(u8),
    KeypadHold(u8),
}

// ---- framebuffer boards ----------------------------------------------------

#[cfg(ui_full)]
pub use full::*;

#[cfg(ui_full)]
mod full {
    use std::sync::atomic::AtomicBool;

    use esp_idf_svc::sys::bsp;
    use log::info;
    use slint::platform::software_renderer::Rgb565Pixel;
    use slint::LogicalPosition;

    use super::{touch_to_logical, FRAME_HEIGHT, FRAME_WIDTH};

    pub const SD_MOUNT: &str = "/sdcard";

    /// Set once the display (and with it any shared I2C bus) is up; the audio
    /// codec waits for it.
    pub static DISPLAY_UP: AtomicBool = AtomicBool::new(false);

    /// Panel + touch handles and the two scanout buffers.
    pub struct Display {
        #[allow(dead_code)] // owned for the program's life; drawn via `fb`
        pub panel: bsp::esp_lcd_panel_handle_t,
        pub touch: bsp::esp_lcd_touch_handle_t,
        pub fb: [*mut Rgb565Pixel; 2],
    }

    /// Fill both framebuffers with the background and wrap the handles
    /// (called by each board's `display_init`).
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
}

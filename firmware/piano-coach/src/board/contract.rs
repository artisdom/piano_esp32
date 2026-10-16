//! Compile-time contract every board module must satisfy. A new board that
//! is missing an item, or has it with another signature, fails here with a
//! precise error. Keep this list in step with `board/mod.rs` and the porting
//! checklist in boards/README.md.

use super::{imp, Button, Caps};
use crate::link::{Status, Transport};

// ---- every board ------------------------------------------------------------

/// What the board has (see [`Caps`]).
const _: Caps = imp::CAPS;
/// Before anything else: bus recovery, memory policy.
const _: fn() = imp::early_init;
/// Power the rails the chosen MIDI transport needs.
const _: fn(Transport) = imp::init_power;
/// 0–100; boards without backlight control ignore it.
const _: fn(i32) = imp::set_backlight;
const _: fn() -> ! = imp::power_off;
const _: fn() -> ! = imp::reboot;
/// Whatever must happen before `nimble_port_init` (coprocessor bring-up).
const _: fn(&Status) -> anyhow::Result<()> = imp::bt_controller_start;
/// Button presses since the last call (empty for boards without buttons).
const _: fn() -> Vec<Button> = imp::poll_buttons;

// ---- HAS=sd / HAS=speaker -----------------------------------------------------

/// Mount the microSD card at /sdcard.
#[cfg(has_sd)]
const _: fn() -> bool = imp::mount_sd;

/// Install the USB host stack and power the host port.
#[cfg(has_usb)]
const _: fn() -> anyhow::Result<()> = imp::usb_host_start;

#[cfg(has_speaker)]
mod speaker {
    use super::super::imp;
    use esp_idf_svc::sys::bsp;

    /// Speaker output: sample rate (a multiple of the synth's 24 kHz) and channels.
    const _: u32 = imp::AUDIO_OUT_RATE;
    const _: u32 = imp::AUDIO_CHANNELS;
    /// Synth master gain for this speaker (the synth's soft clip keeps loud
    /// chords from wrapping): 0.8 is the synth's own default.
    const _: f32 = imp::SYNTH_GAIN;
    /// The speaker's esp_codec_dev handle (codec + I2S up), or None.
    /// Called once, from the audio thread.
    const _: fn() -> Option<bsp::esp_codec_dev_handle_t> = imp::speaker_codec;
    /// Speaker amplifier on/off around playback (no-op where the codec driver
    /// switches it).
    const _: fn(bool) = imp::speaker_enable;
}

// ---- UI=full: framebuffer boards -------------------------------------------

#[cfg(ui_full)]
mod full {
    use slint::platform::software_renderer::RenderingRotation;
    use slint::LogicalPosition;

    use super::super::{imp, Display};

    /// Shadow/scanout framebuffer geometry in pixels (as the panel scans).
    const _: usize = imp::FRAME_WIDTH;
    const _: usize = imp::FRAME_HEIGHT;
    /// Physical landscape size of the UI window (before rotation) and the
    /// logical→physical scale (logical size = WINDOW / UI_SCALE).
    const _: u32 = imp::WINDOW_WIDTH;
    const _: u32 = imp::WINDOW_HEIGHT;
    const _: f32 = imp::UI_SCALE;
    const _: RenderingRotation = imp::RENDERING_ROTATION;
    /// Panel + touch up, both framebuffers filled with the RGB565 background.
    const _: fn(u16) -> anyhow::Result<Display> = imp::display_init;
    /// Publish shadow rows y0..y1 already copied into `fb[back]`.
    const _: fn(&Display, usize, usize, usize) -> anyhow::Result<()> = imp::present;
    /// Raw touch controller coordinates → logical.
    const _: fn(u16, u16) -> LogicalPosition = imp::touch_to_logical;
    /// Physical landscape pixel → shadow framebuffer index.
    const _: fn(usize, usize) -> usize = imp::fb_index;
    /// Shadow rows covering a physical landscape rect (x, y, w, h).
    const _: fn(usize, usize, usize, usize) -> (usize, usize) = imp::fb_rows;
}

// ---- UI=headless: no display --------------------------------------------------

/// The status LED: on / off, and an RGB colour for boards whose LED has
/// one (single-colour LEDs ignore it).
#[cfg(ui_headless)]
const _: fn(bool, [u8; 3]) = imp::status_led;

// ---- UI=mini: small SPI panels ----------------------------------------------

#[cfg(ui_mini)]
mod mini {
    use super::super::{imp, spi_lcd::SpiLcd};

    /// Panel size as the UI sees it (after the board's rotation).
    const _: usize = imp::LCD_WIDTH;
    const _: usize = imp::LCD_HEIGHT;
    /// Round panel: the UI keeps content inside the inscribed circle.
    const _: bool = imp::ROUND;
    /// UI scale (physical / logical pixels): > 1 enlarges the layout on
    /// panels taller than the 170-pixel bar it is designed for.
    const _: f32 = imp::UI_SCALE;
    /// Panel up, cleared to the RGB565 background, backlight on.
    const _: fn(u16) -> anyhow::Result<SpiLcd> = imp::lcd_init;
    /// Write a w×h block of native-endian RGB565 at (x, y); may swap the
    /// buffer's bytes in place. Returns once the buffer may be reused.
    const _: fn(&SpiLcd, usize, usize, usize, usize, &mut [u16]) = imp::lcd_draw;
    /// Current touch point in panel pixels (None: no touch or no panel).
    const _: fn() -> Option<(u16, u16)> = imp::touch_point;
}

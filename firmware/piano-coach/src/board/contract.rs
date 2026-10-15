//! Compile-time contract every board module must satisfy. A new board that
//! is missing an item, or has it with another signature, fails here with a
//! precise error. Keep this list in step with `board/mod.rs` and the porting
//! checklist in boards/README.md.

use slint::platform::software_renderer::RenderingRotation;
use slint::LogicalPosition;

use super::{imp, Button, Caps, Display};
use crate::link::{Status, Transport};

/// What the board has (see [`Caps`]).
const _: Caps = imp::CAPS;

/// Shadow/scanout framebuffer geometry in pixels (as the panel scans).
const _: usize = imp::FRAME_WIDTH;
const _: usize = imp::FRAME_HEIGHT;
/// Physical landscape size of the UI window (before rotation) and the
/// logical→physical scale (logical size = WINDOW / UI_SCALE).
const _: u32 = imp::WINDOW_WIDTH;
const _: u32 = imp::WINDOW_HEIGHT;
const _: f32 = imp::UI_SCALE;
const _: RenderingRotation = imp::RENDERING_ROTATION;
/// Speaker output: sample rate (a multiple of the synth's 24 kHz) and channels.
const _: u32 = imp::AUDIO_OUT_RATE;
const _: u32 = imp::AUDIO_CHANNELS;

/// Before anything else: bus recovery, memory policy.
const _: fn() = imp::early_init;
/// Power the rails the chosen MIDI transport needs.
const _: fn(Transport) = imp::init_power;
/// Mount the microSD card at /sdcard.
const _: fn() -> bool = imp::mount_sd;
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
/// 0–100; boards without backlight control ignore it.
const _: fn(i32) = imp::set_backlight;
const _: fn() -> ! = imp::power_off;
const _: fn() -> ! = imp::reboot;
/// Install the USB host stack and power the host port.
const _: fn() -> anyhow::Result<()> = imp::usb_host_start;
/// Whatever must happen before `nimble_port_init` (coprocessor bring-up).
const _: fn(&Status) -> anyhow::Result<()> = imp::bt_controller_start;
/// Speaker amplifier on/off around playback (no-op where the codec driver
/// switches it).
const _: fn(bool) = imp::speaker_enable;
/// Button presses since the last call (empty for boards without buttons).
const _: fn() -> Vec<Button> = imp::poll_buttons;

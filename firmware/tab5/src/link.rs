//! What the player needs from a MIDI transport (USB host or BLE central):
//! a queue of outgoing messages, a stream of key events, and a status the UI
//! can show. `usb_midi::start` and `ble_midi::start` both return this shape.

use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// A received channel message from the piano.
#[derive(Debug, Clone, Copy)]
pub enum KeyEvent {
    Down { key: u8, vel: u8 },
    Up { key: u8 },
}

/// Connection state shared with the UI.
#[derive(Default)]
pub struct Status {
    pub connected: AtomicBool,
    /// Device name, or what the transport is doing ("Scanning…").
    pub name: Mutex<String>,
    /// Last panel SysEx from the piano (vendor `F0 52 4B pp vv 00 00 F7`).
    pub panel: Mutex<Option<(u8, u8)>>,
}

/// The two transports. Only one runs per boot: on the Tab5 the USB-HS host
/// and the C6's SDIO link (which carries BLE) cannot run together.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Usb,
    Ble,
}

impl Transport {
    pub fn label(self) -> &'static str {
        match self {
            Transport::Usb => "USB",
            Transport::Ble => "Bluetooth",
        }
    }
}

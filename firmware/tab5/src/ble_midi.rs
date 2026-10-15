//! BLE-MIDI central (placeholder until the NimBLE transport lands).
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

use crate::link::{KeyEvent, Status};

pub fn start(_out: Receiver<[u8; 3]>, _keys: Sender<KeyEvent>) -> anyhow::Result<Arc<Status>> {
    anyhow::bail!("Bluetooth transport not built yet")
}

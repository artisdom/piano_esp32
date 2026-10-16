//! Both transports at once, for boards that have a USB host port and their
//! own Bluetooth radio (the YD-ESP32-S3): the USB host client and the BLE
//! central both run, and a router thread in front of them presents one
//! transport to the player.
//!
//! USB wins: while the piano is on the cable, messages go out over USB and
//! key events from BLE are dropped (the piano reports every key on both
//! links when both are up, which would count each note twice). Otherwise
//! BLE carries everything. Plugging or unplugging the cable switches over
//! on the fly; the piano keeps its lit keys, so a song in progress carries
//! on over the other link.

use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use log::{info, warn};

use crate::link::{KeyEvent, Status};
use crate::{ble_midi, usb_midi};

pub fn start(out: Receiver<[u8; 3]>, keys: Sender<KeyEvent>) -> anyhow::Result<Arc<Status>> {
    let (usb_out, usb_out_rx) = channel();
    let (usb_keys_tx, usb_keys) = channel();
    let usb = match usb_midi::start(usb_out_rx, usb_keys_tx) {
        Ok(s) => Some(s),
        Err(e) => {
            warn!("dual: USB host failed to start ({e:#}) — Bluetooth only");
            None
        }
    };
    let (ble_out, ble_out_rx) = channel();
    let (ble_keys_tx, ble_keys) = channel();
    let ble = ble_midi::start(ble_out_rx, ble_keys_tx)?;

    let status = Arc::new(Status::default());
    let st = status.clone();
    std::thread::Builder::new()
        .name("midi-route".into())
        .stack_size(4 * 1024)
        .spawn(move || route(out, keys, usb, usb_out, usb_keys, ble, ble_out, ble_keys, st))?;
    Ok(status)
}

#[allow(clippy::too_many_arguments)]
fn route(
    out: Receiver<[u8; 3]>,
    keys: Sender<KeyEvent>,
    usb: Option<Arc<Status>>,
    usb_out: Sender<[u8; 3]>,
    usb_keys: Receiver<KeyEvent>,
    ble: Arc<Status>,
    ble_out: Sender<[u8; 3]>,
    ble_keys: Receiver<KeyEvent>,
    status: Arc<Status>,
) {
    let mut on_usb = false;
    let mut last_name = String::new();
    loop {
        let usb_up = usb.as_ref().is_some_and(|s| s.connected.load(Ordering::Relaxed));
        let ble_up = ble.connected.load(Ordering::Relaxed);
        if usb_up != on_usb {
            on_usb = usb_up;
            info!("dual: piano now over {}", if on_usb { "USB" } else { "Bluetooth" });
        }

        // Outgoing: wait briefly for the first message (this paces the
        // loop), then pass on whatever else is queued.
        match out.recv_timeout(Duration::from_millis(2)) {
            Ok(m) => {
                let to = if on_usb { &usb_out } else { &ble_out };
                let _ = to.send(m);
                while let Ok(m) = out.try_recv() {
                    let _ = to.send(m);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        while let Ok(k) = usb_keys.try_recv() {
            let _ = keys.send(k);
        }
        while let Ok(k) = ble_keys.try_recv() {
            if !on_usb {
                let _ = keys.send(k);
            }
        }

        // Combined status: the active link's name and panel state.
        let (src, label): (&Status, &str) = match (&usb, on_usb) {
            (Some(u), true) => (u, "USB"),
            _ => (&ble, "Bluetooth"),
        };
        status.connected.store(usb_up || ble_up, Ordering::Relaxed);
        status.usb.store(on_usb, Ordering::Relaxed);
        let name = src.name.lock().unwrap().clone();
        let name = if on_usb || ble_up { format!("{name} ({label})") } else { name };
        if name != last_name {
            *status.name.lock().unwrap() = name.clone();
            last_name = name;
        }
        let panel = *src.panel.lock().unwrap();
        if panel.is_some() {
            *status.panel.lock().unwrap() = panel;
        }
    }
}

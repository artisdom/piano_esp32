//! Piano play & learn firmware for the M5Stack Tab5.
//!
//! Plays MIDI files from the microSD card on the GZUT 88-key LED piano and
//! teaches them: in Learn mode the piano's red key LEDs show the next chord
//! and the song waits until it is played. The piano is reached over USB (the
//! Tab5's USB-A host port) or Bluetooth LE (via the Tab5's ESP32-C6), chosen
//! in Settings; see doc/tab5_firmware.md.
//!
//! Threads: `app` (boot), `ui` (Slint render + touch), `player` (engine
//! clock), and the transport's own task (`usb-midi` or NimBLE host).

mod audio;
mod ble_midi;
mod board;
mod globe;
mod link;
mod player;
mod prefs;
mod store;
mod ui;
mod usb_midi;

use std::sync::mpsc::channel;
use std::sync::Arc;

use link::{Status, Transport};
use log::{error, info};

fn main() {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    // The IDF main task's stack comes from the small pre-scheduler heap:
    // do the real work on a task allocated after startup.
    std::thread::Builder::new()
        .name("app".into())
        .stack_size(24 * 1024)
        .spawn(run_app)
        .expect("spawn app");
}

fn run_app() {
    info!("piano-tab5 {} starting", env!("CARGO_PKG_VERSION"));
    board::early_init();
    prefs::init();
    let transport = match prefs::get_i32("transport") {
        Some(1) => Transport::Ble,
        _ => Transport::Usb,
    };
    board::init_power(transport);
    let sd_ok = board::mount_sd();

    let (out_tx, out_rx) = channel();
    let (key_tx, key_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();

    let status = match transport {
        Transport::Usb => usb_midi::start(out_rx, key_tx),
        Transport::Ble => ble_midi::start(out_rx, key_tx),
    }
    .unwrap_or_else(|e| {
        error!("{} transport failed to start: {e:#}", transport.label());
        let st = Arc::new(Status::default());
        *st.name.lock().unwrap() = format!("error — {e}");
        st
    });

    // Tab5 speaker: synth + codec thread (idle until a note sounds).
    let synth = audio::start();
    player::SOUND.store(prefs::get_i32("sound").unwrap_or(0).clamp(0, 2) as u8, std::sync::atomic::Ordering::Relaxed);
    audio::VOLUME.store(prefs::get_i32("spkvol").unwrap_or(70).clamp(0, 100), std::sync::atomic::Ordering::Relaxed);
    let shared = player::start(cmd_rx, key_rx, out_tx, ui::initial_options(), synth);

    let ctx = ui::Ctx {
        cmds: cmd_tx,
        shared,
        status,
        transport,
        sd_ok,
    };
    std::thread::Builder::new()
        .name("ui".into())
        .stack_size(96 * 1024)
        .spawn(move || {
            if let Err(e) = ui::run(ctx) {
                error!("ui stopped: {e:#}");
            }
        })
        .expect("spawn ui");
}

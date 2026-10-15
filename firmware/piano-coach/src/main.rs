//! Piano Coach — play & learn firmware for the GZUT LED piano on ESP32 dev
//! boards (M5Stack Tab5, ESP32-S31-Korvo-1, ESP32-1732S019, ESP32-2424S012;
//! see src/board/).
//!
//! Plays MIDI songs on the GZUT 88-key LED piano and teaches them: in Learn
//! mode the piano's red key LEDs show the next chord and the song waits
//! until it is played. Large boards (`ui_full`) read the microSD card and
//! reach the piano over USB (their USB-A host port) or Bluetooth LE; small
//! boards (`ui_mini`) keep a song pack in flash and use Bluetooth LE. See
//! doc/piano_coach_firmware.md.
//!
//! Threads: `app` (boot), `ui` (Slint render + input), `player` (engine
//! clock), and the transport's own task (`usb-midi` or NimBLE host).

#[cfg(has_speaker)]
mod audio;
mod ble_midi;
mod board;
#[cfg(ui_full)]
mod globe;
mod link;
mod player;
mod prefs;
mod songs;
mod threads;
#[cfg(ui_full)]
mod store;
#[cfg(ui_full)]
mod ui;
#[cfg(ui_mini)]
mod ui_mini;
#[cfg(ui_headless)]
mod headless;
#[cfg(ui_headless)]
mod web;
#[cfg(has_usb)]
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
    info!("piano-coach {} on {}", env!("CARGO_PKG_VERSION"), board::CAPS.name);
    board::early_init();
    prefs::init();
    let transport = if board::CAPS.usb_midi && prefs::get_i32("transport") != Some(1) {
        Transport::Usb
    } else {
        Transport::Ble
    };
    board::init_power(transport);
    #[cfg(has_sd)]
    let sd_ok = board::mount_sd();

    let (out_tx, out_rx) = channel();
    let (key_tx, key_rx) = channel();
    let (cmd_tx, cmd_rx) = channel();

    let started = match transport {
        #[cfg(has_usb)]
        Transport::Usb => usb_midi::start(out_rx, key_tx),
        _ => ble_midi::start(out_rx, key_tx),
    };
    let status = started.unwrap_or_else(|e| {
        error!("{} transport failed to start: {e:#}", transport.label());
        let st = Arc::new(Status::default());
        *st.name.lock().unwrap() = format!("error — {e}");
        st
    });

    // Speaker: synth + codec thread (idle until a note sounds).
    #[cfg(has_speaker)]
    let synth = {
        let synth = audio::start();
        player::SOUND.store(prefs::get_i32("sound").unwrap_or(0).clamp(0, 2) as u8, std::sync::atomic::Ordering::Relaxed);
        audio::VOLUME.store(prefs::get_i32("spkvol").unwrap_or(70).clamp(0, 100), std::sync::atomic::Ordering::Relaxed);
        Some(synth)
    };
    #[cfg(not(has_speaker))]
    let synth = None;

    #[cfg(ui_full)]
    {
        let shared = player::start(cmd_rx, key_rx, out_tx, player::saved_options(), synth, None);
        let ctx = ui::Ctx {
            cmds: cmd_tx,
            shared,
            status,
            transport,
            sd_ok,
        };
        spawn_ui(96 * 1024, move || ui::run(ctx));
    }
    #[cfg(ui_headless)]
    {
        let (nav_tx, nav_rx) = channel();
        let shared = player::start(cmd_rx, key_rx, out_tx, player::saved_options(), synth.clone(), Some(nav_tx));
        let ctx = headless::Ctx {
            cmds: cmd_tx,
            shared,
            status,
            nav: nav_rx,
            synth,
            sd_ok,
        };
        spawn_ui(10 * 1024, move || headless::run(ctx));
    }
    #[cfg(ui_mini)]
    {
        let (nav_tx, nav_rx) = channel();
        let shared = player::start(cmd_rx, key_rx, out_tx, player::saved_options(), synth, Some(nav_tx));
        let ctx = ui_mini::Ctx {
            cmds: cmd_tx,
            shared,
            status,
            nav: nav_rx,
        };
        spawn_ui(ui_mini::STACK, move || ui_mini::run(ctx));
    }
}

fn spawn_ui(stack: usize, f: impl FnOnce() -> anyhow::Result<()> + Send + 'static) {
    std::thread::Builder::new()
        .name("ui".into())
        .stack_size(stack)
        .spawn(move || {
            if let Err(e) = f() {
                error!("ui stopped: {e:#}");
            }
        })
        .expect("spawn ui");
}

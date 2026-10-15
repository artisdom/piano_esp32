//! Slint UI on the Tab5's DPI framebuffer: platform glue, the render/touch
//! loop, and the view-model wiring for the Library, Play and Settings pages.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::info;
use piano_core::engine::{Mode, Options, State};
use piano_core::library::{self, Entry};
use piano_core::song::{is_black, key_name, Hand, Song, KEY_HI, KEY_LO};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, PhysicalSize, SharedString, VecModel};

use crate::board::{self, FRAME_HEIGHT, FRAME_WIDTH};
use crate::link::{Status, Transport};
use crate::player::{self, Cmd, Shared};
use crate::prefs;

slint::include_modules!();

const H_RES: usize = FRAME_WIDTH;
const V_RES: usize = FRAME_HEIGHT;
/// Theme.bg #0e1116 as RGB565.
const BG565: u16 = 0x0882;

// Must match ui/player.slint `Geometry`.
const KEYS_W: f32 = 1248.0;
const ROLL_H: f32 = 300.0;
/// Song milliseconds visible above the "now" line.
const LOOKAHEAD_MS: f32 = 2600.0;
const NOW_LINE: f32 = ROLL_H - 22.0;
const SCROLL_GAIN: f32 = 2.0;

struct EmbeddedPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start: Instant,
}

impl Platform for EmbeddedPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.window.clone())
    }
    fn duration_since_start(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Key geometry along the keyboard: (x, width) per MIDI key.
fn key_geometry() -> Vec<(u8, f32, f32)> {
    let whites = (KEY_LO..=KEY_HI).filter(|&k| !is_black(k)).count() as f32;
    let ww = KEYS_W / whites;
    let bw = ww * 0.6;
    let mut out = Vec::new();
    let mut wi = 0f32;
    for k in KEY_LO..=KEY_HI {
        if is_black(k) {
            out.push((k, wi * ww - bw / 2.0, bw));
        } else {
            out.push((k, wi * ww, ww));
            wi += 1.0;
        }
    }
    out
}

struct Lib {
    root: PathBuf,
    stack: Vec<(String, String)>, // (fs name, title)
    entries: Vec<Entry>,
}

impl Lib {
    fn dir(&self) -> PathBuf {
        let mut p = self.root.clone();
        for (n, _) in &self.stack {
            p.push(n);
        }
        p
    }
    fn reload(&mut self) -> Result<(), String> {
        let dir = self.dir();
        let rd = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut v = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e
                .file_type()
                .map(|t| t.is_dir())
                .or_else(|_| e.metadata().map(|m| m.is_dir()))
                .unwrap_or(false);
            v.push(library::entry(&name, is_dir));
        }
        self.entries = library::sort_entries(v);
        Ok(())
    }
}

fn library_root() -> PathBuf {
    for p in ["/sdcard/midi", "/sdcard/MIDI", "/sdcard"] {
        if std::fs::metadata(p).map(|m| m.is_dir()).unwrap_or(false) {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("/sdcard")
}

fn push_library(ui: &AppWindow, lib: &Lib, sd_ok: bool, err: Option<String>) {
    let vm = ui.global::<LibraryVM>();
    let mut crumbs: Vec<SharedString> = vec!["Library".into()];
    crumbs.extend(lib.stack.iter().map(|(_, t)| SharedString::from(t.as_str())));
    vm.set_path_text(
        if lib.stack.is_empty() {
            format!("{} · {} items", lib.root.display(), lib.entries.len())
        } else {
            format!("{} items", lib.entries.len())
        }
        .into(),
    );
    vm.set_crumbs(ModelRc::new(VecModel::from(crumbs)));
    let rows: Vec<LibEntry> = lib
        .entries
        .iter()
        .map(|e| LibEntry {
            title: e.title.as_str().into(),
            subtitle: e.subtitle.as_str().into(),
            is_dir: e.is_dir,
        })
        .collect();
    vm.set_entries(ModelRc::new(VecModel::from(rows)));
    vm.set_selected(-1);
    vm.set_empty_text(
        match (sd_ok, err) {
            (false, _) => "No microSD card. Copy your MIDI files to a FAT32/exFAT card (a `midi` folder is used if present) and insert it.".to_string(),
            (true, Some(e)) => format!("Cannot read folder: {e}"),
            (true, None) => "No MIDI files here.".to_string(),
        }
        .into(),
    );
}

fn sd_label() -> String {
    match board::sd_usage() {
        Some((t, f)) => format!("{:.1} GB free of {:.1} GB", f as f64 / 1e9, t as f64 / 1e9),
        None => "mounted".into(),
    }
}

fn song_info(s: &Song) -> String {
    let split = match s.split {
        piano_core::song::SplitRule::TrackNames => "hands from track names",
        piano_core::song::SplitRule::TrackPitch => "hands from tracks",
        piano_core::song::SplitRule::MiddleC => "hands split at middle C",
    };
    let right = s.notes.iter().filter(|n| n.hand == Hand::Right).count();
    format!(
        "{} · {:.0} bpm · {} notes ({} right, {} left) · {}",
        library::fmt_time(s.duration_ms),
        s.bpm,
        s.notes.len(),
        right,
        s.notes.len() - right,
        split
    )
}

pub struct Ctx {
    pub cmds: Sender<Cmd>,
    pub shared: Shared,
    pub status: Arc<Status>,
    pub transport: Transport,
    pub sd_ok: bool,
}

fn current_options(ui: &AppWindow) -> Options {
    let p = ui.global::<PlayerVM>();
    let s = ui.global::<SettingsVM>();
    Options {
        mode: match p.get_mode() {
            0 => Mode::Listen,
            2 => Mode::Follow,
            _ => Mode::Learn,
        },
        hands: player::hands_from(p.get_hands()),
        tempo_pct: p.get_tempo().clamp(25, 200) as u16,
        accompany: s.get_accompany(),
        easy: s.get_easy(),
        led_off_on_hit: s.get_led_off_on_hit(),
        lead_ms: s.get_lead().round() as u32,
        hint_vel: 1,
    }
}

pub fn initial_options() -> Options {
    Options {
        mode: match prefs::get_i32("mode") {
            Some(0) => Mode::Listen,
            Some(2) => Mode::Follow,
            _ => Mode::Learn,
        },
        hands: player::hands_from(prefs::get_i32("hands").unwrap_or(0)),
        tempo_pct: prefs::get_i32("tempo").unwrap_or(100).clamp(25, 200) as u16,
        accompany: prefs::get_bool("accompany", true),
        easy: prefs::get_bool("easy", false),
        led_off_on_hit: prefs::get_bool("ledoff", true),
        lead_ms: prefs::get_i32("lead").unwrap_or(700).clamp(200, 1500) as u32,
        hint_vel: 1,
    }
}

pub fn run(ctx: Ctx) -> anyhow::Result<()> {
    let display = board::display_init(BG565)?;
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(EmbeddedPlatform {
        window: window.clone(),
        start: Instant::now(),
    }))
    .map_err(|e| anyhow::anyhow!("set_platform: {e:?}"))?;
    let ui = AppWindow::new().map_err(|e| anyhow::anyhow!("slint ui: {e}"))?;

    // ---- initial state ----------------------------------------------------
    let o = initial_options();
    {
        let p = ui.global::<PlayerVM>();
        p.set_mode(match o.mode {
            Mode::Listen => 0,
            Mode::Learn => 1,
            Mode::Follow => 2,
        });
        p.set_hands(match o.hands {
            piano_core::song::HandSel::Both => 0,
            piano_core::song::HandSel::Right => 1,
            piano_core::song::HandSel::Left => 2,
        });
        p.set_tempo(o.tempo_pct as i32);
        let s = ui.global::<SettingsVM>();
        s.set_accompany(o.accompany);
        s.set_easy(o.easy);
        s.set_led_off_on_hit(o.led_off_on_hit);
        s.set_lead(o.lead_ms as f32);
        let br = prefs::get_i32("brightness").unwrap_or(80).clamp(10, 100);
        s.set_brightness(br as f32);
        board::set_backlight(br);
        s.set_transport(match ctx.transport {
            Transport::Usb => 0,
            Transport::Ble => 1,
        });
        let a = ui.global::<AppVM>();
        a.set_version(format!("piano-tab5 {}", env!("CARGO_PKG_VERSION")).into());
        a.set_transport(ctx.transport.label().into());
        a.set_sd_status(if ctx.sd_ok { sd_label() } else { "no card".into() }.into());
    }

    let sd_ok = Rc::new(Cell::new(ctx.sd_ok));
    let geometry = key_geometry();
    let lib = Rc::new(RefCell::new(Lib {
        root: library_root(),
        stack: Vec::new(),
        entries: Vec::new(),
    }));
    ui.global::<SettingsVM>()
        .set_library_root(lib.borrow().root.display().to_string().into());
    {
        let err = lib.borrow_mut().reload().err();
        push_library(&ui, &lib.borrow(), sd_ok.get(), err);
    }
    let toast: Rc<RefCell<Option<Instant>>> = Rc::new(RefCell::new(None));
    let show_toast = {
        let ui = ui.as_weak();
        let toast = toast.clone();
        move |msg: &str| {
            if let Some(ui) = ui.upgrade() {
                ui.global::<AppVM>().set_toast(msg.into());
                *toast.borrow_mut() = Some(Instant::now());
            }
        }
    };

    // ---- callbacks: library ----------------------------------------------
    {
        let vm = ui.global::<LibraryVM>();
        let (l, w, c, sd) = (lib.clone(), ui.as_weak(), ctx.cmds.clone(), sd_ok.clone());
        vm.on_open(move |i| {
            let ui = w.unwrap();
            let entry = l.borrow().entries.get(i as usize).cloned();
            let Some(e) = entry else { return };
            if e.is_dir {
                l.borrow_mut().stack.push((e.name.clone(), e.title.clone()));
                let err = l.borrow_mut().reload().err();
                push_library(&ui, &l.borrow(), sd.get(), err);
            } else {
                let vm = ui.global::<LibraryVM>();
                vm.set_selected(i);
                vm.set_sel_title(e.title.as_str().into());
                vm.set_sel_subtitle(e.subtitle.as_str().into());
                vm.set_sel_info("Loading…".into());
                vm.set_sel_error("".into());
                vm.set_sel_ready(false);
                let _ = c.send(Cmd::Load(l.borrow().dir().join(&e.name)));
            }
        });
        let (l, w, sd) = (lib.clone(), ui.as_weak(), sd_ok.clone());
        vm.on_up(move || {
            let ui = w.unwrap();
            l.borrow_mut().stack.pop();
            let err = l.borrow_mut().reload().err();
            push_library(&ui, &l.borrow(), sd.get(), err);
        });
        let (l, w, sd) = (lib.clone(), ui.as_weak(), sd_ok.clone());
        vm.on_crumb(move |i| {
            let ui = w.unwrap();
            l.borrow_mut().stack.truncate(i.max(0) as usize);
            let err = l.borrow_mut().reload().err();
            push_library(&ui, &l.borrow(), sd.get(), err);
        });
        let (w, c) = (ui.as_weak(), ctx.cmds.clone());
        vm.on_start(move |mode| {
            let ui = w.unwrap();
            ui.global::<PlayerVM>().set_mode(mode);
            prefs::set_i32("mode", mode);
            let _ = c.send(Cmd::SetOptions(current_options(&ui)));
            let _ = c.send(Cmd::Restart);
            ui.global::<AppVM>().set_page(1);
        });
    }

    // ---- callbacks: player -----------------------------------------------
    {
        let vm = ui.global::<PlayerVM>();
        let c = ctx.cmds.clone();
        vm.on_play_pause(move || {
            let _ = c.send(Cmd::PlayPause);
        });
        let c = ctx.cmds.clone();
        vm.on_stop(move || {
            let _ = c.send(Cmd::Stop);
        });
        let c = ctx.cmds.clone();
        vm.on_restart(move || {
            let _ = c.send(Cmd::Restart);
        });
        let c = ctx.cmds.clone();
        vm.on_seek(move |f| {
            let _ = c.send(Cmd::Seek(f));
        });
        let (w, c) = (ui.as_weak(), ctx.cmds.clone());
        vm.on_set_mode(move |m| {
            let ui = w.unwrap();
            ui.global::<PlayerVM>().set_mode(m);
            prefs::set_i32("mode", m);
            let _ = c.send(Cmd::SetOptions(current_options(&ui)));
        });
        let (w, c) = (ui.as_weak(), ctx.cmds.clone());
        vm.on_set_hands(move |h| {
            let ui = w.unwrap();
            ui.global::<PlayerVM>().set_hands(h);
            prefs::set_i32("hands", h);
            let _ = c.send(Cmd::SetOptions(current_options(&ui)));
        });
        let (w, c) = (ui.as_weak(), ctx.cmds.clone());
        vm.on_tempo_step(move |d| {
            let ui = w.unwrap();
            let p = ui.global::<PlayerVM>();
            let t = (p.get_tempo() + d).clamp(30, 200);
            p.set_tempo(t);
            prefs::set_i32("tempo", t);
            let _ = c.send(Cmd::Tempo(t as u16));
        });
    }

    // ---- callbacks: settings ---------------------------------------------
    {
        let vm = ui.global::<SettingsVM>();
        vm.on_set_brightness(|v| {
            let v = v.round() as i32;
            board::set_backlight(v);
            prefs::set_i32("brightness", v);
        });
        let opt_cb = |key: &'static str| {
            let (w, c) = (ui.as_weak(), ctx.cmds.clone());
            move |v: bool| {
                prefs::set_bool(key, v);
                let _ = c.send(Cmd::SetOptions(current_options(&w.unwrap())));
            }
        };
        vm.on_set_accompany(opt_cb("accompany"));
        vm.on_set_easy(opt_cb("easy"));
        vm.on_set_led_off_on_hit(opt_cb("ledoff"));
        let (w, c) = (ui.as_weak(), ctx.cmds.clone());
        vm.on_set_lead(move |v| {
            prefs::set_i32("lead", v.round() as i32);
            let _ = c.send(Cmd::SetOptions(current_options(&w.unwrap())));
        });
        let c = ctx.cmds.clone();
        vm.on_led_test(move || {
            let _ = c.send(Cmd::LedTest);
        });
        let c = ctx.cmds.clone();
        vm.on_all_off(move || {
            let _ = c.send(Cmd::AllOff);
        });
        let (l, w, sd) = (lib.clone(), ui.as_weak(), sd_ok.clone());
        let st = show_toast.clone();
        vm.on_rescan(move || {
            let ui = w.unwrap();
            // A card inserted after boot: mount it now.
            if !sd.get() && board::mount_sd() {
                sd.set(true);
                ui.global::<AppVM>().set_sd_status(sd_label().into());
                st("microSD card mounted");
            } else if !sd.get() {
                st("No microSD card found");
            }
            ui.global::<SettingsVM>().set_library_root(library_root().display().to_string().into());
            let mut lb = l.borrow_mut();
            lb.root = library_root();
            lb.stack.clear();
            let err = lb.reload().err();
            push_library(&ui, &lb, sd.get(), err);
        });
        let current = ctx.transport;
        let st = show_toast.clone();
        vm.on_set_transport(move |t| {
            let want = if t == 1 { Transport::Ble } else { Transport::Usb };
            if want == current {
                return;
            }
            prefs::set_i32("transport", t);
            st(&format!("Switching to {} — restarting…", want.label()));
            // Let the toast paint, then reboot into the other transport.
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(900));
                board::reboot();
            });
        });
    }

    // ---- show -------------------------------------------------------------
    ui.show().map_err(|e| anyhow::anyhow!("slint show: {e}"))?;
    window.set_size(PhysicalSize::new(board::WINDOW_WIDTH, board::WINDOW_HEIGHT));
    window.request_redraw();
    info!("ui: up ({} transport)", ctx.transport.label());

    let keys_model: Rc<VecModel<KeyView>> = Rc::new(VecModel::default());
    ui.global::<PlayerVM>().set_keys(keys_model.clone().into());
    let roll_model: Rc<VecModel<RollNote>> = Rc::new(VecModel::default());
    ui.global::<PlayerVM>().set_roll(roll_model.clone().into());
    ui.global::<PlayerVM>().set_now_line(NOW_LINE);
    // Whites first so blacks paint on top.
    let order: Vec<usize> = (0..geometry.len())
        .filter(|&i| !is_black(geometry[i].0))
        .chain((0..geometry.len()).filter(|&i| is_black(geometry[i].0)))
        .collect();
    let mut key_states: Vec<i32> = vec![-1; geometry.len()];

    let mut last_rev = u64::MAX;
    let mut last_path: Option<PathBuf> = None;
    let mut last_state = State::Stopped;
    let mut last_connected: Option<bool> = None;
    let mut last_name = String::new();
    let mut last_panel: Option<(u8, u8)> = None;
    let mut last_touch: Option<LogicalPosition> = None;
    let mut press_origin: Option<LogicalPosition> = None;
    let mut back = 1usize;
    let mut shadow: Vec<Rgb565Pixel> = vec![Rgb565Pixel(BG565); H_RES * V_RES];
    let mut prev_band: (usize, usize) = (0, V_RES);

    unsafe {
        let _ = esp_idf_svc::sys::esp_task_wdt_add(core::ptr::null_mut());
    }
    loop {
        unsafe {
            let _ = esp_idf_svc::sys::esp_task_wdt_reset();
        }
        slint::platform::update_timers_and_animations();

        // ---- transport status ----
        let connected = ctx.status.connected.load(std::sync::atomic::Ordering::Relaxed);
        let name = ctx.status.name.lock().unwrap().clone();
        if last_connected != Some(connected) || name != last_name {
            let a = ui.global::<AppVM>();
            a.set_connected(connected);
            a.set_device(
                if connected {
                    format!("{} · {}", name, ctx.transport.label())
                } else if name.is_empty() {
                    format!("{}: piano not connected", ctx.transport.label())
                } else {
                    format!("{}: {}", ctx.transport.label(), name)
                }
                .into(),
            );
            if last_connected.is_some() {
                show_toast(if connected { "Piano connected" } else { "Piano disconnected" });
            }
            last_connected = Some(connected);
            last_name = name;
        }
        let panel = *ctx.status.panel.lock().unwrap();
        if panel != last_panel {
            last_panel = panel;
            if let Some((p, v)) = panel {
                let txt = match p {
                    0x2a => format!("volume {v}"),
                    0x15 => format!("tone {v}"),
                    0x2b => format!("metronome {v}"),
                    _ => format!("param {p:#04x} = {v}"),
                };
                ui.global::<SettingsVM>().set_panel(txt.into());
            }
        }
        if toast.borrow().is_some_and(|t| t.elapsed() > Duration::from_millis(2500)) {
            *toast.borrow_mut() = None;
            ui.global::<AppVM>().set_toast("".into());
        }

        // ---- player snapshot ----
        let snap = {
            let s = ctx.shared.lock().unwrap();
            if s.rev == last_rev {
                None
            } else {
                Some(s.clone())
            }
        };
        if let Some(s) = snap {
            last_rev = s.rev;
            let p = ui.global::<PlayerVM>();
            if s.path != last_path {
                last_path = s.path.clone();
                let lvm = ui.global::<LibraryVM>();
                match (&s.song, &s.load_error) {
                    (Some(song), _) => {
                        lvm.set_sel_info(song_info(song).into());
                        lvm.set_sel_ready(true);
                        p.set_loaded(true);
                        p.set_title(lvm.get_sel_title());
                        p.set_subtitle(lvm.get_sel_subtitle());
                        p.set_dur_text(library::fmt_time(song.duration_ms).into());
                    }
                    (None, Some(err)) => {
                        lvm.set_sel_error(format!("Cannot play this file: {err}").into());
                        lvm.set_sel_ready(false);
                    }
                    _ => {}
                }
            }
            p.set_state(match s.state {
                State::Stopped => 0,
                State::Playing => 1,
                State::Waiting => 2,
                State::Paused => 3,
                State::Finished => 4,
            });
            if s.state == State::Finished && last_state != State::Finished && s.opts.mode != Mode::Listen {
                show_toast(&format!(
                    "Finished — {}% accuracy, best streak {}",
                    s.stats.accuracy(),
                    s.stats.best_streak
                ));
            }
            last_state = s.state;
            p.set_pos_text(library::fmt_time(s.pos_ms).into());
            let dur = s.song.as_ref().map(|x| x.duration_ms).unwrap_or(0).max(1);
            p.set_progress((s.pos_ms as f32 / dur as f32).min(1.0));
            let st = s.stats;
            p.set_accuracy(format!("{}%", st.accuracy()).into());
            p.set_steps(
                match s.opts.mode {
                    Mode::Learn => format!("{}/{}", st.steps_done, st.steps_total),
                    _ => format!("{}/{}", st.hits, st.steps_total),
                }
                .into(),
            );
            p.set_streak(st.streak.to_string().into());
            p.set_wrong(st.wrong.to_string().into());
            p.set_prompt(
                if s.state == State::Waiting && !s.waiting.is_empty() {
                    let names: Vec<String> = s.waiting.iter().map(|&k| key_name(k)).collect();
                    format!("Play  {}", names.join(" · "))
                } else if s.state == State::Paused {
                    "Paused".into()
                } else if s.state == State::Finished {
                    "Well done!".into()
                } else {
                    String::new()
                }
                .into(),
            );

            // Keyboard.
            let wrong_key = s.wrong.map(|w| w.0);
            for (row, &gi) in order.iter().enumerate() {
                let (k, x, w) = geometry[gi];
                let ku = k as usize;
                let state = (s.led[ku] as i32)
                    | ((s.pressed[ku] as i32) << 1)
                    | (((wrong_key == Some(k)) as i32) << 2)
                    | ((s.hint[ku] as i32) << 3);
                if key_states[row] != state {
                    key_states[row] = state;
                    let kv = KeyView {
                        x,
                        w,
                        black: is_black(k),
                        state,
                        label: if k % 12 == 0 { key_name(k).into() } else { SharedString::new() },
                    };
                    if keys_model.row_count() > row {
                        keys_model.set_row_data(row, kv);
                    } else {
                        keys_model.push(kv);
                    }
                }
            }

            // Falling notes.
            let mut rows: Vec<RollNote> = Vec::new();
            if let Some(song) = &s.song {
                let pos = s.pos_ms as f32;
                let ppm = NOW_LINE / LOOKAHEAD_MS;
                let first = song.notes.partition_point(|n| (n.start_ms as f32) < pos - 10_000.0);
                let learner = s.opts.mode != Mode::Listen;
                for n in song.notes[first..].iter() {
                    if n.start_ms as f32 > pos + LOOKAHEAD_MS {
                        break;
                    }
                    if (n.end_ms as f32) < pos - 400.0 {
                        continue;
                    }
                    let gi = (n.key - KEY_LO) as usize;
                    let (_, x, w) = geometry[gi];
                    let y_bottom = NOW_LINE - (n.start_ms as f32 - pos) * ppm;
                    let y_top = NOW_LINE - (n.end_ms as f32 - pos) * ppm;
                    let mine = learner && s.opts.hands.includes(n.hand);
                    let kind = if mine
                        && s.step.as_ref().is_some_and(|st| st.ms == n.start_ms && st.keys.contains(&n.key))
                    {
                        2
                    } else if mine && (n.start_ms as f32) < pos - 1.0 {
                        3
                    } else if n.hand == Hand::Right {
                        0
                    } else {
                        1
                    };
                    rows.push(RollNote {
                        x: x + 1.0,
                        y: y_top,
                        w: (w - 2.0).max(3.0),
                        h: (y_bottom - y_top).max(4.0),
                        kind,
                    });
                }
            }
            roll_model.set_vec(rows);
        }

        // ---- touch ----
        match (board::poll_touch(display.touch), last_touch) {
            (Some(pos), None) => {
                window.dispatch_event(WindowEvent::PointerPressed {
                    position: pos,
                    button: PointerEventButton::Left,
                });
                press_origin = Some(pos);
                last_touch = Some(pos);
            }
            (Some(pos), Some(_)) => {
                let o = press_origin.unwrap_or(pos);
                let scaled = LogicalPosition::new(
                    o.x + (pos.x - o.x) * SCROLL_GAIN,
                    o.y + (pos.y - o.y) * SCROLL_GAIN,
                );
                window.dispatch_event(WindowEvent::PointerMoved { position: scaled });
                last_touch = Some(scaled);
            }
            (None, Some(pos)) => {
                window.dispatch_event(WindowEvent::PointerReleased {
                    position: pos,
                    button: PointerEventButton::Left,
                });
                window.dispatch_event(WindowEvent::PointerExited);
                last_touch = None;
                press_origin = None;
            }
            (None, None) => {}
        }

        // ---- render: shadow buffer (partial), copy dirty band, present ----
        let mut band: Option<(usize, usize)> = None;
        let redrew = window.draw_if_needed(|renderer| {
            renderer.set_rendering_rotation(
                slint::platform::software_renderer::RenderingRotation::Rotate270,
            );
            let region = renderer.render(shadow.as_mut_slice(), H_RES);
            let o = region.bounding_box_origin();
            let sz = region.bounding_box_size();
            if sz.height > 0 && sz.width > 0 {
                let y0 = o.y.max(0) as usize;
                band = Some((y0, (y0 + sz.height as usize).min(V_RES)));
            }
        });
        if let (true, Some((y0, y1))) = (redrew, band) {
            let (u0, u1) = (y0.min(prev_band.0), y1.max(prev_band.1).min(V_RES));
            unsafe {
                core::ptr::copy_nonoverlapping(
                    shadow.as_ptr().add(u0 * H_RES),
                    display.fb[back].add(u0 * H_RES),
                    (u1 - u0) * H_RES,
                );
            }
            board::present(&display, back, u0, u1)?;
            prev_band = (y0, y1);
            back ^= 1;
        }
        std::thread::sleep(Duration::from_millis(if redrew || window.has_active_animations() { 4 } else { 12 }));
    }
}

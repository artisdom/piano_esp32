//! Player thread: owns the [`Engine`], runs its clock at ~1 kHz, routes key
//! events from the piano into it and its MIDI output to the transport, and
//! publishes a snapshot the UI reads every frame.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{info, warn};
use piano_core::controls::{self, Control};
use piano_core::engine::{Engine, Msg, Options, State, Stats, ALL_NOTES_OFF};
use piano_core::song::{HandSel, Song, Step, KEY_HI, KEY_LO};

use crate::link::KeyEvent;
use crate::prefs;

/// The speaker synth shared with the audio thread (boards with a speaker).
pub type SharedSynth = Arc<Mutex<piano_core::synth::Synth>>;

/// How long a control key's LED stays lit after a press (feedback).
const CONTROL_FLASH: Duration = Duration::from_millis(600);

/// Where played notes sound (Settings → "Notes sound from").
pub static SOUND: AtomicU8 = AtomicU8::new(SOUND_PIANO);
pub const SOUND_PIANO: u8 = 0;
pub const SOUND_SPEAKER: u8 = 1;
pub const SOUND_BOTH: u8 = 2;

/// Send one engine message to the piano and/or the board's speaker synth.
///
/// Hint LEDs (Note On velocity 1) only ever go to the piano. With the
/// speaker selected, audible notes play on the synth while the piano gets a
/// silent velocity-1 Note On, so its LEDs still follow the music.
fn route(m: Msg, out: &Sender<Msg>, synth: Option<&SharedSynth>) {
    let Some(synth) = synth else {
        let _ = out.send(m);
        return;
    };
    let mode = SOUND.load(Ordering::Relaxed);
    let audible_on = m[0] & 0xf0 == 0x90 && m[2] > 1;
    match mode {
        SOUND_SPEAKER if audible_on => {
            synth.lock().unwrap().note_on(m[1], m[2]);
            let _ = out.send([m[0], m[1], 1]);
        }
        SOUND_SPEAKER | SOUND_BOTH => {
            let is_hint = m[0] & 0xf0 == 0x90 && m[2] == 1;
            if !is_hint && m[0] & 0xf0 != 0xc0 {
                synth.lock().unwrap().midi(&m);
            }
            let _ = out.send(m);
        }
        _ => {
            let _ = out.send(m);
        }
    }
}

// The mini and headless UIs have no seek bar or A-B loop.
#[cfg_attr(any(ui_mini, ui_headless), allow(dead_code))]
pub enum Cmd {
    Load(PathBuf),
    PlayPause,
    Stop,
    Restart,
    Seek(f32),
    SetOptions(Options),
    Tempo(u16),
    /// Practice section (song ms), or None to clear.
    SetLoop(Option<(u32, u32)>),
    LedTest,
    AllOff,
}

/// What the UI renders. Cloned out under the lock once per frame.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub song: Option<Arc<Song>>,
    pub path: Option<PathBuf>,
    pub load_error: Option<String>,
    pub opts: Options,
    pub state: State,
    pub pos_ms: u32,
    pub stats: Stats,
    /// Per MIDI key: our LED (hint or sounding), hint, pressed.
    pub led: Vec<bool>,
    pub hint: Vec<bool>,
    pub pressed: Vec<bool>,
    pub waiting: Vec<u8>,
    pub step: Option<Step>,
    pub wrong: Option<(u8, Instant)>,
    pub loop_range: Option<(u32, u32)>,
    pub loops: u32,
    /// Bumped on every change so the UI can skip identical frames.
    pub rev: u64,
}

pub type Shared = Arc<Mutex<Snapshot>>;

pub fn start(
    cmds: Receiver<Cmd>,
    keys: Receiver<KeyEvent>,
    out: Sender<Msg>,
    initial: Options,
    synth: Option<SharedSynth>,
    controls: Sender<Control>,
) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(Snapshot {
        opts: initial,
        led: vec![false; 128],
        hint: vec![false; 128],
        pressed: vec![false; 128],
        ..Default::default()
    }));
    let sh = shared.clone();
    // The player never writes flash: its stack may live in PSRAM.
    crate::threads::spawn_ext("player", 12 * 1024, move || run(cmds, keys, out, sh, initial, synth, controls))
        .expect("spawn player");
    shared
}

fn run(
    cmds: Receiver<Cmd>,
    keys: Receiver<KeyEvent>,
    out: Sender<Msg>,
    sh: Shared,
    initial: Options,
    synth: Option<SharedSynth>,
    controls: Sender<Control>,
) {
    let t0 = Instant::now();
    let now_ms = || t0.elapsed().as_millis() as u64;
    let mut engine: Option<Engine> = None;
    let mut opts = initial;
    let mut pressed = [false; 128];
    let mut wrong: Option<(u8, Instant)> = None;
    let mut buf: Vec<Msg> = Vec::with_capacity(256);
    // LED test: all keys on now, all off at this deadline.
    let mut led_test_until: Option<Instant> = None;
    let mut rev = 0u64;
    let mut last_publish = Instant::now();
    let mut dirty = true;
    // Control keys pressed: their LED goes off at the deadline; their
    // release is not passed on.
    let mut flashes: Vec<(u8, Instant)> = Vec::new();
    let mut held = [false; 128];

    loop {
        while let Ok(c) = cmds.try_recv() {
            dirty = true;
            match c {
                Cmd::Load(path) => {
                    if let Some(e) = engine.as_mut() {
                        e.stop(&mut buf);
                    }
                    // Free the old song before parsing the next (small RAM).
                    drop(engine.take());
                    set_loaded(&sh, None, None, None);
                    match crate::songs::read(&path)
                        .and_then(|b| Song::parse(&b).map_err(|e| e.to_string()))
                    {
                        Ok(song) if song.notes.is_empty() => {
                            engine = None;
                            set_loaded(&sh, None, Some(path), Some("This file has no notes".into()));
                        }
                        Ok(song) => {
                            let song = Arc::new(song);
                            engine = Some(Engine::new(song.clone(), opts));
                            info!(
                                "loaded {} — {} notes, {} tracks, {:.0} bpm, split {:?}; heap free {} KB",
                                path.display(),
                                song.notes.len(),
                                song.note_tracks,
                                song.bpm,
                                song.split,
                                unsafe { esp_idf_svc::sys::esp_get_free_heap_size() } / 1024
                            );
                            set_loaded(&sh, Some(song), Some(path), None);
                        }
                        Err(err) => {
                            warn!("load {}: {err}", path.display());
                            engine = None;
                            set_loaded(&sh, None, Some(path), Some(err));
                        }
                    }
                }
                Cmd::PlayPause => {
                    if let Some(e) = engine.as_mut() {
                        match e.state() {
                            State::Playing | State::Waiting => e.pause(&mut buf),
                            _ => {
                                // Host-played notes use the acoustic piano tone.
                                buf.push([0xC0, 0, 0]);
                                e.play(now_ms(), &mut buf);
                            }
                        }
                    }
                }
                Cmd::Stop => {
                    if let Some(e) = engine.as_mut() {
                        e.stop(&mut buf);
                    }
                }
                Cmd::Restart => {
                    if let Some(e) = engine.as_mut() {
                        e.stop(&mut buf);
                        buf.push([0xC0, 0, 0]);
                        e.play(now_ms(), &mut buf);
                    }
                }
                Cmd::Seek(frac) => {
                    if let Some(e) = engine.as_mut() {
                        let ms = (frac.clamp(0.0, 1.0) * e.song().duration_ms as f32) as u32;
                        e.seek(ms, &mut buf);
                    }
                }
                Cmd::SetOptions(o) => {
                    let only_tempo = Options {
                        tempo_pct: opts.tempo_pct,
                        ..o
                    } == opts;
                    opts = o;
                    if let Some(e) = engine.as_mut() {
                        if only_tempo {
                            e.set_tempo(o.tempo_pct);
                        } else {
                            e.set_options(o, &mut buf);
                        }
                    }
                }
                Cmd::Tempo(t) => {
                    opts.tempo_pct = t.clamp(25, 200);
                    if let Some(e) = engine.as_mut() {
                        e.set_tempo(opts.tempo_pct);
                    }
                }
                Cmd::SetLoop(r) => {
                    if let Some(e) = engine.as_mut() {
                        if !e.set_loop(r, &mut buf) {
                            warn!("loop {r:?} rejected (shorter than {} ms)", piano_core::engine::MIN_LOOP_MS);
                        }
                    }
                }
                Cmd::LedTest => {
                    if let Some(e) = engine.as_mut() {
                        e.pause(&mut buf);
                    }
                    for k in KEY_LO..=KEY_HI {
                        buf.push([0x90, k, 1]);
                    }
                    led_test_until = Some(Instant::now() + Duration::from_secs(4));
                }
                Cmd::AllOff => {
                    if let Some(e) = engine.as_mut() {
                        e.stop(&mut buf);
                    }
                    buf.push(ALL_NOTES_OFF);
                }
            }
        }
        let now = Instant::now();
        flashes.retain(|&(key, until)| {
            if now < until {
                return true;
            }
            // Unless the song has lit it meanwhile.
            if !engine.as_ref().is_some_and(|e| e.led(key)) {
                buf.push([0x80, key, 0]);
            }
            false
        });
        if led_test_until.is_some_and(|t| Instant::now() >= t) {
            led_test_until = None;
            buf.push(ALL_NOTES_OFF);
        }

        while let Ok(k) = keys.try_recv() {
            dirty = true;
            // Control keys (piano_core::controls): any time nothing plays,
            // mid-song unless the song plays that key near where it is.
            let playing = engine
                .as_ref()
                .filter(|e| matches!(e.state(), State::Playing | State::Waiting))
                .map(|e| (e.song().as_ref(), e.position_ms(), e.options().hands));
            match k {
                KeyEvent::Down { key, .. } => {
                    if let (Some(c), Some((song, pos, hands))) = (controls::control(key), playing) {
                        // Say so when a control key is taken as a note: on
                        // the piano it just looks like the key did nothing.
                        if let Some(d) = controls::note_near(song, key, pos, hands) {
                            info!(
                                "control key {} is a note here ({:+.1} s): no {}",
                                piano_core::song::key_name(key),
                                d as f32 / 1000.0,
                                controls::label(c)
                            );
                        }
                    }
                    if let Some(c) = controls::press(key, playing) {
                        info!("control: {} ({})", controls::label(c), piano_core::song::key_name(key));
                        let _ = controls.send(c);
                        held[key as usize & 127] = true;
                        buf.push([0x90, key, 1]);
                        flashes.retain(|f| f.0 != key);
                        flashes.push((key, Instant::now() + CONTROL_FLASH));
                        continue;
                    }
                }
                KeyEvent::Up { key } if held[key as usize & 127] => {
                    held[key as usize & 127] = false;
                    continue;
                }
                _ => {}
            }
            match k {
                KeyEvent::Down { key, .. } => {
                    pressed[key as usize & 127] = true;
                    if let Some(e) = engine.as_mut() {
                        let before = e.stats().wrong;
                        e.key_down(key, &mut buf);
                        if e.stats().wrong > before {
                            wrong = Some((key, Instant::now()));
                        }
                    }
                }
                KeyEvent::Up { key } => {
                    pressed[key as usize & 127] = false;
                    if let Some(e) = engine.as_mut() {
                        e.key_up(key);
                    }
                }
            }
        }

        if let Some(e) = engine.as_mut() {
            let before = buf.len();
            e.tick(now_ms(), &mut buf);
            if buf.len() != before || matches!(e.state(), State::Playing) {
                dirty = true;
            }
        }
        for m in buf.drain(..) {
            route(m, &out, synth.as_ref());
        }

        // Publish at most every 15 ms (the UI frames at ~30–60 Hz).
        if dirty && last_publish.elapsed() >= Duration::from_millis(15) {
            dirty = false;
            last_publish = Instant::now();
            rev += 1;
            let mut s = sh.lock().unwrap();
            s.rev = rev;
            s.opts = opts;
            s.wrong = wrong.filter(|w| w.1.elapsed() < Duration::from_millis(600));
            for k in 0..128u8 {
                s.pressed[k as usize] = pressed[k as usize];
                s.led[k as usize] = engine.as_ref().is_some_and(|e| e.led(k)) || led_test_until.is_some();
                s.hint[k as usize] = engine.as_ref().is_some_and(|e| e.is_hint(k));
            }
            match engine.as_ref() {
                Some(e) => {
                    s.state = e.state();
                    s.pos_ms = e.position_ms();
                    s.stats = e.stats();
                    s.waiting = e.waiting_keys();
                    s.step = e.current_step().cloned();
                    s.loop_range = e.loop_range();
                    s.loops = e.loops();
                }
                None => {
                    s.state = State::Stopped;
                    s.pos_ms = 0;
                    s.stats = Stats::default();
                    s.waiting.clear();
                    s.step = None;
                    s.loop_range = None;
                    s.loops = 0;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn set_loaded(sh: &Shared, song: Option<Arc<Song>>, path: Option<PathBuf>, err: Option<String>) {
    let mut s = sh.lock().unwrap();
    s.song = song;
    s.path = path;
    s.load_error = err;
    s.rev += 1;
}

/// The options a board boots with: the saved ones, in Follow mode (every
/// boot starts a song in Follow, see browse::start_song).
pub fn boot_options() -> Options {
    Options {
        mode: piano_core::engine::Mode::Follow,
        ..saved_options()
    }
}

/// The engine options saved in NVS (Play page and Settings).
pub fn saved_options() -> Options {
    use piano_core::engine::Mode;
    Options {
        mode: match prefs::get_i32("mode") {
            Some(0) => Mode::Listen,
            Some(2) => Mode::Follow,
            _ => Mode::Learn,
        },
        hands: hands_from(prefs::get_i32("hands").unwrap_or(0)),
        tempo_pct: prefs::get_i32("tempo").unwrap_or(100).clamp(25, 200) as u16,
        accompany: prefs::get_bool("accompany", true),
        easy: prefs::get_bool("easy", false),
        led_off_on_hit: prefs::get_bool("ledoff", true),
        lead_ms: prefs::get_i32("lead").unwrap_or(700).clamp(200, 1500) as u32,
        hint_vel: 1,
    }
}

/// Engine option helpers for the UI.
pub fn hands_from(i: i32) -> HandSel {
    match i {
        1 => HandSel::Right,
        2 => HandSel::Left,
        _ => HandSel::Both,
    }
}

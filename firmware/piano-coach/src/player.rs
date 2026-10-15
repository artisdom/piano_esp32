//! Player thread: owns the [`Engine`], runs its clock at ~1 kHz, routes key
//! events from the piano into it and its MIDI output to the transport, and
//! publishes a snapshot the UI reads every frame.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::{info, warn};
use piano_core::engine::{Engine, Msg, Options, State, Stats, ALL_NOTES_OFF};
use piano_core::song::{HandSel, Song, Step, KEY_HI, KEY_LO};

use crate::audio::SharedSynth;
use crate::link::KeyEvent;

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
fn route(m: Msg, out: &Sender<Msg>, synth: &SharedSynth) {
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
    synth: SharedSynth,
) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(Snapshot {
        opts: initial,
        led: vec![false; 128],
        hint: vec![false; 128],
        pressed: vec![false; 128],
        ..Default::default()
    }));
    let sh = shared.clone();
    std::thread::Builder::new()
        .name("player".into())
        .stack_size(16 * 1024)
        .spawn(move || run(cmds, keys, out, sh, initial, synth))
        .expect("spawn player");
    shared
}

fn run(
    cmds: Receiver<Cmd>,
    keys: Receiver<KeyEvent>,
    out: Sender<Msg>,
    sh: Shared,
    initial: Options,
    synth: SharedSynth,
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

    loop {
        while let Ok(c) = cmds.try_recv() {
            dirty = true;
            match c {
                Cmd::Load(path) => {
                    if let Some(e) = engine.as_mut() {
                        e.stop(&mut buf);
                    }
                    match std::fs::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|b| Song::parse(&b).map_err(|e| e.to_string()))
                    {
                        Ok(song) if song.notes.is_empty() => {
                            engine = None;
                            set_loaded(&sh, None, Some(path), Some("This file has no notes".into()));
                        }
                        Ok(song) => {
                            info!(
                                "loaded {} — {} notes, {} tracks, {:.0} bpm, split {:?}",
                                path.display(),
                                song.notes.len(),
                                song.note_tracks,
                                song.bpm,
                                song.split
                            );
                            let song = Arc::new(song);
                            engine = Some(Engine::new(song.clone(), opts));
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
        if led_test_until.is_some_and(|t| Instant::now() >= t) {
            led_test_until = None;
            buf.push(ALL_NOTES_OFF);
        }

        while let Ok(k) = keys.try_recv() {
            dirty = true;
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
            route(m, &out, &synth);
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

/// Engine option helpers for the UI.
pub fn hands_from(i: i32) -> HandSel {
    match i {
        1 => HandSel::Right,
        2 => HandSel::Left,
        _ => HandSel::Both,
    }
}

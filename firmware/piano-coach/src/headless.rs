//! Headless UI for boards without a display (`ui_headless`: the
//! ESP32-Audio-Kit, the YD-ESP32-S3): the board's keys, short tones on its
//! speaker (if it has one), its status LED and the piano's own LEDs for
//! feedback, plus a Wi-Fi web page (`web.rs`) to browse and search the
//! library and pick a song. The library is the microSD card, or on boards
//! without one the flash song pack, shown as the same folder tree under
//! [`PACK_ROOT`].
//!
//! [`Coach`] holds what the user chose — the playlist (the current song's
//! folder), the song, mode, hands, tempo and speaker settings — and turns
//! actions into player commands. The key loop here and the web handlers
//! share it behind a mutex.
//!
//! Keys (KEY1…KEY6 on the board; ESP-ADF names in brackets):
//! * KEY1 [REC]  — restart the song; hold: notes from piano / speaker / both
//! * KEY2 [MODE] — Learn → Follow → Listen; hold: hands Both → Right → Left
//! * KEY3 [SET]  — next song in the folder (it starts at once); hold: previous
//! * KEY4 [PLAY] — play / pause; hold: stop
//! * KEY5 [VOL+] — speaker louder; hold: tempo +10 %
//! * KEY6 [VOL−] — speaker softer; hold: tempo −10 %
//!
//! A single BOOT key (YD-ESP32-S3): tap = play / pause, double tap = next
//! song, hold = next mode (the RGB LED flashes white / yellow / violet for
//! Learn / Follow / Listen).
//!
//! Status LED: blinking while the piano is not connected, on while it is —
//! an RGB LED shows green for USB, blue for Bluetooth.
//!
//! The piano's lowest white keys, while nothing plays: A0 previous song,
//! B0 next song, C1 play / pause, D1 stop.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use log::{info, warn};
use piano_core::engine::{Mode, Options, State};
use piano_core::library;
use piano_core::search::{Index, SongRef};
use piano_core::songpack::Pack;
use piano_core::song::HandSel;

use crate::board::{self, Button};
use crate::link::Status;
use crate::player::{self, Cmd, Shared, SharedSynth};
use crate::prefs;

pub struct Ctx {
    pub cmds: Sender<Cmd>,
    pub shared: Shared,
    pub status: Arc<Status>,
    pub nav: Receiver<u8>,
    pub synth: Option<SharedSynth>,
    pub sd_ok: bool,
}

/// Virtual root of the song pack's folder tree.
pub const PACK_ROOT: &str = "/songs";

/// Where the library lives.
#[cfg_attr(has_sd, allow(dead_code))] // card boards never build the pack
enum Source {
    Card,
    Pack(&'static Pack<'static>),
}

fn source() -> &'static Source {
    static SOURCE: OnceLock<Source> = OnceLock::new();
    SOURCE.get_or_init(|| {
        #[cfg(not(has_sd))]
        if let Ok(p) = crate::songs::open_pack() {
            return Source::Pack(p);
        }
        Source::Card
    })
}

/// The pack node at a path under [`PACK_ROOT`].
fn pack_node(pack: &Pack, path: &Path) -> Option<u32> {
    let rel = path.strip_prefix(PACK_ROOT).ok()?;
    let mut node = pack.root();
    for part in rel.components() {
        let name = part.as_os_str().to_str()?;
        if !node.is_dir() {
            return None;
        }
        node = node.children().map(|i| pack.node(i)).find(|n| n.name == name)?;
    }
    Some(node.index)
}

/// A song file (card) or song entry (pack) at `path`.
pub fn is_song(path: &Path) -> bool {
    match source() {
        Source::Card => path.is_file(),
        Source::Pack(p) => pack_node(p, path).is_some_and(|i| !p.node(i).is_dir()),
    }
}

/// What the player loads for the song at `path` (pack: `/pack/<index>`).
fn playable(path: &Path) -> PathBuf {
    match source() {
        Source::Card => path.to_path_buf(),
        #[cfg(not(has_sd))]
        Source::Pack(p) => pack_node(p, path).map(crate::songs::pack_path).unwrap_or_default(),
        #[cfg(has_sd)]
        Source::Pack(_) => PathBuf::new(),
    }
}

/// The search index over the whole library.
fn build_index(root: &Path) -> Index {
    match source() {
        Source::Card => Index::build(root),
        Source::Pack(p) => {
            let mut songs: Vec<SongRef> = p
                .songs()
                .into_iter()
                .map(|i| {
                    let path = Path::new(PACK_ROOT).join(p.path(i));
                    let folder = p.node(p.node(i).parent).name;
                    SongRef::new(path, library::entry(folder, true).title)
                })
                .filter(|s| s.path.starts_with(root))
                .collect();
            songs.sort_by(|a, b| library::natural_cmp(&a.title, &b.title));
            Index { songs }
        }
    }
}

/// The library root: a top-level `midi/` folder if there is one (as on the
/// full boards).
pub fn library_root() -> PathBuf {
    let mut root = match source() {
        Source::Pack(_) => PathBuf::from(PACK_ROOT),
        Source::Card => ["/sdcard/midi", "/sdcard/MIDI"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/sdcard")),
    };
    // A library that is one folder (e.g. "JustPiano2-20160326/") starts
    // inside it, so the web page opens on its categories.
    for _ in 0..4 {
        let (dirs, songs) = list_dir(&root);
        match (dirs.as_slice(), songs.is_empty()) {
            ([only], true) => root = root.join(only),
            _ => break,
        }
    }
    root
}

/// Sub-folders and songs (sorted) of a folder.
pub fn list_dir(dir: &Path) -> (Vec<String>, Vec<String>) {
    if let Source::Pack(p) = source() {
        // Pack folders are stored in library order already.
        let Some(node) = pack_node(p, dir).map(|i| p.node(i)).filter(|n| n.is_dir()) else {
            return Default::default();
        };
        let (mut dirs, mut songs) = (Vec::new(), Vec::new());
        for n in node.children().map(|i| p.node(i)) {
            if n.is_dir() { &mut dirs } else { &mut songs }.push(n.name.to_string());
        }
        return (dirs, songs);
    }
    let mut entries: Vec<library::Entry> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| library::entry(&e.file_name().to_string_lossy(), e.path().is_dir()))
                .collect()
        })
        .unwrap_or_default();
    entries = library::sort_entries(entries);
    let dirs = entries.iter().filter(|e| e.is_dir).map(|e| e.name.clone()).collect();
    let songs = entries.iter().filter(|e| !e.is_dir).map(|e| e.name.clone()).collect();
    (dirs, songs)
}

pub struct Coach {
    cmds: Sender<Cmd>,
    pub shared: Shared,
    pub status: Arc<Status>,
    synth: Option<SharedSynth>,
    pub root: PathBuf,
    pub sd_ok: bool,
    /// Current folder and its songs (file names), the song's position in it.
    pub dir: PathBuf,
    pub playlist: Vec<String>,
    pub index: usize,
    pub opts: Options,
    /// The search index, built in the background after boot.
    pub search: Arc<Mutex<Option<Index>>>,
}

impl Coach {
    fn new(ctx: &Ctx) -> Coach {
        let root = library_root();
        let mut c = Coach {
            cmds: ctx.cmds.clone(),
            shared: ctx.shared.clone(),
            status: ctx.status.clone(),
            synth: ctx.synth.clone(),
            root: root.clone(),
            sd_ok: ctx.sd_ok,
            dir: root.clone(),
            playlist: Vec::new(),
            index: 0,
            opts: player::saved_options(),
            search: Arc::new(Mutex::new(None)),
        };
        // The last song played, else the first folder that has songs.
        let last = prefs::get_str("lastsong").map(|rel| root.join(rel)).filter(|p| is_song(p));
        match last {
            Some(p) => c.select(&p, false),
            None => {
                let mut d = root.clone();
                for _ in 0..8 {
                    let (dirs, songs) = list_dir(&d);
                    if !songs.is_empty() || dirs.is_empty() {
                        break;
                    }
                    d = d.join(&dirs[0]);
                }
                c.set_dir(&d);
                if let Some(first) = c.playlist.first().cloned() {
                    c.select(&d.join(first), false);
                }
            }
        }
        c
    }

    fn set_dir(&mut self, dir: &Path) {
        self.dir = dir.to_path_buf();
        self.playlist = list_dir(dir).1;
        self.index = 0;
    }

    /// Path of the current song, if any.
    pub fn current(&self) -> Option<PathBuf> {
        self.playlist.get(self.index).map(|n| self.dir.join(n))
    }

    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().into_owned()
    }

    /// Make `path` the current song (its folder the playlist) and load it;
    /// `start` plays it at once.
    pub fn select(&mut self, path: &Path, start: bool) {
        if let Some(parent) = path.parent() {
            if parent != self.dir || self.playlist.is_empty() {
                self.set_dir(parent);
            }
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.index = self.playlist.iter().position(|n| *n == name).unwrap_or(0);
        let _ = self.cmds.send(Cmd::SetOptions(self.opts));
        let _ = self.cmds.send(Cmd::Load(playable(path)));
        prefs::set_str("lastsong", &self.rel(path));
        if start {
            let _ = self.cmds.send(Cmd::Restart);
        }
        info!("headless: {} ({}/{})", self.rel(path), self.index + 1, self.playlist.len());
    }

    pub fn step(&mut self, d: isize) {
        let n = self.playlist.len() as isize;
        if n == 0 {
            self.tone(&[48, 43], 120);
            return;
        }
        self.index = (self.index as isize + d).rem_euclid(n) as usize;
        if let Some(p) = self.current() {
            self.select(&p, true);
        }
    }

    pub fn state(&self) -> State {
        self.shared.lock().unwrap().state
    }

    pub fn play_pause(&mut self) {
        match self.state() {
            State::Stopped | State::Finished => {
                let _ = self.cmds.send(Cmd::Restart);
            }
            _ => {
                let _ = self.cmds.send(Cmd::PlayPause);
            }
        }
    }

    pub fn stop(&mut self) {
        let _ = self.cmds.send(Cmd::Stop);
    }

    pub fn restart(&mut self) {
        let _ = self.cmds.send(Cmd::Restart);
    }

    fn apply(&mut self) {
        let _ = self.cmds.send(Cmd::SetOptions(self.opts));
    }

    pub fn set_mode(&mut self, m: Mode) {
        self.opts.mode = m;
        prefs::set_i32("mode", m as i32);
        self.apply();
        // One, two or three rising notes: Learn, Follow, Listen.
        let n = match m {
            Mode::Learn => 1,
            Mode::Follow => 2,
            Mode::Listen => 3,
        };
        self.tone(&[72, 76, 79][..n], 110);
    }

    pub fn set_hands(&mut self, h: HandSel) {
        self.opts.hands = h;
        prefs::set_i32("hands", h as i32);
        self.apply();
        // High = right hand, low = left, both = an octave.
        self.tone(
            match h {
                HandSel::Both => &[60, 72],
                HandSel::Right => &[72],
                HandSel::Left => &[48],
            },
            140,
        );
    }

    pub fn set_tempo(&mut self, pct: i32) {
        self.opts.tempo_pct = pct.clamp(30, 200) as u16;
        prefs::set_i32("tempo", self.opts.tempo_pct as i32);
        let _ = self.cmds.send(Cmd::Tempo(self.opts.tempo_pct));
        self.tone(&[if pct >= 100 { 79 } else { 67 }], 90);
    }

    #[cfg(has_speaker)]
    pub fn set_volume(&mut self, v: i32) {
        let v = v.clamp(0, 100);
        crate::audio::VOLUME.store(v, Ordering::Relaxed);
        prefs::set_i32("spkvol", v);
        self.tone(&[76], 120);
    }

    pub fn set_sound(&mut self, s: u8) {
        let s = s.min(2);
        player::SOUND.store(s, Ordering::Relaxed);
        prefs::set_i32("sound", s as i32);
        self.tone(&[60, 64, 67][..s as usize + 1], 90);
    }

    pub fn led_test(&mut self) {
        let _ = self.cmds.send(Cmd::LedTest);
    }

    pub fn all_off(&mut self) {
        let _ = self.cmds.send(Cmd::AllOff);
    }

    /// A few short notes on the board's speaker (always the speaker: this
    /// is feedback, not music).
    pub fn tone(&self, keys: &[u8], ms: u64) {
        let Some(synth) = self.synth.clone() else { return };
        let keys = keys.to_vec();
        let _ = crate::threads::spawn_ext("tone", 3 * 1024, move || {
            for k in keys {
                synth.lock().unwrap().note_on(k, 90);
                std::thread::sleep(Duration::from_millis(ms));
                synth.lock().unwrap().note_off(k);
            }
        });
    }

    /// Next / previous mode, hands, sound, in the key order.
    fn next_mode(&mut self) {
        self.set_mode(match self.opts.mode {
            Mode::Learn => Mode::Follow,
            Mode::Follow => Mode::Listen,
            Mode::Listen => Mode::Learn,
        });
    }

    fn next_hands(&mut self) {
        self.set_hands(match self.opts.hands {
            HandSel::Both => HandSel::Right,
            HandSel::Right => HandSel::Left,
            HandSel::Left => HandSel::Both,
        });
    }
}

pub type SharedCoach = Arc<Mutex<Coach>>;

/// Internal (DMA-capable) RAM is what runs out first on the ESP32.
fn log_internal(when: &str) {
    use esp_idf_svc::sys as sys;
    unsafe {
        info!(
            "headless: {when}: internal RAM free {} KB (largest {} KB), PSRAM free {} KB",
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) / 1024,
            sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_INTERNAL) / 1024,
            sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM) / 1024
        );
    }
}

pub fn run(ctx: Ctx) -> anyhow::Result<()> {
    let coach: SharedCoach = Arc::new(Mutex::new(Coach::new(&ctx)));
    if !ctx.sd_ok {
        if cfg!(has_sd) {
            warn!("headless: no microSD card — insert one with a midi/ folder and press RST");
        } else {
            warn!("headless: no song pack in flash — tools/songs.sh <board> build, then flash");
        }
    }

    // Search index in the background (reads every folder of the library).
    {
        let (root, slot) = {
            let c = coach.lock().unwrap();
            (c.root.clone(), c.search.clone())
        };
        let _ = crate::threads::spawn_ext("index", 16 * 1024, move || {
            let t = Instant::now();
            let idx = build_index(&root);
            info!("headless: indexed {} songs in {:?}", idx.songs.len(), t.elapsed());
            *slot.lock().unwrap() = Some(idx);
        });
    }

    log_internal("before Wi-Fi");
    // The web page (Wi-Fi access point + HTTP server).
    let _web = match crate::web::start(coach.clone()) {
        Ok(w) => Some(w),
        Err(e) => {
            warn!("web: not started — {e:#}");
            None
        }
    };

    log_internal("after Wi-Fi");
    let mut blink = Instant::now();
    let mut led = false;
    // A colour shown for a moment (mode change on single-key boards).
    let mut flash: Option<(Instant, [u8; 3])> = None;
    let mut shown_last = None;
    loop {
        for b in board::poll_buttons() {
            let mut c = coach.lock().unwrap();
            match b {
                Button::Keypad(1) => c.restart(),
                Button::KeypadHold(1) => {
                    let s = (player::SOUND.load(Ordering::Relaxed) + 1) % 3;
                    c.set_sound(s)
                }
                Button::Keypad(2) => c.next_mode(),
                Button::KeypadHold(2) => c.next_hands(),
                Button::Keypad(3) => c.step(1),
                Button::KeypadHold(3) => c.step(-1),
                Button::Keypad(4) => c.play_pause(),
                Button::KeypadHold(4) => c.stop(),
                #[cfg(has_speaker)]
                Button::Keypad(5) => {
                    let v = crate::audio::VOLUME.load(Ordering::Relaxed) + 10;
                    c.set_volume(v)
                }
                #[cfg(has_speaker)]
                Button::Keypad(6) => {
                    let v = crate::audio::VOLUME.load(Ordering::Relaxed) - 10;
                    c.set_volume(v)
                }
                Button::KeypadHold(5) => {
                    let t = c.opts.tempo_pct as i32 + 10;
                    c.set_tempo(t)
                }
                Button::KeypadHold(6) => {
                    let t = c.opts.tempo_pct as i32 - 10;
                    c.set_tempo(t)
                }
                // The single BOOT key.
                Button::KeyTap => c.play_pause(),
                Button::KeyDoubleTap => c.step(1),
                Button::KeyHold => {
                    c.next_mode();
                    flash = Some((
                        Instant::now(),
                        match c.opts.mode {
                            Mode::Learn => [40, 40, 40],
                            Mode::Follow => [40, 30, 0],
                            Mode::Listen => [30, 0, 40],
                        },
                    ));
                }
                _ => {}
            }
        }
        while let Ok(k) = ctx.nav.try_recv() {
            let mut c = coach.lock().unwrap();
            match k {
                player::NAV_PREV => c.step(-1),
                player::NAV_NEXT => c.step(1),
                player::NAV_OK => c.play_pause(),
                _ => c.stop(),
            }
        }
        // Status LED: on while the piano is connected (RGB: green over USB,
        // blue over Bluetooth), else blinking amber.
        let connected = ctx.status.connected.load(Ordering::Relaxed);
        let flashing = flash.filter(|f| f.0.elapsed() < Duration::from_millis(1200));
        if flashing.is_none() {
            flash = None;
        }
        let shown = if let Some((_, rgb)) = flashing {
            (true, rgb)
        } else if connected {
            (true, if ctx.status.usb.load(Ordering::Relaxed) { [0, 24, 0] } else { [0, 0, 32] })
        } else {
            if blink.elapsed() >= Duration::from_millis(400) {
                blink = Instant::now();
                led = !led;
            }
            (led, [24, 10, 0])
        };
        if shown_last != Some(shown) {
            board::status_led(shown.0, shown.1);
            shown_last = Some(shown);
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}

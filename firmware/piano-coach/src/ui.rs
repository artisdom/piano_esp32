//! Slint UI on the board's framebuffer: platform glue, the render/touch
//! loop, and the view-model wiring for the Library, Play and Settings pages.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use log::info;
use piano_core::controls::{self, Control};
use piano_core::engine::{Mode, Options, State};
use piano_core::engine::MIN_LOOP_MS;
use piano_core::library::{self, Entry};
use piano_core::progress::{self, ScoreKind};
use piano_core::search::{Index, SongRef};
use piano_core::song::{is_black, key_name, Hand, Song, KEY_HI, KEY_LO};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, PhysicalSize, SharedString, VecModel};

use crate::board::{self, FRAME_HEIGHT, FRAME_WIDTH};
use crate::browse;
use crate::link::{Status, Transport};
use crate::player::{self, Cmd, Shared};
use crate::prefs;
use crate::globe::Globe;
use crate::store::Store;

mod sheet;

thread_local! {
    /// Last panel origin reported by Slint (the globe may be built later).
    static GLOBE_ORIGIN: Cell<Option<(f32, f32)>> = const { Cell::new(None) };
}

/// Set when bookmarks change so the globe recolours its gold dots.
static GLOBE_FAVS_DIRTY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
/// Drag amplification from the press point (lists scroll faster than the
/// finger). Must match `Touch.drag-gain` in ui/widgets.slint.
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
    /// 0 folders, 1 bookmarks, 2 recently played, 3 song globe.
    view: i32,
    /// Bookmarks / recent: the song behind each entry.
    flat: Vec<PathBuf>,
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

    /// The song file behind list row `i`, if it is one.
    fn song_path(&self, i: usize) -> Option<PathBuf> {
        match self.view {
            0 => self.entries.get(i).filter(|e| !e.is_dir).map(|e| self.dir().join(&e.name)),
            _ => self.flat.get(i).cloned(),
        }
    }

    /// Fill a flat view (bookmarks / recent) from the progress store.
    fn fill_flat(&mut self, store: &Store) {
        let rels = match self.view {
            1 => store.progress.bookmarks(),
            2 => store.progress.recent(60),
            _ => Vec::new(),
        };
        self.flat = rels.iter().map(|r| store.abs(r)).collect();
        self.entries = self
            .flat
            .iter()
            .map(|p| {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                library::entry(&name, false)
            })
            .collect();
    }
}

/// Parent folder title of a song path, for subtitles.
fn folder_title(p: &std::path::Path) -> String {
    p.parent()
        .and_then(|d| d.file_name())
        .map(|n| library::entry(&n.to_string_lossy(), true).title)
        .unwrap_or_default()
}

/// Stars for a song's best score (learn or follow), "" if never scored.
fn badge(store: &Store, path: &std::path::Path) -> (bool, String) {
    let r = store.progress.get(&store.rel(path));
    let best = r.best_learn.max(r.best_follow);
    let b = best
        .map(|a| {
            let n = progress::stars(a) as usize;
            format!("{}{}", "★".repeat(n), "☆".repeat(3 - n))
        })
        .unwrap_or_default();
    (r.bookmarked, b)
}

/// Bookmark / scores / plays line for the song card.
fn score_line(store: &Store, path: &std::path::Path) -> String {
    let r = store.progress.get(&store.rel(path));
    if r.best_learn.is_none() && r.best_follow.is_none() && r.plays == 0 {
        return String::new();
    }
    format!(
        "Learn {} · Follow {} · played {}×",
        progress::star_text(r.best_learn),
        progress::star_text(r.best_follow),
        r.plays
    )
}

fn library_root() -> PathBuf {
    for p in ["/sdcard/midi", "/sdcard/MIDI", "/sdcard"] {
        if std::fs::metadata(p).map(|m| m.is_dir()).unwrap_or(false) {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("/sdcard")
}

fn push_library(ui: &AppWindow, lib: &Lib, store: &Store, sd_ok: bool, err: Option<String>) {
    let vm = ui.global::<LibraryVM>();
    let mut crumbs: Vec<SharedString> = vec!["Library".into()];
    crumbs.extend(lib.stack.iter().map(|(_, t)| SharedString::from(t.as_str())));
    let n = lib.entries.len();
    vm.set_path_text(
        match lib.view {
            1 => format!("{n} bookmarked songs"),
            2 => format!("{n} songs, most recent first"),
            3 => "drag to spin · tap a dot · tap a colour to turn to it".to_string(),
            _ if lib.stack.is_empty() => format!("{} · {n} items", lib.root.display()),
            _ => format!("{n} items"),
        }
        .into(),
    );
    vm.set_crumbs(ModelRc::new(VecModel::from(crumbs)));
    let rows: Vec<LibEntry> = lib
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let (fav, badge) = lib.song_path(i).map(|p| badge(store, &p)).unwrap_or_default();
            let subtitle = match (lib.view, lib.flat.get(i)) {
                (1 | 2, Some(p)) if e.subtitle.is_empty() => folder_title(p),
                (1 | 2, Some(p)) => format!("{} · {}", e.subtitle, folder_title(p)),
                _ => e.subtitle.clone(),
            };
            LibEntry {
                title: e.title.as_str().into(),
                subtitle: subtitle.into(),
                is_dir: e.is_dir,
                fav,
                badge: badge.into(),
            }
        })
        .collect();
    vm.set_entries(ModelRc::new(VecModel::from(rows)));
    vm.set_selected(-1);
    vm.set_empty_text(
        match (lib.view, sd_ok, err) {
            (_, false, _) => "No microSD card. Copy your MIDI files to a FAT32/exFAT card (a `midi` folder is used if present) and insert it.".to_string(),
            (1, ..) => "No bookmarks yet — tap the ☆ on a song's card to add one.".to_string(),
            (2, ..) => "Nothing played yet.".to_string(),
            (_, true, Some(e)) => format!("Cannot read folder: {e}"),
            (_, true, None) => "No MIDI files here.".to_string(),
        }
        .into(),
    );
}

/// Show a song in the card and load it into the player.
fn select_song(ui: &AppWindow, store: &Store, path: PathBuf, cmds: &Sender<Cmd>) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let e = library::entry(&name, false);
    let vm = ui.global::<LibraryVM>();
    vm.set_sel_title(e.title.as_str().into());
    vm.set_sel_subtitle(e.subtitle.as_str().into());
    vm.set_sel_info("Loading…".into());
    vm.set_sel_error("".into());
    vm.set_sel_ready(false);
    vm.set_sel_fav(store.progress.get(&store.rel(&path)).bookmarked);
    vm.set_sel_scores(score_line(store, &path).into());
    let _ = cmds.send(Cmd::Load(path));
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

/// The library-wide search index, built off the UI thread.
#[derive(Default)]
struct SearchState {
    index: Option<Arc<Index>>,
    building: bool,
    /// Bumped when a build finishes.
    rev: u32,
}

type SharedSearch = Arc<Mutex<SearchState>>;

fn build_index(search: &SharedSearch, root: PathBuf) {
    {
        let mut st = search.lock().unwrap();
        if st.building {
            return;
        }
        st.building = true;
    }
    let s = search.clone();
    let _ = std::thread::Builder::new()
        .name("index".into())
        .stack_size(16 * 1024)
        .spawn(move || {
            let t = Instant::now();
            let idx = Index::build(&root);
            info!("search: indexed {} songs under {} in {:?}", idx.songs.len(), root.display(), t.elapsed());
            let mut st = s.lock().unwrap();
            st.index = Some(Arc::new(idx));
            st.building = false;
            st.rev += 1;
        });
}

/// Re-run the query and refresh the result list + status line.
fn run_search(ui: &AppWindow, search: &SharedSearch, query: &str, results: &RefCell<Vec<SongRef>>, store: &Store) {
    let vm = ui.global::<LibraryVM>();
    vm.set_query(query.into());
    vm.set_result_selected(-1);
    let (index, building) = {
        let st = search.lock().unwrap();
        (st.index.clone(), st.building)
    };
    let hits: Vec<SongRef> = match &index {
        Some(idx) => idx.search(query, 200).into_iter().cloned().collect(),
        None => Vec::new(),
    };
    vm.set_search_status(
        match (&index, building) {
            (None, true) => "indexing the SD card…".to_string(),
            (None, false) => "no songs indexed (insert a card and Rescan)".to_string(),
            (Some(idx), _) if query.trim().is_empty() => format!("{} songs — start typing", idx.songs.len()),
            (Some(_), _) if hits.len() >= 200 => "200+ matches — keep typing".to_string(),
            (Some(_), _) => format!("{} matches", hits.len()),
        }
        .into(),
    );
    let rows: Vec<LibEntry> = hits
        .iter()
        .map(|h| {
            let (fav, badge) = badge(store, &h.path);
            LibEntry {
                title: h.title.as_str().into(),
                subtitle: if h.artist.is_empty() {
                    h.folder.as_str().into()
                } else {
                    format!("{} · {}", h.artist, h.folder).into()
                },
                is_dir: false,
                fav,
                badge: badge.into(),
            }
        })
        .collect();
    vm.set_results(ModelRc::new(VecModel::from(rows)));
    *results.borrow_mut() = hits;
}

pub struct Ctx {
    pub cmds: Sender<Cmd>,
    pub shared: Shared,
    pub status: Arc<Status>,
    pub transport: Transport,
    pub sd_ok: bool,
    /// Piano-key controls (piano_core::controls).
    pub controls: Receiver<Control>,
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

/// Every boot starts in Follow (see browse::start_song).
pub fn initial_options() -> Options {
    player::boot_options()
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
        s.set_sound(player::SOUND.load(std::sync::atomic::Ordering::Relaxed) as i32);
        s.set_speaker_volume(crate::audio::VOLUME.load(std::sync::atomic::Ordering::Relaxed) as f32);
        s.set_transport(match ctx.transport {
            Transport::Usb => 0,
            Transport::Ble => 1,
        });
        let a = ui.global::<AppVM>();
        a.set_version(format!("piano-coach {}", env!("CARGO_PKG_VERSION")).into());
        a.set_board_name(board::CAPS.name.into());
        a.set_has_backlight(board::CAPS.backlight);
        a.set_has_power_latch(board::CAPS.power_latch);
        a.set_show_touches(prefs::get_bool("showtouch", false));
        a.set_transport(ctx.transport.label().into());
        a.set_sd_status(if ctx.sd_ok { sd_label() } else { "no card".into() }.into());
    }

    let sd_ok = Rc::new(Cell::new(ctx.sd_ok));
    let geometry = key_geometry();
    let lib = Rc::new(RefCell::new(Lib {
        root: library_root(),
        stack: Vec::new(),
        entries: Vec::new(),
        view: 0,
        flat: Vec::new(),
    }));
    ui.global::<SettingsVM>()
        .set_library_root(lib.borrow().root.display().to_string().into());
    let store = Rc::new(RefCell::new(Store::load(lib.borrow().root.clone())));
    {
        let err = lib.borrow_mut().reload().err();
        push_library(&ui, &lib.borrow(), &store.borrow(), sd_ok.get(), err);
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
        let (l, w, c, sd, st) = (lib.clone(), ui.as_weak(), ctx.cmds.clone(), sd_ok.clone(), store.clone());
        vm.on_open(move |i| {
            let ui = w.unwrap();
            let entry = l.borrow().entries.get(i as usize).cloned();
            let Some(e) = entry else { return };
            if e.is_dir && l.borrow().view == 0 {
                l.borrow_mut().stack.push((e.name.clone(), e.title.clone()));
                let err = l.borrow_mut().reload().err();
                push_library(&ui, &l.borrow(), &st.borrow(), sd.get(), err);
            } else if let Some(path) = l.borrow().song_path(i as usize) {
                ui.global::<LibraryVM>().set_selected(i);
                select_song(&ui, &st.borrow(), path, &c);
            }
        });
        let (l, w, sd, st) = (lib.clone(), ui.as_weak(), sd_ok.clone(), store.clone());
        vm.on_up(move || {
            let ui = w.unwrap();
            l.borrow_mut().stack.pop();
            let err = l.borrow_mut().reload().err();
            push_library(&ui, &l.borrow(), &st.borrow(), sd.get(), err);
        });
        let (l, w, sd, st) = (lib.clone(), ui.as_weak(), sd_ok.clone(), store.clone());
        vm.on_crumb(move |i| {
            let ui = w.unwrap();
            l.borrow_mut().stack.truncate(i.max(0) as usize);
            let err = l.borrow_mut().reload().err();
            push_library(&ui, &l.borrow(), &st.borrow(), sd.get(), err);
        });
        let (l, w, sd, st) = (lib.clone(), ui.as_weak(), sd_ok.clone(), store.clone());
        vm.on_set_view(move |v| {
            let ui = w.unwrap();
            ui.global::<LibraryVM>().set_view(v);
            let mut lb = l.borrow_mut();
            lb.view = v;
            let err = match v {
                0 => lb.reload().err(),
                1 | 2 => {
                    lb.fill_flat(&st.borrow());
                    None
                }
                _ => {
                    lb.entries.clear();
                    None
                }
            };
            push_library(&ui, &lb, &st.borrow(), sd.get(), err);
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

    // ---- callbacks: search -----------------------------------------------
    let search: SharedSearch = Arc::new(Mutex::new(SearchState::default()));
    if sd_ok.get() {
        build_index(&search, lib.borrow().root.clone());
    }
    let query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let results: Rc<RefCell<Vec<SongRef>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let vm = ui.global::<LibraryVM>();
        let (w, se, q, r, l, sto) = (ui.as_weak(), search.clone(), query.clone(), results.clone(), lib.clone(), store.clone());
        vm.on_search_toggle(move || {
            let ui = w.unwrap();
            let vm = ui.global::<LibraryVM>();
            let on = !vm.get_searching();
            vm.set_searching(on);
            if on {
                let need = {
                    let st = se.lock().unwrap();
                    st.index.is_none() && !st.building
                };
                if need {
                    build_index(&se, l.borrow().root.clone());
                }
                run_search(&ui, &se, &q.borrow(), &r, &sto.borrow());
            }
        });
        let (w, se, q, r, sto) = (ui.as_weak(), search.clone(), query.clone(), results.clone(), store.clone());
        vm.on_type_key(move |k| {
            if q.borrow().chars().count() < 40 {
                q.borrow_mut().push_str(&k);
            }
            run_search(&w.unwrap(), &se, &q.borrow(), &r, &sto.borrow());
        });
        let (w, se, q, r, sto) = (ui.as_weak(), search.clone(), query.clone(), results.clone(), store.clone());
        vm.on_backspace(move || {
            q.borrow_mut().pop();
            run_search(&w.unwrap(), &se, &q.borrow(), &r, &sto.borrow());
        });
        let (w, se, q, r, sto) = (ui.as_weak(), search.clone(), query.clone(), results.clone(), store.clone());
        vm.on_clear_query(move || {
            q.borrow_mut().clear();
            run_search(&w.unwrap(), &se, &q.borrow(), &r, &sto.borrow());
        });
        let (w, r, c, st) = (ui.as_weak(), results.clone(), ctx.cmds.clone(), store.clone());
        vm.on_open_result(move |i| {
            let ui = w.unwrap();
            let Some(hit) = r.borrow().get(i as usize).cloned() else { return };
            let vm = ui.global::<LibraryVM>();
            vm.set_result_selected(i);
            vm.set_selected(-1);
            select_song(&ui, &st.borrow(), hit.path.clone(), &c);
        });
    }

    // ---- callbacks: bookmarks ---------------------------------------------
    // The song currently loaded in the player (and shown on the card).
    let sel_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    {
        let (w, sp, st, l, sd, toast_fn) =
            (ui.as_weak(), sel_path.clone(), store.clone(), lib.clone(), sd_ok.clone(), show_toast.clone());
        ui.global::<LibraryVM>().on_toggle_fav(move || {
            let ui = w.unwrap();
            let Some(path) = sp.borrow().clone() else { return };
            let on = {
                let mut sto = st.borrow_mut();
                let rel = sto.rel(&path);
                let on = sto.progress.toggle_bookmark(&rel);
                sto.save();
                on
            };
            ui.global::<LibraryVM>().set_sel_fav(on);
            toast_fn(if on { "Bookmarked ★" } else { "Bookmark removed" });
            let mut lb = l.borrow_mut();
            if lb.view == 1 {
                lb.fill_flat(&st.borrow());
            }
            if lb.view != 3 {
                let sel = ui.global::<LibraryVM>().get_selected();
                push_library(&ui, &lb, &st.borrow(), sd.get(), None);
                if lb.view != 1 {
                    ui.global::<LibraryVM>().set_selected(sel);
                }
            }
            GLOBE_FAVS_DIRTY.store(true, std::sync::atomic::Ordering::Relaxed);
        });
    }

    // ---- piano-key controls and the boot song --------------------------------
    // Show `path` in the library (its folder, its row) and load it; `start`
    // plays it at once and opens the Play page.
    let open_song = {
        let (w, l, st, sd, c) = (ui.as_weak(), lib.clone(), store.clone(), sd_ok.clone(), ctx.cmds.clone());
        move |path: PathBuf, start: bool| {
            let ui = w.unwrap();
            {
                let mut lb = l.borrow_mut();
                let rel = path.parent().and_then(|d| d.strip_prefix(&lb.root).ok()).map(|r| r.to_path_buf());
                lb.view = 0;
                ui.global::<LibraryVM>().set_view(0);
                lb.stack = rel
                    .iter()
                    .flat_map(|r| r.components())
                    .map(|c| {
                        let n = c.as_os_str().to_string_lossy().into_owned();
                        let t = library::entry(&n, true).title;
                        (n, t)
                    })
                    .collect();
                let err = lb.reload().err();
                push_library(&ui, &lb, &st.borrow(), sd.get(), err);
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(i) = lb.entries.iter().position(|e| !e.is_dir && e.name == name) {
                    ui.global::<LibraryVM>().set_selected(i as i32);
                }
            }
            let _ = c.send(Cmd::SetOptions(current_options(&ui)));
            select_song(&ui, &st.borrow(), path, &c);
            if start {
                let _ = c.send(Cmd::Restart);
                ui.global::<AppVM>().set_page(1);
            }
        }
    };
    // Every boot: a random song from 儿时回忆 in Follow, started once the
    // piano is connected.
    let mut autostart = false;
    if ctx.sd_ok {
        let root = lib.borrow().root.clone();
        if let Some(p) = browse::start_song(&root) {
            open_song(p, false);
            ui.global::<AppVM>().set_page(1);
            autostart = true;
        }
    }

    // ---- callbacks: song globe ----------------------------------------------
    let globe: Rc<RefCell<Option<Globe>>> = Rc::new(RefCell::new(None));
    let globe_rev: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    {
        let vm = ui.global::<LibraryVM>();
        let (w, g, st, c) = (ui.as_weak(), globe.clone(), store.clone(), ctx.cmds.clone());
        vm.on_sphere_pointer(move |kind, x, y| {
            let ui = w.unwrap();
            let picked = g.borrow_mut().as_mut().and_then(|g| g.pointer(kind, x, y, SCROLL_GAIN));
            if let Some(i) = picked {
                let path = g.borrow().as_ref().map(|g| g.songs[i].clone());
                if let Some(path) = path {
                    select_song(&ui, &st.borrow(), path, &c);
                }
            }
        });
        let g = globe.clone();
        vm.on_sphere_origin(move |x, y| {
            if let Some(g) = g.borrow_mut().as_mut() {
                g.set_origin(x, y);
            }
            GLOBE_ORIGIN.with(|o| o.set(Some((x, y))));
        });
        let g = globe.clone();
        vm.on_legend_tap(move |i| {
            if let Some(g) = g.borrow_mut().as_mut() {
                g.face_category(i.max(0) as usize);
            }
        });
    }

    // ---- callbacks: practice loop ----------------------------------------
    // Current position / song from the latest snapshot (set in the loop).
    let cur_pos: Rc<Cell<u32>> = Rc::new(Cell::new(0));
    let cur_song: Rc<RefCell<Option<Arc<Song>>>> = Rc::new(RefCell::new(None));
    let loop_ab: Rc<Cell<(Option<u32>, Option<u32>)>> = Rc::new(Cell::new((None, None)));
    let show_loop = |ui: &AppWindow, ab: (Option<u32>, Option<u32>), dur: u32| {
        let p = ui.global::<PlayerVM>();
        let f = |v: Option<u32>| v.map(|ms| ms as f32 / dur.max(1) as f32).unwrap_or(-1.0);
        p.set_loop_a(f(ab.0));
        p.set_loop_b(f(ab.1));
    };
    {
        let vm = ui.global::<PlayerVM>();
        let (w, pos, song, ab, c, st) =
            (ui.as_weak(), cur_pos.clone(), cur_song.clone(), loop_ab.clone(), ctx.cmds.clone(), show_toast.clone());
        vm.on_set_loop_a(move || {
            let ui = w.unwrap();
            let Some(sg) = song.borrow().clone() else { return };
            let a = sg.onset_at_or_before(pos.get());
            let (_, mut b) = ab.get();
            if b.is_some_and(|b| b < a + MIN_LOOP_MS) {
                b = None;
            }
            ab.set((Some(a), b));
            if let Some(b) = b {
                let _ = c.send(Cmd::SetLoop(Some((a, b))));
                st("Loop section updated");
            } else {
                st("Loop start set — now tap B at the end of the section");
            }
            show_loop(&ui, ab.get(), sg.duration_ms);
        });
        let (w, pos, song, ab, c, st) =
            (ui.as_weak(), cur_pos.clone(), cur_song.clone(), loop_ab.clone(), ctx.cmds.clone(), show_toast.clone());
        vm.on_set_loop_b(move || {
            let ui = w.unwrap();
            let Some(sg) = song.borrow().clone() else { return };
            let a = ab.get().0.unwrap_or(0);
            let b = pos.get().min(sg.duration_ms);
            if b < a + MIN_LOOP_MS {
                st("B must be after A — play on a little, then tap B");
                return;
            }
            ab.set((Some(a), Some(b)));
            let _ = c.send(Cmd::SetLoop(Some((a, b))));
            st(&format!("Looping {} – {}", library::fmt_time(a), library::fmt_time(b)));
            show_loop(&ui, ab.get(), sg.duration_ms);
        });
        let (w, ab, c) = (ui.as_weak(), loop_ab.clone(), ctx.cmds.clone());
        vm.on_clear_loop(move || {
            ab.set((None, None));
            let _ = c.send(Cmd::SetLoop(None));
            show_loop(&w.unwrap(), (None, None), 1);
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
        vm.on_set_sound(|v| {
            let v = v.clamp(0, 2);
            player::SOUND.store(v as u8, std::sync::atomic::Ordering::Relaxed);
            prefs::set_i32("sound", v);
        });
        vm.on_set_speaker_volume(|v| {
            let v = v.round() as i32;
            crate::audio::VOLUME.store(v, std::sync::atomic::Ordering::Relaxed);
            prefs::set_i32("spkvol", v);
        });
        let c = ctx.cmds.clone();
        vm.on_led_test(move || {
            let _ = c.send(Cmd::LedTest);
        });
        let c = ctx.cmds.clone();
        vm.on_all_off(move || {
            let _ = c.send(Cmd::AllOff);
        });
        let (l, w, sd, se, sto) = (lib.clone(), ui.as_weak(), sd_ok.clone(), search.clone(), store.clone());
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
            lb.view = 0;
            ui.global::<LibraryVM>().set_view(0);
            *sto.borrow_mut() = Store::load(lb.root.clone());
            let err = lb.reload().err();
            push_library(&ui, &lb, &sto.borrow(), sd.get(), err);
            if sd.get() {
                se.lock().unwrap().index = None;
                build_index(&se, lb.root.clone());
            }
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

    ui.global::<AppVM>().on_set_show_touches(|v| prefs::set_bool("showtouch", v));

    // ---- shut down --------------------------------------------------------
    {
        let (w, c) = (ui.as_weak(), ctx.cmds.clone());
        ui.global::<AppVM>().on_shutdown(move || {
            let ui = w.unwrap();
            ui.global::<AppVM>().set_shutting_down(true);
            info!("ui: shut down requested");
            // Stop playback and clear every LED, give the transport time to
            // deliver it (USB pacing / BLE write), then cut power. The
            // shutting-down screen paints meanwhile.
            let _ = c.send(Cmd::AllOff);
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(700));
                board::power_off();
            });
        });
    }

    // ---- show -------------------------------------------------------------
    // The 1280-wide design scaled to the panel (Korvo: 0.625 on 800×480,
    // i.e. a 1280×768 logical canvas). Scale first, size after show (herdline).
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: board::UI_SCALE });
    ui.show().map_err(|e| anyhow::anyhow!("slint show: {e}"))?;
    window.set_size(PhysicalSize::new(board::WINDOW_WIDTH, board::WINDOW_HEIGHT));
    window.request_redraw();
    info!("ui: up ({} transport)", ctx.transport.label());

    let keys_model: Rc<VecModel<KeyView>> = Rc::new(VecModel::default());
    ui.global::<PlayerVM>().set_keys(keys_model.clone().into());
    // Sheet music (Play page "Sheet" view); the toggle forces a refresh.
    let mut sheet_view = sheet::Sheet::new(&ui.global::<PlayerVM>());
    let sheet_refresh = Rc::new(Cell::new(false));
    {
        let p = ui.global::<PlayerVM>();
        p.set_sheet(prefs::get_bool("sheet", false));
        p.set_sheet_colors(prefs::get_i32("shcolors").unwrap_or(1).clamp(0, 2));
        p.set_sheet_names(prefs::get_i32("shnames").unwrap_or(0).clamp(0, 2));
        let (w, r) = (ui.as_weak(), sheet_refresh.clone());
        p.on_set_sheet(move |on| {
            if let Some(ui) = w.upgrade() {
                ui.global::<PlayerVM>().set_sheet(on);
            }
            prefs::set_bool("sheet", on);
            r.set(true);
        });
        let (w, r) = (ui.as_weak(), sheet_refresh.clone());
        p.on_set_sheet_colors(move |c| {
            if let Some(ui) = w.upgrade() {
                ui.global::<PlayerVM>().set_sheet_colors(c);
            }
            prefs::set_i32("shcolors", c);
            r.set(true);
        });
        let (w, r) = (ui.as_weak(), sheet_refresh.clone());
        p.on_set_sheet_names(move |n| {
            if let Some(ui) = w.upgrade() {
                ui.global::<PlayerVM>().set_sheet_names(n);
            }
            prefs::set_i32("shnames", n);
            r.set(true);
        });
    }
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
    let mut last_search_rev = 0u32;
    let mut last_globe_frame = Instant::now();
    // Heartbeat: (since, frames drawn, touch polls with a finger, presses).
    let mut beat: (Instant, u32, u32, u32) = (Instant::now(), 0, 0, 0);
    let mut globe_title: Option<String> = None;
    // Globe perf: (render µs, slint draw+present µs, frames, since).
    let mut globe_stats: (u64, u64, u32, Instant) = (0, 0, 0, Instant::now());
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

        // ---- board buttons (Korvo: PLAY, SET, VOL±) ----
        for b in board::poll_buttons() {
            match b {
                board::Button::PlayPause => {
                    let _ = ctx.cmds.send(Cmd::PlayPause);
                }
                board::Button::Select => {
                    // Cycle Library → Play → Settings: every page is reachable
                    // from the keys alone (the Korvo's touch panel has a dead
                    // strip on the bench unit, see doc/piano_coach_firmware.md).
                    let a = ui.global::<AppVM>();
                    a.set_page((a.get_page() + 1) % 3);
                }
                board::Button::VolumeUp | board::Button::VolumeDown => {
                    let d = if b == board::Button::VolumeUp { 10 } else { -10 };
                    let v = (crate::audio::VOLUME.load(std::sync::atomic::Ordering::Relaxed) + d).clamp(0, 100);
                    crate::audio::VOLUME.store(v, std::sync::atomic::Ordering::Relaxed);
                    prefs::set_i32("spkvol", v);
                    ui.global::<SettingsVM>().set_speaker_volume(v as f32);
                    show_toast(&format!("Speaker volume {v}%"));
                }
                // The single-key gestures belong to the small boards.
                // The small boards' keys.
                _ => {}
            }
        }

        // ---- piano-key controls ----
        while let Ok(c) = ctx.controls.try_recv() {
            let current = sel_path.borrow().clone();
            let root = lib.borrow().root.clone();
            let next = match c {
                Control::PrevSong | Control::NextSong => current
                    .as_deref()
                    .and_then(|p| browse::song_step(p, if c == Control::NextSong { 1 } else { -1 })),
                Control::PrevFolder | Control::NextFolder => current.as_deref().and_then(|p| {
                    browse::folder_step(&root, p.parent()?, if c == Control::NextFolder { 1 } else { -1 })
                        .and_then(|d| browse::random_song(&d))
                }),
                Control::RandomSong => current.as_deref().and_then(|p| browse::random_song(p.parent()?)),
                _ => None,
            };
            let p = ui.global::<PlayerVM>();
            match c {
                Control::PlayPause => {
                    let _ = ctx.cmds.send(Cmd::PlayPause);
                }
                Control::Restart => {
                    let _ = ctx.cmds.send(Cmd::Restart);
                    ui.global::<AppVM>().set_page(1);
                }
                Control::TempoDown | Control::TempoUp => {
                    let t = (p.get_tempo() + if c == Control::TempoUp { 10 } else { -10 }).clamp(30, 200);
                    p.set_tempo(t);
                    prefs::set_i32("tempo", t);
                    let _ = ctx.cmds.send(Cmd::Tempo(t as u16));
                }
                Control::Mode(m) => {
                    let m = match m {
                        Mode::Listen => 0,
                        Mode::Learn => 1,
                        Mode::Follow => 2,
                    };
                    p.set_mode(m);
                    prefs::set_i32("mode", m);
                    let _ = ctx.cmds.send(Cmd::SetOptions(current_options(&ui)));
                }
                Control::Hands(h) => {
                    let h = match h {
                        piano_core::song::HandSel::Both => 0,
                        piano_core::song::HandSel::Right => 1,
                        piano_core::song::HandSel::Left => 2,
                    };
                    p.set_hands(h);
                    prefs::set_i32("hands", h);
                    let _ = ctx.cmds.send(Cmd::SetOptions(current_options(&ui)));
                }
                // Same as the ☆ on the song card (it toasts the result).
                Control::Bookmark => ui.global::<LibraryVM>().invoke_toggle_fav(),
                _ => {}
            }
            if let Some(path) = next {
                open_song(path, true);
            }
            if c != Control::Bookmark {
                show_toast(controls::label(c));
            }
        }

        // ---- transport status ----
        let connected = ctx.status.connected.load(std::sync::atomic::Ordering::Relaxed);
        // (Right away when the speaker plays the notes.)
        if autostart
            && (connected
                || player::SOUND.load(std::sync::atomic::Ordering::Relaxed) != player::SOUND_PIANO)
        {
            autostart = false;
            info!("ui: starting the boot song");
            let _ = ctx.cmds.send(Cmd::Restart);
        }
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

        // ---- search index finished building ----
        let srev = search.lock().unwrap().rev;
        if srev != last_search_rev {
            last_search_rev = srev;
            if ui.global::<LibraryVM>().get_searching() {
                run_search(&ui, &search, &query.borrow(), &results, &store.borrow());
            }
        }

        // ---- song globe: build / update (painted after Slint renders) ----
        let globe_showing = {
            let lvm = ui.global::<LibraryVM>();
            let a = ui.global::<AppVM>();
            a.get_page() == 0 && lvm.get_view() == 3 && !lvm.get_searching()
        };
        if globe_showing {
            let lvm = ui.global::<LibraryVM>();
            let srev = search.lock().unwrap().rev;
            if globe.borrow().is_none() || globe_rev.get() != srev {
                let index = search.lock().unwrap().index.clone();
                if let Some(index) = index {
                    let t = Instant::now();
                    let mut g = Globe::build(&index, &lib.borrow().root, &store.borrow());
                    g.select_path(sel_path.borrow().as_deref());
                    if let Some((x, y)) = GLOBE_ORIGIN.with(|o| o.get()) {
                        g.set_origin(x, y);
                    }
                    info!("globe: {} songs in {} categories ({:?})", g.songs.len(), g.categories.len(), t.elapsed());
                    let legend: Vec<LegendItem> = g
                        .categories
                        .iter()
                        .take(8)
                        .map(|(n, c)| LegendItem { name: n.as_str().into(), tint: slint::Color::from_rgb_u8(c[0], c[1], c[2]) })
                        .collect();
                    lvm.set_legend(ModelRc::new(VecModel::from(legend)));
                    *globe.borrow_mut() = Some(g);
                    globe_rev.set(srev);
                    globe_title = None;
                } else {
                    lvm.set_path_text("indexing the SD card…".into());
                }
            }
            if let Some(g) = globe.borrow_mut().as_mut() {
                if GLOBE_FAVS_DIRTY.swap(false, std::sync::atomic::Ordering::Relaxed) {
                    g.refresh_favs(&store.borrow());
                }
                if g.selected.map(|i| Some(&g.songs[i])) != Some(sel_path.borrow().as_ref()) {
                    g.select_path(sel_path.borrow().as_deref());
                }
                // Header line: the selected song, or how to use the globe.
                let title = g.selected_title().map(str::to_string);
                if title != globe_title {
                    lvm.set_path_text(
                        match &title {
                            Some(t) => format!("● {t}"),
                            None => format!("{} songs · drag to spin · tap a dot · tap a colour to turn to it", g.songs.len()),
                        }
                        .into(),
                    );
                    globe_title = title;
                }
            }
        }

        // ---- player snapshot ----
        if sheet_refresh.take() {
            last_rev = u64::MAX;
        }
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
            cur_pos.set(s.pos_ms);
            *cur_song.borrow_mut() = s.song.clone();
            if s.path != last_path {
                last_path = s.path.clone();
                *sel_path.borrow_mut() = s.path.clone();
                // A new song starts without a practice loop.
                loop_ab.set((None, None));
                show_loop(&ui, (None, None), 1);
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
            // Play history: a run starts.
            let running = |st: State| matches!(st, State::Playing | State::Waiting);
            if running(s.state) && matches!(last_state, State::Stopped | State::Finished) {
                if let Some(path) = &s.path {
                    let mut sto = store.borrow_mut();
                    let rel = sto.rel(path);
                    sto.progress.record_play(&rel);
                    sto.save();
                    ui.global::<LibraryVM>().set_sel_scores(score_line(&sto, path).into());
                }
            }
            // Scoring: a Learn / Follow run that covered (almost) the whole song.
            if s.state == State::Finished && last_state != State::Finished && s.opts.mode != Mode::Listen {
                let st = s.stats;
                let acc = st.accuracy().min(100) as u8;
                let covered = match s.opts.mode {
                    Mode::Learn => st.steps_done * 10 >= st.steps_total * 9,
                    _ => (st.hits + st.misses) * 10 >= st.steps_total * 9,
                };
                let kind = if s.opts.mode == Mode::Learn { ScoreKind::Learn } else { ScoreKind::Follow };
                let new_best = match (&s.path, covered) {
                    (Some(path), true) => {
                        let mut sto = store.borrow_mut();
                        let rel = sto.rel(path);
                        let nb = sto.progress.record_score(&rel, kind, acc);
                        sto.save();
                        ui.global::<LibraryVM>().set_sel_scores(score_line(&sto, path).into());
                        nb
                    }
                    _ => false,
                };
                let stars = "★".repeat(progress::stars(acc) as usize);
                show_toast(&if new_best {
                    format!("New best! {stars} {acc}% — best streak {}", st.best_streak)
                } else {
                    format!("Finished — {stars} {acc}% accuracy, best streak {}", st.best_streak)
                });
                // Refresh list badges.
                let lb = lib.borrow();
                if lb.view != 3 {
                    push_library(&ui, &lb, &store.borrow(), sd_ok.get(), None);
                }
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
            p.set_looping(s.loop_range.is_some());
            p.set_loop_text(
                match s.loop_range {
                    Some(_) => format!("#{}", s.loops + 1),
                    None => String::new(),
                }
                .into(),
            );
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

            // Sheet music: cursor, page turns, the notes to play now.
            let sheet_on = p.get_sheet();
            if sheet_on {
                let learner = s.opts.mode != Mode::Listen;
                let now = match (&s.step, s.state) {
                    (_, State::Stopped) if s.pos_ms == 0 => None,
                    (Some(st), _) if s.opts.mode == Mode::Learn => Some(st.ms),
                    _ => s.song.as_ref().map(|song| song.onset_at_or_before(s.pos_ms)),
                };
                let staves = match (learner, s.opts.hands) {
                    (true, piano_core::song::HandSel::Right) => 1,
                    (true, piano_core::song::HandSel::Left) => 2,
                    _ => 3,
                };
                // Full screen below the 52 px control band (ui/player.slint).
                let area = (1280.0, board::WINDOW_HEIGHT as f32 / board::UI_SCALE - 52.0);
                sheet_view.update(&p, s.song.as_ref(), s.pos_ms, now, staves, area);
            }

            // Falling notes (not while the sheet replaces them).
            let mut rows: Vec<RollNote> = Vec::new();
            if let Some(song) = s.song.as_ref().filter(|_| !sheet_on) {
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
        let touch_now = board::poll_touch(display.touch);
        if touch_now.is_some() {
            beat.2 += 1;
            if last_touch.is_none() {
                beat.3 += 1;
            }
        }
        if beat.0.elapsed() >= Duration::from_secs(5) {
            log::debug!(
                "ui: alive — {} frames, finger seen in {} polls, {} presses, page {}, touching {:?}",
                beat.1,
                beat.2,
                beat.3,
                ui.global::<AppVM>().get_page(),
                touch_now.map(|p| (p.x as i32, p.y as i32))
            );
            beat = (Instant::now(), 0, 0, 0);
        }
        // Touch diagnostic dot follows the reported (unamplified) position.
        {
            let a = ui.global::<AppVM>();
            if a.get_show_touches() {
                if let Some(p) = touch_now {
                    a.set_touch_x(p.x);
                    a.set_touch_y(p.y);
                }
                if a.get_touching() != touch_now.is_some() {
                    a.set_touching(touch_now.is_some());
                }
            }
        }
        match (touch_now, last_touch) {
            (Some(pos), None) => {
                log::debug!("touch: down at ({:.0}, {:.0}) logical", pos.x, pos.y);
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
                log::debug!("touch: up at ({:.0}, {:.0}) logical", pos.x, pos.y);
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
        // Wipe globe dots Slint does not know about: when the globe is
        // hidden (Slint assumes its empty panel is still there) or moved.
        // Switching the repaint-buffer type clears Slint's partial-render
        // cache, so the next render repaints the whole window.
        let globe_now = {
            let lvm = ui.global::<LibraryVM>();
            ui.global::<AppVM>().get_page() == 0 && lvm.get_view() == 3 && !lvm.get_searching()
        };
        let mut full_repaint = false;
        if let Some(g) = globe.borrow_mut().as_mut() {
            full_repaint = (!globe_now && g.hide()) || g.take_stale_paint();
        }
        if full_repaint {
            window.request_redraw();
        }
        let t_draw = Instant::now();
        let mut band: Option<(usize, usize)> = None;
        let redrew = window.draw_if_needed(|renderer| {
            if full_repaint {
                renderer.set_repaint_buffer_type(RepaintBufferType::SwappedBuffers);
                renderer.set_repaint_buffer_type(RepaintBufferType::ReusedBuffer);
            }
            renderer.set_rendering_rotation(board::RENDERING_ROTATION);
            let region = renderer.render(shadow.as_mut_slice(), H_RES);
            let o = region.bounding_box_origin();
            let sz = region.bounding_box_size();
            if sz.height > 0 && sz.width > 0 {
                let y0 = o.y.max(0) as usize;
                band = Some((y0, (y0 + sz.height as usize).min(V_RES)));
            }
        });
        // Globe: paint over Slint's frame when it moved or Slint touched its
        // rows. Paused under overlays (toast, dialogs) so they stay visible.
        let mut globe_band: Option<(usize, usize)> = None;
        if globe_showing && globe_now {
            let a = ui.global::<AppVM>();
            let overlay = !a.get_toast().is_empty() || a.get_confirm_shutdown() || a.get_shutting_down();
            let dt = last_globe_frame.elapsed().as_secs_f32() * 1000.0;
            last_globe_frame = Instant::now();
            if let Some(g) = globe.borrow_mut().as_mut() {
                let moved = g.step(dt.min(100.0));
                let touched = match (band, g.fb_rows()) {
                    (Some((a0, a1)), Some((b0, b1))) => a0 < b1 && b0 < a1,
                    _ => false,
                };
                if redrew && touched {
                    g.invalidate();
                }
                if !overlay && (moved || (redrew && touched)) {
                    let t = Instant::now();
                    globe_band = g.paint(shadow.as_mut_slice());
                    globe_stats.0 += t.elapsed().as_micros() as u64;
                    globe_stats.2 += 1;
                }
            }
        } else {
            last_globe_frame = Instant::now();
        }
        let band = match (band, globe_band) {
            (Some((a0, a1)), Some((b0, b1))) => Some((a0.min(b0), a1.max(b1))),
            (b, None) | (None, b) => b,
        };
        let redrew = redrew || globe_band.is_some();
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
        if redrew {
            globe_stats.1 += t_draw.elapsed().as_micros() as u64;
            beat.1 += 1;
        }
        if globe_stats.2 > 0 && globe_stats.3.elapsed() >= Duration::from_secs(3) {
            let n = globe_stats.2 as u64;
            log::debug!(
                "globe: {:.1} fps · paint {} ms · slint draw + present {} ms per frame",
                n as f32 / globe_stats.3.elapsed().as_secs_f32(),
                globe_stats.0 / n / 1000,
                globe_stats.1 / n / 1000
            );
            globe_stats = (0, 0, 0, Instant::now());
        }
        std::thread::sleep(Duration::from_millis(if redrew || window.has_active_animations() { 4 } else { 12 }));
    }
}

//! Minimal UI for the small SPI-panel boards (`ui_mini`): a wheel menu over
//! the flash song pack, a song menu (Learn / Follow / Listen, hands, tempo),
//! a settings list, and a play screen. Slint renders line by line straight
//! into small DMA bands (no framebuffer: the ESP32-C3 has no RAM for one).
//!
//! Input, unified into [`Nav`]:
//! * the board key (BOOT): tap = next, double tap = previous, hold = OK;
//!   on the play screen tap = pause/resume, double tap = restart, hold =
//!   back;
//! * the piano's four lowest white keys while nothing plays: A0 ◀, B0 ▶,
//!   C1 OK, D1 back (see `player::NAV_*`);
//! * touch (round board): tap a row, swipe up/down to scroll, swipe right
//!   to go back, tap the play screen to pause/resume.

use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{info, warn};
use piano_core::engine::{Mode, Options, State};
use piano_core::library::{self, fmt_time};
use piano_core::progress::stars;
use piano_core::song::{key_name, HandSel};
use piano_core::songpack::{path_key, Pack};
use slint::platform::software_renderer::{LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel};
use slint::platform::{Platform, PointerEventButton, WindowEvent};
use slint::{ComponentHandle, LogicalPosition, Model, ModelRc, PhysicalSize, VecModel};

use crate::board::{self, spi_lcd::SpiLcd, Button, LCD_HEIGHT, LCD_WIDTH};
use crate::link::Status;
use crate::player::{self, Cmd, Shared};
use crate::{prefs, songs};

slint::include_modules!();

/// UI thread stack: Slint's renderer recurses through the item tree
/// (measured ~6 KB used on the round board).
pub const STACK: usize = 14 * 1024;
/// Theme.bg #0e1116 as RGB565.
const BG565: u16 = 0x0882;
/// Lines per SPI transfer (a 4.8 KB DMA buffer on the round board).
const BAND: usize = 10;

pub struct Ctx {
    pub cmds: Sender<Cmd>,
    pub shared: Shared,
    pub status: Arc<Status>,
    /// Piano menu keys (player::NAV_*), while nothing plays.
    pub nav: Receiver<u8>,
}

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

// ---- rendering: Slint lines → SPI bands ------------------------------------

/// Collects consecutive lines with the same x-range into one band and sends
/// it when the range changes, the band is full or the frame ends.
struct Bands<'a> {
    lcd: &'a SpiLcd,
    buf: &'a mut [Rgb565Pixel],
    y0: usize,
    lines: usize,
    range: core::ops::Range<usize>,
}

impl Bands<'_> {
    fn flush(&mut self) {
        if self.lines == 0 {
            return;
        }
        let w = self.range.len();
        let n = w * self.lines;
        // Rgb565Pixel is a transparent u16.
        let px = unsafe { core::slice::from_raw_parts_mut(self.buf.as_mut_ptr() as *mut u16, n) };
        board::lcd_draw(self.lcd, self.range.start, self.y0, w, self.lines, px);
        self.lines = 0;
    }
}

impl LineBufferProvider for &mut Bands<'_> {
    type TargetPixel = Rgb565Pixel;
    fn process_line(
        &mut self,
        line: usize,
        range: core::ops::Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        if self.lines > 0 && (range != self.range || line != self.y0 + self.lines || self.lines == BAND) {
            self.flush();
        }
        if self.lines == 0 {
            self.y0 = line;
            self.range = range.clone();
        }
        let w = range.len();
        let off = self.lines * w;
        render_fn(&mut self.buf[off..off + w]);
        self.lines += 1;
    }
}

// ---- menus --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum Nav {
    Prev(usize),
    Next(usize),
    Ok,
    Back,
    /// Touch tap at panel coordinates (forwarded to Slint).
    Tap(f32, f32),
}

#[derive(Clone, Copy, PartialEq)]
enum Act {
    Back,
    Dir(u32),
    Song(u32),
    Settings,
    Start(Mode),
    Hands,
    Tempo,
    Easy,
    LedOff,
    Accompany,
    Lead,
    Brightness,
    LedTest,
    AllOff,
    Sleep,
    Info,
}

struct Item {
    icon: &'static str,
    title: String,
    sub: String,
    value: String,
    act: Act,
}

impl Item {
    fn new(icon: &'static str, title: impl Into<String>, act: Act) -> Item {
        Item {
            icon,
            title: title.into(),
            sub: String::new(),
            value: String::new(),
            act,
        }
    }
    fn sub(mut self, s: impl Into<String>) -> Item {
        self.sub = s.into();
        self
    }
    fn value(mut self, v: impl Into<String>) -> Item {
        self.value = v.into();
        self
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Page {
    Browse(u32),
    Song(u32),
    Settings,
}

struct Level {
    page: Page,
    focus: usize,
}

/// Best scores per song in NVS: key "s" + FNV of the pack path; value =
/// learn % | follow % << 8 (0xff = none).
fn best(pack: &Pack, song: u32) -> (Option<u8>, Option<u8>) {
    let v = prefs::get_i32(&format!("s{:08x}", path_key(&pack.path(song)))).unwrap_or(-1);
    let get = |b: i32| (b != 0xff).then_some(b as u8);
    if v < 0 {
        (None, None)
    } else {
        (get(v & 0xff), get((v >> 8) & 0xff))
    }
}

/// Record a finished run; true if it is a new best.
fn record(pack: &Pack, song: u32, mode: Mode, acc: u8) -> bool {
    let (mut l, mut f) = best(pack, song);
    let slot = if mode == Mode::Learn { &mut l } else { &mut f };
    if slot.is_some_and(|b| b >= acc) {
        return false;
    }
    *slot = Some(acc);
    let v = l.unwrap_or(0xff) as i32 | (f.unwrap_or(0xff) as i32) << 8;
    prefs::set_i32(&format!("s{:08x}", path_key(&pack.path(song))), v);
    true
}

fn star_str(acc: Option<u8>) -> String {
    match acc {
        Some(a) => "★".repeat(stars(a) as usize) + &"☆".repeat(3 - stars(a) as usize),
        None => String::new(),
    }
}

fn hands_label(h: HandSel) -> &'static str {
    match h {
        HandSel::Both => "Both",
        HandSel::Right => "Right",
        HandSel::Left => "Left",
    }
}

fn mode_label(m: Mode) -> &'static str {
    match m {
        Mode::Listen => "Listen",
        Mode::Learn => "Learn",
        Mode::Follow => "Follow",
    }
}

fn on_off(b: bool) -> &'static str {
    if b {
        "On"
    } else {
        "Off"
    }
}

struct App {
    pack: Option<&'static Pack<'static>>,
    pack_err: String,
    stack: Vec<Level>,
    items: Vec<Item>,
    opts: Options,
    brightness: i32,
    playing: Option<u32>,
    cmds: Sender<Cmd>,
}

impl App {
    fn level(&self) -> &Level {
        self.stack.last().unwrap()
    }

    fn build(&mut self) {
        let page = self.level().page;
        let mut v = Vec::new();
        match page {
            Page::Browse(dir) => {
                if dir == 0 {
                    v.push(Item::new("settings", "Settings", Act::Settings));
                } else {
                    v.push(Item::new("back", "Back", Act::Back));
                }
                match self.pack {
                    Some(p) => {
                        for i in p.node(dir).children() {
                            let n = p.node(i);
                            let e = library::entry(n.name, n.is_dir());
                            if n.is_dir() {
                                v.push(Item::new("folder", e.title, Act::Dir(i)).sub(format!("{} songs", n.songs())));
                            } else {
                                let (l, f) = best(p, i);
                                v.push(
                                    Item::new("note", e.title, Act::Song(i))
                                        .sub(e.subtitle)
                                        .value(star_str(l.max(f))),
                                );
                            }
                        }
                    }
                    None => v.push(Item::new("close", "No songs", Act::Info).sub(self.pack_err.clone())),
                }
            }
            Page::Song(song) => {
                let (l, f) = self.pack.map(|p| best(p, song)).unwrap_or((None, None));
                let pct = |a: Option<u8>| a.map(|a| format!("best {a}%")).unwrap_or_default();
                v.push(Item::new("learn", "Learn", Act::Start(Mode::Learn)).sub(pct(l)).value(star_str(l)));
                v.push(Item::new("follow", "Follow", Act::Start(Mode::Follow)).sub(pct(f)).value(star_str(f)));
                v.push(Item::new("listen", "Listen", Act::Start(Mode::Listen)));
                v.push(Item::new("library", "Hands", Act::Hands).value(hands_label(self.opts.hands)));
                v.push(Item::new("bolt", "Tempo", Act::Tempo).value(format!("{}%", self.opts.tempo_pct)));
                v.push(Item::new("back", "Back", Act::Back));
            }
            Page::Settings => {
                let o = self.opts;
                v.push(Item::new("back", "Back", Act::Back));
                v.push(Item::new("check", "Easy mode", Act::Easy).sub("any key advances").value(on_off(o.easy)));
                v.push(Item::new("bulb", "Hint off on hit", Act::LedOff).value(on_off(o.led_off_on_hit)));
                v.push(Item::new("note", "Accompaniment", Act::Accompany).value(on_off(o.accompany)));
                v.push(
                    Item::new("follow", "Follow lead", Act::Lead)
                        .sub("hint before the note")
                        .value(format!("{:.1} s", o.lead_ms as f32 / 1000.0)),
                );
                v.push(Item::new("bulb", "Brightness", Act::Brightness).value(format!("{}%", self.brightness)));
                v.push(Item::new("bolt", "Light all keys", Act::LedTest).sub("LED test, 4 s"));
                v.push(Item::new("erase", "All LEDs off", Act::AllOff));
                let songs = self.pack.map(|p| p.root().songs()).unwrap_or(0);
                v.push(
                    Item::new("home", board::CAPS.name, Act::Info)
                        .sub(format!("piano-coach {} · {songs} songs", env!("CARGO_PKG_VERSION"))),
                );
                v.push(Item::new("power", "Sleep", Act::Sleep).sub(if board::CAPS.touch {
                    "touch or RESET wakes"
                } else {
                    "BOOT or RESET wakes"
                }));
            }
        }
        self.items = v;
        let f = self.level().focus.min(self.items.len().saturating_sub(1));
        self.stack.last_mut().unwrap().focus = f;
    }

    fn header(&self) -> String {
        match (self.level().page, self.pack) {
            (Page::Browse(0), _) | (_, None) => "Piano Coach".into(),
            (Page::Browse(d), Some(p)) => library::entry(p.node(d).name, true).title,
            (Page::Song(s), Some(p)) => library::entry(p.node(s).name, false).title,
            (Page::Settings, _) => "Settings".into(),
        }
    }

    fn push(&mut self, page: Page, focus: usize) {
        self.stack.push(Level { page, focus });
        self.build();
    }

    fn pop(&mut self) {
        if self.stack.len() > 1 {
            self.stack.pop();
            self.build();
        }
    }

    fn step(&mut self, d: isize) {
        let n = self.items.len() as isize;
        if n == 0 {
            return;
        }
        let l = self.stack.last_mut().unwrap();
        l.focus = ((l.focus as isize + d).rem_euclid(n)) as usize;
    }

    fn set_opts(&mut self, o: Options) {
        self.opts = o;
        let _ = self.cmds.send(Cmd::SetOptions(o));
    }

    /// OK on the focused item. Returns a toast, if any.
    fn activate(&mut self) -> Option<String> {
        let focus = self.level().focus;
        let act = self.items.get(focus)?.act;
        let mut o = self.opts;
        match act {
            Act::Back => self.pop(),
            Act::Dir(d) => self.push(Page::Browse(d), 1),
            Act::Song(s) => self.push(Page::Song(s), 0),
            Act::Settings => self.push(Page::Settings, 1),
            Act::Start(mode) => {
                let Page::Song(s) = self.level().page else { return None };
                o.mode = mode;
                prefs::set_i32("mode", mode as i32);
                self.set_opts(o);
                let _ = self.cmds.send(Cmd::Load(songs::pack_path(s)));
                let _ = self.cmds.send(Cmd::Restart);
                self.playing = Some(s);
            }
            Act::Hands => {
                o.hands = match o.hands {
                    HandSel::Both => HandSel::Right,
                    HandSel::Right => HandSel::Left,
                    HandSel::Left => HandSel::Both,
                };
                prefs::set_i32("hands", o.hands as i32);
                self.set_opts(o);
            }
            Act::Tempo => {
                o.tempo_pct = match o.tempo_pct {
                    t if t < 60 => 60,
                    t if t < 75 => 75,
                    t if t < 90 => 90,
                    t if t < 100 => 100,
                    t if t < 120 => 120,
                    _ => 50,
                };
                prefs::set_i32("tempo", o.tempo_pct as i32);
                self.set_opts(o);
            }
            Act::Easy => {
                o.easy = !o.easy;
                prefs::set_bool("easy", o.easy);
                self.set_opts(o);
            }
            Act::LedOff => {
                o.led_off_on_hit = !o.led_off_on_hit;
                prefs::set_bool("ledoff", o.led_off_on_hit);
                self.set_opts(o);
            }
            Act::Accompany => {
                o.accompany = !o.accompany;
                prefs::set_bool("accompany", o.accompany);
                self.set_opts(o);
            }
            Act::Lead => {
                o.lead_ms = match o.lead_ms {
                    l if l < 400 => 400,
                    l if l < 700 => 700,
                    l if l < 1000 => 1000,
                    l if l < 1500 => 1500,
                    _ => 200,
                };
                prefs::set_i32("lead", o.lead_ms as i32);
                self.set_opts(o);
            }
            Act::Brightness => {
                self.brightness = if self.brightness >= 100 { 20 } else { self.brightness + 20 };
                prefs::set_i32("brightness", self.brightness);
                board::set_backlight(self.brightness);
            }
            Act::LedTest => {
                let _ = self.cmds.send(Cmd::LedTest);
                return Some("All keys lit for 4 s".into());
            }
            Act::AllOff => {
                let _ = self.cmds.send(Cmd::AllOff);
                return Some("LEDs off".into());
            }
            Act::Sleep => {
                let _ = self.cmds.send(Cmd::AllOff);
                std::thread::sleep(Duration::from_millis(300));
                board::power_off();
            }
            Act::Info => return None,
        }
        self.build();
        None
    }
}

// ---- touch gestures ---------------------------------------------------------

#[derive(Default)]
struct Gesture {
    start: Option<(Instant, f32, f32)>,
    last: (f32, f32),
}

impl Gesture {
    /// Feed the current touch point; returns a finished gesture.
    fn feed(&mut self, p: Option<(u16, u16)>) -> Option<Nav> {
        match (p, self.start) {
            (Some((x, y)), None) => {
                self.start = Some((Instant::now(), x as f32, y as f32));
                self.last = (x as f32, y as f32);
                None
            }
            (Some((x, y)), Some(_)) => {
                self.last = (x as f32, y as f32);
                None
            }
            (None, Some((t, x0, y0))) => {
                self.start = None;
                let (dx, dy) = (self.last.0 - x0, self.last.1 - y0);
                if dx.abs() < 14.0 && dy.abs() < 14.0 {
                    (t.elapsed() < Duration::from_millis(700)).then_some(Nav::Tap(x0, y0))
                } else if dx.abs() > dy.abs() {
                    (dx > 40.0).then_some(Nav::Back)
                } else {
                    // About one row per 40 px; a flick moves at least one.
                    let n = ((dy.abs() / 40.0).round() as usize).max(1);
                    Some(if dy < 0.0 { Nav::Next(n) } else { Nav::Prev(n) })
                }
            }
            (None, None) => None,
        }
    }
}

// ---- main loop ----------------------------------------------------------------

pub fn run(ctx: Ctx) -> anyhow::Result<()> {
    let lcd = board::lcd_init(BG565)?;
    let brightness = prefs::get_i32("brightness").unwrap_or(80).clamp(10, 100);
    board::set_backlight(brightness);

    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    slint::platform::set_platform(Box::new(EmbeddedPlatform {
        window: window.clone(),
        start: Instant::now(),
    }))
    .map_err(|e| anyhow::anyhow!("set_platform: {e:?}"))?;
    let ui = MiniApp::new().map_err(|e| anyhow::anyhow!("slint ui: {e}"))?;
    window.set_size(PhysicalSize::new(LCD_WIDTH as u32, LCD_HEIGHT as u32));
    ui.show().map_err(|e| anyhow::anyhow!("show: {e}"))?;
    let vm = ui.global::<MiniVM>();
    vm.set_round(board::ROUND);

    let (pack, pack_err) = match songs::open_pack() {
        Ok(p) => (Some(p), String::new()),
        Err(e) => {
            warn!("songs: {e}");
            (None, e)
        }
    };
    let mut app = App {
        pack,
        pack_err,
        stack: vec![Level {
            page: Page::Browse(0),
            focus: 1,
        }],
        items: Vec::new(),
        opts: player::saved_options(),
        brightness,
        playing: None,
        cmds: ctx.cmds.clone(),
    };
    app.build();

    // Touch taps reach Slint's TouchAreas; they report back here.
    let taps: Rc<std::cell::RefCell<Vec<Nav>>> = Rc::new(Default::default());
    {
        let t = taps.clone();
        vm.on_tap_row(move |i| {
            t.borrow_mut().push(match i {
                0 => Nav::Prev(1),
                2 => Nav::Next(1),
                _ => Nav::Ok,
            })
        });
        let t = taps.clone();
        vm.on_tap_play(move || t.borrow_mut().push(Nav::Ok));
    }

    let band = board::spi_lcd::alloc_band(LCD_WIDTH * BAND);
    // Rgb565Pixel is a transparent u16.
    let band: &mut [Rgb565Pixel] =
        unsafe { core::slice::from_raw_parts_mut(band.as_mut_ptr() as *mut Rgb565Pixel, band.len()) };
    let mut bands = Bands {
        lcd: &lcd,
        buf: band,
        y0: 0,
        lines: 0,
        range: 0..0,
    };

    let mut gesture = Gesture::default();
    let mut menu_dirty = true;
    let mut on_play = false;
    let mut last_rev = u64::MAX;
    let mut last_state = State::Stopped;
    let mut toast_until: Option<Instant> = None;
    let mut result = String::new();
    let mut last_conn = None;
    let mut heap_log = Instant::now();
    let rows = Rc::new(VecModel::<Row>::from(vec![Row::default(), Row::default(), Row::default()]));
    vm.set_rows(ModelRc::from(rows.clone()));

    info!("ui: mini UI up ({}x{}, {})", LCD_WIDTH, LCD_HEIGHT, if board::ROUND { "round" } else { "bar" });
    log_heap("ui");
    unsafe {
        let _ = esp_idf_svc::sys::esp_task_wdt_add(core::ptr::null_mut());
    }
    loop {
        unsafe {
            let _ = esp_idf_svc::sys::esp_task_wdt_reset();
        }
        slint::platform::update_timers_and_animations();

        // ---- input ----------------------------------------------------------
        let mut navs: Vec<Nav> = Vec::new();
        let mut started = false;
        for b in board::poll_buttons() {
            navs.push(match (on_play, b) {
                (false, Button::KeyTap) => Nav::Next(1),
                (false, Button::KeyDoubleTap) => Nav::Prev(1),
                (false, Button::KeyHold) => Nav::Ok,
                (true, Button::KeyTap) => Nav::Ok,
                (true, Button::KeyDoubleTap) => Nav::Prev(1),
                (true, Button::KeyHold) => Nav::Back,
                _ => continue,
            });
        }
        while let Ok(k) = ctx.nav.try_recv() {
            navs.push(match k {
                player::NAV_PREV => Nav::Prev(1),
                player::NAV_NEXT => Nav::Next(1),
                player::NAV_OK => Nav::Ok,
                _ => Nav::Back,
            });
        }
        if board::CAPS.touch {
            if let Some(g) = gesture.feed(board::touch_point()) {
                navs.push(g);
            }
        }

        for n in navs {
            if let Nav::Tap(x, y) = n {
                let position = LogicalPosition::new(x, y);
                window.dispatch_event(WindowEvent::PointerPressed {
                    position,
                    button: PointerEventButton::Left,
                });
                window.dispatch_event(WindowEvent::PointerReleased {
                    position,
                    button: PointerEventButton::Left,
                });
                window.dispatch_event(WindowEvent::PointerExited);
                continue;
            }
            started |= handle(n, &mut app, &mut on_play, &mut menu_dirty, &ctx, &mut toast_until, &vm);
        }
        // Row / play-screen taps queued by Slint callbacks above.
        let queued: Vec<Nav> = taps.borrow_mut().drain(..).collect();
        for n in queued {
            started |= handle(n, &mut app, &mut on_play, &mut menu_dirty, &ctx, &mut toast_until, &vm);
        }
        if started {
            // A new run: refresh everything and score its end once.
            last_rev = u64::MAX;
            last_state = State::Stopped;
        }

        // ---- connection -------------------------------------------------------
        let connected = ctx.status.connected.load(Ordering::Relaxed);
        if last_conn != Some(connected) {
            last_conn = Some(connected);
            vm.set_connected(connected);
            let name = ctx.status.name.lock().unwrap().clone();
            show_toast(
                &vm,
                &mut toast_until,
                if connected { format!("Piano connected · {name}") } else { "Searching for the piano…".into() },
            );
        }

        // ---- menu ---------------------------------------------------------------
        if menu_dirty && !on_play {
            menu_dirty = false;
            let n = app.items.len();
            let f = app.level().focus;
            let row = |i: Option<usize>| -> Row {
                match i.and_then(|i| app.items.get(i)) {
                    Some(it) => Row {
                        icon: it.icon.into(),
                        title: it.title.as_str().into(),
                        sub: it.sub.as_str().into(),
                        value: it.value.as_str().into(),
                    },
                    None => Row::default(),
                }
            };
            // Neighbours wrap around, as the wheel does (none if 1 item).
            let prev = (n > 1).then(|| (f + n - 1) % n);
            let next = (n > 2).then(|| (f + 1) % n);
            rows.set_row_data(0, row(prev));
            rows.set_row_data(1, row(Some(f)));
            rows.set_row_data(2, row(next));
            vm.set_header(app.header().into());
            vm.set_footer(
                if n > 1 {
                    if board::CAPS.touch {
                        format!("{} / {}", f + 1, n)
                    } else {
                        format!("{} / {}  ·  tap ›  2× ‹  hold OK", f + 1, n)
                    }
                } else {
                    String::new()
                }
                .into(),
            );
            vm.set_page(0);
        }

        // ---- play screen --------------------------------------------------------
        if on_play {
            vm.set_page(1);
            let snap = ctx.shared.lock().unwrap().clone();
            if snap.rev != last_rev {
                last_rev = snap.rev;
                let o = snap.opts;
                vm.set_mode(
                    format!("{} · {} · {}%", mode_label(o.mode), hands_label(o.hands), o.tempo_pct).into(),
                );
                if let (Some(p), Some(s)) = (app.pack, app.playing) {
                    vm.set_song(library::entry(p.node(s).name, false).title.into());
                }
                let dur = snap.song.as_ref().map(|s| s.duration_ms).unwrap_or(0);
                vm.set_progress(if dur > 0 { snap.pos_ms as f32 / dur as f32 } else { 0.0 });
                vm.set_time(format!("{} / {}", fmt_time(snap.pos_ms), fmt_time(dur)).into());
                vm.set_state(match snap.state {
                    State::Stopped => 0,
                    State::Playing => 1,
                    State::Waiting => 2,
                    State::Paused => 3,
                    State::Finished => 4,
                });
                let st = snap.stats;
                vm.set_stats(
                    match o.mode {
                        Mode::Listen => String::new(),
                        Mode::Learn => format!("✓ {}   ✕ {}", st.correct, st.wrong),
                        Mode::Follow => format!("✓ {}   ✕ {}", st.hits, st.misses + st.wrong),
                    }
                    .into(),
                );
                // The cue: what to play next, else the state.
                let keys: Vec<u8> = if !snap.waiting.is_empty() {
                    snap.waiting.clone()
                } else if o.mode != Mode::Listen {
                    snap.step.as_ref().map(|s| s.keys.clone()).unwrap_or_default()
                } else {
                    Vec::new()
                };
                let (cue, color, hint) = if let Some(err) = &snap.load_error {
                    ("✕".to_string(), slint::Color::from_rgb_u8(0xff, 0x5a, 0x6a), err.clone())
                } else {
                    match snap.state {
                        State::Paused => ("■".into(), slint::Color::from_rgb_u8(0x8d, 0x99, 0xa6), "Paused".into()),
                        State::Stopped => ("♪".into(), slint::Color::from_rgb_u8(0x8d, 0x99, 0xa6), "Loading…".into()),
                        _ if !keys.is_empty() => {
                            let mut k = keys;
                            k.sort_unstable();
                            k.truncate(4);
                            let names: Vec<String> = k.iter().map(|&k| key_name(k)).collect();
                            (
                                names.join(" "),
                                slint::Color::from_rgb_u8(0xff, 0x3b, 0x4f),
                                if snap.state == State::Waiting {
                                    "Play the lit keys".into()
                                } else {
                                    "Next".into()
                                },
                            )
                        }
                        _ => ("♪".into(), slint::Color::from_rgb_u8(0x34, 0xd3, 0xc2), "Listening".into()),
                    }
                };
                vm.set_cue(cue.into());
                vm.set_cue_color(color);
                vm.set_hint(
                    if connected { hint } else { format!("{hint} · piano not connected") }.into(),
                );
                if snap.state == State::Finished && last_state != State::Finished {
                    let acc = st.accuracy().min(100) as u8;
                    if o.mode == Mode::Listen {
                        vm.set_stars(3);
                        result = "Finished".into();
                    } else {
                        let new_best = match (app.pack, app.playing) {
                            (Some(p), Some(s)) => record(p, s, o.mode, acc),
                            _ => false,
                        };
                        vm.set_stars(stars(acc) as i32);
                        result = if new_best { format!("{acc}%  ·  New best!") } else { format!("{acc}%") };
                    }
                    vm.set_result(result.as_str().into());
                }
                last_state = snap.state;
            }
        }

        if toast_until.is_some_and(|t| Instant::now() >= t) {
            toast_until = None;
            vm.set_toast("".into());
        }

        // ---- render -------------------------------------------------------------
        let redrew = window.draw_if_needed(|renderer| {
            renderer.render_by_line(&mut bands);
            bands.flush();
        });

        if heap_log.elapsed() > Duration::from_secs(10) {
            heap_log = Instant::now();
            log_heap("ui");
        }
        std::thread::sleep(Duration::from_millis(if redrew || window.has_active_animations() { 10 } else { 25 }));
    }
}

/// Free heap, its low-water mark and the largest block (fragmentation).
fn log_heap(what: &str) {
    use esp_idf_svc::sys as sys;
    unsafe {
        info!(
            "{what}: heap free {} KB (min {} KB, largest block {} KB); ui stack left {} B",
            sys::esp_get_free_heap_size() / 1024,
            sys::esp_get_minimum_free_heap_size() / 1024,
            sys::heap_caps_get_largest_free_block(sys::MALLOC_CAP_8BIT) / 1024,
            sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut())
        );
    }
}

fn show_toast(vm: &MiniVM, until: &mut Option<Instant>, text: String) {
    vm.set_toast(text.into());
    *until = Some(Instant::now() + Duration::from_millis(2200));
}

fn handle(
    n: Nav,
    app: &mut App,
    on_play: &mut bool,
    menu_dirty: &mut bool,
    ctx: &Ctx,
    toast_until: &mut Option<Instant>,
    vm: &MiniVM,
) -> bool {
    if *on_play {
        let finished = ctx.shared.lock().unwrap().state == State::Finished;
        match n {
            Nav::Ok if finished => {
                let _ = ctx.cmds.send(Cmd::Restart);
            }
            Nav::Ok => {
                let _ = ctx.cmds.send(Cmd::PlayPause);
            }
            Nav::Prev(_) => {
                let _ = ctx.cmds.send(Cmd::Restart);
            }
            Nav::Back => {
                let _ = ctx.cmds.send(Cmd::Stop);
                *on_play = false;
                app.build(); // best scores may have changed
                *menu_dirty = true;
            }
            _ => {}
        }
        return false;
    }
    *menu_dirty = true;
    match n {
        Nav::Next(k) => app.step(k as isize),
        Nav::Prev(k) => app.step(-(k as isize)),
        Nav::Back => app.pop(),
        Nav::Ok => {
            let start = matches!(app.items.get(app.level().focus).map(|i| i.act), Some(Act::Start(_)));
            if let Some(t) = app.activate() {
                show_toast(vm, toast_until, t);
            }
            if start {
                *on_play = true;
                if !ctx.status.connected.load(Ordering::Relaxed) {
                    show_toast(vm, toast_until, "Piano not connected".into());
                }
                return true;
            }
        }
        Nav::Tap(..) => {}
    }
    false
}


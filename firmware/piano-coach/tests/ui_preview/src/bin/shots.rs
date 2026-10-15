//! README screenshots of the Tab5 UI, rendered on the PC with the device's
//! own Slint UI, fonts, engraver and globe from the real song library.
//!
//!   cargo +stable run --release --target x86_64-unknown-linux-gnu --bin shots -- <out dir>
//!
//! Library: $PIANO_MIDI_DIR or ~/w/piano/midi-piano/assets/midi.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use piano_core::library::{self, fmt_time};
use piano_core::search::Index;
use piano_core::song::{is_black, key_name, Hand, HandSel, Song, KEY_HI, KEY_LO};
use piano_core::sphere::{Sphere, PALETTE};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::Platform;
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedString, VecModel};

slint::include_modules!();

#[path = "../../../../src/ui/sheet.rs"]
mod sheet;

const W: u32 = 1280;
const H: u32 = 720;
// ui/player.slint Geometry, src/ui.rs constants.
const KEYS_W: f32 = 1248.0;
const ROLL_H: f32 = 300.0;
const NOW_LINE: f32 = ROLL_H - 22.0;
const LOOKAHEAD_MS: f32 = 2600.0;
// The globe panel (src/globe.rs).
const GW: usize = 760;
const GH: usize = 456;

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn frame(window: &MinimalSoftwareWindow) -> Vec<Rgb8Pixel> {
    window.request_redraw();
    let mut px = vec![Rgb8Pixel::default(); (W * H) as usize];
    window.draw_if_needed(|r| {
        r.render(&mut px, W as usize);
    });
    px
}

fn save(px: &[Rgb8Pixel], dir: &Path, name: &str) {
    let path = dir.join(format!("{name}.png"));
    let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), W, H);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = px.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("{}", path.display());
}

fn find(dir: &Path, needle: &str) -> Option<PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).collect();
    v.sort();
    for p in v {
        if p.is_dir() {
            if let Some(f) = find(&p, needle) {
                return Some(f);
            }
        } else if p.file_name()?.to_string_lossy().starts_with(needle) {
            return Some(p);
        }
    }
    None
}

fn load(root: &Path, needle: &str) -> (PathBuf, Arc<Song>) {
    let p = find(root, needle).unwrap_or_else(|| panic!("no song {needle} in {}", root.display()));
    let s = Song::parse(&std::fs::read(&p).unwrap()).unwrap();
    (p, Arc::new(s))
}

fn info(s: &Song) -> String {
    let right = s.notes.iter().filter(|n| n.hand == Hand::Right).count();
    format!(
        "{} · {:.0} bpm · {} notes ({} right, {} left) · hands from tracks",
        fmt_time(s.duration_ms),
        s.bpm,
        s.notes.len(),
        right,
        s.notes.len() - right
    )
}

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

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("usage: shots <out dir>"));
    std::fs::create_dir_all(&out).unwrap();
    let root = PathBuf::from(
        std::env::var("PIANO_MIDI_DIR")
            .unwrap_or_else(|_| format!("{}/w/piano/midi-piano/assets/midi", std::env::var("HOME").unwrap())),
    );

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(W, H));

    let app = ui.global::<AppVM>();
    app.set_connected(true);
    app.set_device("GZUT-MIDI · Bluetooth".into());
    app.set_version("piano-coach 0.2.0".into());
    app.set_board_name("M5Stack Tab5".into());
    app.set_has_backlight(true);
    app.set_has_power_latch(true);
    app.set_transport("Bluetooth".into());
    app.set_sd_status("27.9 GB free of 31.9 GB".into());

    // ---- Library: a real folder, with a few bookmarks and scores ----------
    let lib = ui.global::<LibraryVM>();
    let folder = find(&root, "卖报歌").unwrap().parent().unwrap().to_path_buf();
    let mut names: Vec<String> = std::fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort_by(|a, b| library::natural_cmp(a, b));
    let favs = ["卖报歌", "小星星", "洋娃娃之梦", "两只老虎"];
    let badges = [("卖报歌", "★★★"), ("小星星", "★★☆"), ("两只老虎", "★★★"), ("洋娃娃之梦", "★☆☆"), ("生日快乐", "★★☆")];
    let entries: Vec<LibEntry> = names
        .iter()
        .map(|n| {
            let e = library::entry(n, false);
            LibEntry {
                fav: favs.contains(&e.title.as_str()),
                badge: badges.iter().find(|b| b.0 == e.title).map(|b| b.1).unwrap_or("").into(),
                title: e.title.as_str().into(),
                subtitle: e.subtitle.as_str().into(),
                is_dir: false,
            }
        })
        .collect();
    let folder_title = library::entry(&folder.file_name().unwrap().to_string_lossy(), true).title;
    lib.set_crumbs(ModelRc::new(VecModel::from(vec![SharedString::from("Library"), folder_title.as_str().into()])));
    lib.set_path_text(format!("{} songs", entries.len()).into());
    lib.set_entries(ModelRc::new(VecModel::from(entries)));
    let (_, mbg) = load(&root, "卖报歌");
    lib.set_sel_title("卖报歌".into());
    lib.set_sel_subtitle("聂耳".into());
    lib.set_sel_info(info(&mbg).into());
    lib.set_sel_ready(true);
    lib.set_sel_fav(true);
    lib.set_sel_scores("Learn ★★★ 96% · Follow ★★☆ 84% · played 12×".into());
    app.set_page(0);
    save(&frame(&window), &out, "library");

    // ---- Search by pinyin initials --------------------------------------
    let index = Index::build(&root);
    let hits = index.search("xxx", 50);
    lib.set_searching(true);
    lib.set_query("xxx".into());
    lib.set_search_status(format!("{} matches", hits.len()).into());
    lib.set_results(ModelRc::new(VecModel::from(
        hits.iter()
            .map(|s| LibEntry {
                title: s.title.as_str().into(),
                subtitle: if s.artist.is_empty() { s.folder.clone() } else { format!("{} · {}", s.artist, s.folder) }.into(),
                is_dir: false,
                fav: s.title == "小星星",
                badge: if s.title == "小星星" { "★★☆".into() } else { "".into() },
            })
            .collect::<Vec<_>>(),
    )));
    let (_, xxx) = load(&root, "小星星 - ");
    lib.set_sel_title("小星星".into());
    lib.set_sel_subtitle("作者不详".into());
    lib.set_sel_info(info(&xxx).into());
    lib.set_sel_scores("Learn ★★☆ 88% · played 4×".into());
    save(&frame(&window), &out, "search");
    lib.set_searching(false);

    // ---- Song globe: every song a dot, coloured by category --------------
    let origin = Rc::new(Cell::new((0.0f32, 0.0f32)));
    {
        let o = origin.clone();
        lib.on_sphere_origin(move |x, y| o.set((x, y)));
    }
    let base = index.songs.first().map(|s| s.path.clone()).unwrap();
    let top = base
        .strip_prefix(&root)
        .unwrap()
        .components()
        .next()
        .map(|c| root.join(c.as_os_str()))
        .unwrap();
    let mut cats: Vec<String> = Vec::new();
    let mut song_cat: Vec<(u8, String)> = index
        .songs
        .iter()
        .map(|s| {
            let rel = s.path.strip_prefix(&top).unwrap_or(&s.path);
            let c = rel.components().next().map(|c| c.as_os_str().to_string_lossy().into_owned()).unwrap_or_default();
            let c = library::entry(&c, true).title;
            let i = cats.iter().position(|x| *x == c).unwrap_or_else(|| {
                cats.push(c);
                cats.len() - 1
            });
            (i as u8, s.title.clone())
        })
        .collect();
    song_cat.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| library::natural_cmp(&a.1, &b.1)));
    let mut sphere = Sphere::new(song_cat.iter().map(|c| c.0).collect());
    let pick = song_cat.iter().position(|c| c.1 == "卖报歌").unwrap();
    sphere.face(pick);
    for _ in 0..400 {
        sphere.step(16.0);
    }
    lib.set_legend(ModelRc::new(VecModel::from(
        cats.iter()
            .enumerate()
            .map(|(i, n)| {
                let c = PALETTE[i % PALETTE.len()];
                LegendItem { name: n.as_str().into(), tint: slint::Color::from_rgb_u8(c[0], c[1], c[2]) }
            })
            .collect::<Vec<_>>(),
    )));
    lib.set_view(3);
    lib.set_path_text("● 卖报歌 · 聂耳".into());
    lib.set_sel_title("卖报歌".into());
    lib.set_sel_subtitle("聂耳".into());
    lib.set_sel_info(info(&mbg).into());
    lib.set_sel_scores("Learn ★★★ 96% · Follow ★★☆ 84% · played 12×".into());
    let _ = frame(&window); // lay out: the panel reports its origin
    let mut px = frame(&window);
    let (ox, oy) = origin.get();
    let favs: Vec<bool> = song_cat.iter().map(|c| favs.contains(&c.1.as_str())).collect();
    let mut buf = vec![0u8; GW * GH * 3];
    sphere.render(&mut buf, GW, GH, &favs, Some(pick));
    for y in 0..GH {
        for x in 0..GW {
            let (dx, dy) = (ox as usize + x, oy as usize + y);
            if dx < W as usize && dy < H as usize {
                let s = (y * GW + x) * 3;
                px[dy * W as usize + dx] = Rgb8Pixel { r: buf[s], g: buf[s + 1], b: buf[s + 2] };
            }
        }
    }
    save(&px, &out, "globe");
    lib.set_view(0);

    // ---- Play page: Learn with falling notes and the keyboard -------------
    let p = ui.global::<PlayerVM>();
    let (_, ode) = load(&root, "欢乐颂 - ");
    let steps = ode.steps(HandSel::Right);
    let step = &steps[steps.len() / 3];
    let pos = step.ms as f32;
    let geo = key_geometry();
    let mut order: Vec<usize> = (0..geo.len()).filter(|&i| !is_black(geo[i].0)).collect();
    order.extend((0..geo.len()).filter(|&i| is_black(geo[i].0)));
    let sounding: Vec<u8> = ode
        .notes
        .iter()
        .filter(|n| n.hand == Hand::Left && n.start_ms as f32 <= pos && n.end_ms as f32 > pos)
        .map(|n| n.key)
        .collect();
    let keys: Vec<KeyView> = order
        .iter()
        .map(|&i| {
            let (k, x, w) = geo[i];
            let hint = step.keys.contains(&k);
            let state = (hint as i32) * 9 | (sounding.contains(&k) as i32);
            KeyView {
                x,
                w,
                black: is_black(k),
                state,
                label: if k % 12 == 0 { key_name(k).into() } else { SharedString::new() },
            }
        })
        .collect();
    p.set_keys(ModelRc::new(VecModel::from(keys)));
    let ppm = NOW_LINE / LOOKAHEAD_MS;
    let roll: Vec<RollNote> = ode
        .notes
        .iter()
        .filter(|n| (n.start_ms as f32) < pos + LOOKAHEAD_MS && (n.end_ms as f32) > pos - 400.0)
        .map(|n| {
            let (_, x, w) = geo[(n.key - KEY_LO) as usize];
            let y_bottom = NOW_LINE - (n.start_ms as f32 - pos) * ppm;
            let y_top = NOW_LINE - (n.end_ms as f32 - pos) * ppm;
            let mine = n.hand == Hand::Right;
            let kind = if mine && n.start_ms == step.ms && step.keys.contains(&n.key) {
                2
            } else if mine && (n.start_ms as f32) < pos - 1.0 {
                3
            } else if n.hand == Hand::Right {
                0
            } else {
                1
            };
            RollNote { x: x + 1.0, y: y_top, w: (w - 2.0).max(3.0), h: (y_bottom - y_top).max(4.0), kind }
        })
        .collect();
    p.set_roll(ModelRc::new(VecModel::from(roll)));
    p.set_now_line(NOW_LINE);
    p.set_loaded(true);
    p.set_title("欢乐颂".into());
    p.set_subtitle("贝多芬".into());
    p.set_mode(1);
    p.set_hands(1);
    p.set_state(2);
    let names: Vec<String> = step.keys.iter().map(|&k| key_name(k)).collect();
    p.set_prompt(format!("Play  {}", names.join(" · ")).into());
    p.set_progress(pos / ode.duration_ms as f32);
    p.set_pos_text(fmt_time(step.ms).into());
    p.set_dur_text(fmt_time(ode.duration_ms).into());
    p.set_tempo(80);
    p.set_accuracy("94%".into());
    p.set_steps(format!("{}/{}", steps.len() / 3, steps.len()).into());
    p.set_streak("12".into());
    app.set_page(1);
    save(&frame(&window), &out, "play");

    // ---- Sheet music: ink, Boomwhackers + letters, rainbow + solfège ------
    let mut sh = sheet::Sheet::new(&p);
    p.set_sheet(true);
    for (name, needle, colors, names_mode, frac, hands) in [
        ("sheet", "卖报歌", 0, 0, 0.25f32, HandSel::Both),
        ("sheet-colours", "小星星 - ", 1, 1, 0.0, HandSel::Both),
        ("sheet-rainbow", "欢乐颂 - ", 2, 2, 0.12, HandSel::Right),
    ] {
        let (_, song) = load(&root, needle);
        let st = song.steps(hands);
        let s = &st[((st.len() as f32 * frac) as usize).min(st.len() - 1)];
        p.set_sheet_colors(colors);
        p.set_sheet_names(names_mode);
        let staves = if hands == HandSel::Right { 1 } else { 3 };
        sh.update(&p, Some(&song), s.ms, Some(s.ms), staves, (W as f32, H as f32 - 52.0));
        let t = needle.trim_end_matches(" - ");
        p.set_title(t.into());
        let keys: Vec<String> = s.keys.iter().map(|&k| key_name(k)).collect();
        p.set_prompt(format!("Play  {}", keys.join(" · ")).into());
        p.set_mode(1);
        save(&frame(&window), &out, name);
    }
    p.set_sheet(false);

    // ---- Settings ---------------------------------------------------------
    let s = ui.global::<SettingsVM>();
    s.set_library_root("/sdcard/midi".into());
    s.set_panel("volume 5".into());
    s.set_transport(1);
    s.set_sound(2);
    p.set_sheet_colors(1);
    p.set_sheet_names(1);
    app.set_page(2);
    save(&frame(&window), &out, "settings");
}

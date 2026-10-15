//! Render each Piano Coach page on the host and save PNGs.
//!
//!   cargo +stable run --release --target x86_64-unknown-linux-gnu
//!
//! (The explicit --target overrides the ESP32 target from ../../.cargo.)
//!
//! Output goes to $UI_PREVIEW_DIR (default /tmp/piano-ui-preview).

use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowEvent};
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, SharedString, VecModel};

slint::include_modules!();

/// (board, physical width, height, UI scale) — as in src/board/<board>.
const BOARDS: &[(&str, u32, u32, f32)] = &[("tab5", 1280, 720, 1.0), ("korvo", 800, 480, 0.625)];

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

thread_local! {
    static SIZE: std::cell::Cell<(u32, u32, &'static str)> = const { std::cell::Cell::new((1280, 720, "tab5")) };
}

fn render(window: &MinimalSoftwareWindow, name: &str) {
    let (w, h, board) = SIZE.with(|s| s.get());
    let name = format!("{board}-{name}");
    let name = name.as_str();
    window.request_redraw();
    let mut px = vec![Rgb8Pixel::default(); (w * h) as usize];
    window.draw_if_needed(|r| {
        r.render(&mut px, w as usize);
    });
    let (W, H) = (w, h);
    let dir = std::env::var("UI_PREVIEW_DIR").unwrap_or_else(|_| "/tmp/piano-ui-preview".into());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/{name}.png");
    let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), W, H);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = px.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("{path}");
}

fn is_black(k: u8) -> bool {
    matches!(k % 12, 1 | 3 | 6 | 8 | 10)
}

fn entries(n: usize, dir: bool) -> ModelRc<LibEntry> {
    let titles = ["儿时回忆", "红色歌谣", "流行空间", "洋娃娃之梦", "卖报歌", "Yesterday Once More", "虫儿飞（加长版）"];
    let v: Vec<LibEntry> = (0..n)
        .map(|i| LibEntry {
            title: titles[i % titles.len()].into(),
            subtitle: if dir { format!("{} songs", 10 + i).into() } else { "奥斯丁 · 儿时回忆".into() },
            is_dir: dir,
            fav: !dir && i % 3 == 0,
            badge: if !dir && i % 2 == 0 { "★★☆".into() } else { "".into() },
        })
        .collect();
    ModelRc::new(VecModel::from(v))
}

fn main() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    for &(board, w, h, scale) in BOARDS {
        SIZE.with(|s| s.set((w, h, board)));
        let ui = AppWindow::new().unwrap();
        window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: scale });
        ui.show().unwrap();
        window.set_size(PhysicalSize::new(w, h));
        pages(&window, &ui);
        // Tap each top-bar tab like the touch path does (logical positions).
        for (x, want) in [(560.0, 1), (430.0, 0), (720.0, 2), (560.0, 1)] {
            ui.global::<AppVM>().set_confirm_shutdown(false);
            let p = slint::LogicalPosition::new(x, 36.0);
            window.dispatch_event(WindowEvent::PointerPressed { position: p, button: slint::platform::PointerEventButton::Left });
            window.dispatch_event(WindowEvent::PointerReleased { position: p, button: slint::platform::PointerEventButton::Left });
            window.dispatch_event(WindowEvent::PointerExited);
            let got = ui.global::<AppVM>().get_page();
            println!("{board}: tap ({x}, 36) -> page {got} (want {want})");
        }
        ui.hide().unwrap();
    }
}

fn pages(window: &MinimalSoftwareWindow, ui: &AppWindow) {
    let window = window;

    let app = ui.global::<AppVM>();
    app.set_connected(true);
    app.set_device("GZUT-MIDI · Bluetooth".into());
    app.set_version("piano-tab5 preview".into());
    app.set_sd_status("27.9 GB free of 31.9 GB".into());

    let lib = ui.global::<LibraryVM>();
    lib.set_crumbs(ModelRc::new(VecModel::from(vec![SharedString::from("Library"), "儿时回忆".into()])));
    lib.set_path_text("41 items".into());
    lib.set_entries(entries(12, false));
    lib.set_sel_title("洋娃娃之梦".into());
    lib.set_sel_subtitle("奥斯丁".into());
    lib.set_sel_info("1:12 · 120 bpm · 1046 notes (434 right, 612 left) · hands from tracks".into());
    lib.set_sel_ready(true);
    lib.set_sel_fav(true);
    lib.set_sel_scores("Learn ★★☆ 92% · Follow ★☆☆ 71% · played 5×".into());
    app.set_page(0);
    render(window, "library");

    // Song globe: the panel is painted by the firmware; here we draw the
    // globe into an image only to check where the panel ends up.
    lib.on_sphere_origin(|x, y| println!("sphere-origin reported: ({x}, {y})"));
    lib.set_view(3);
    lib.set_path_text("● 卖报歌 · 聂耳".into());
    let names = ["儿时回忆", "红色歌谣", "流行空间", "经典乐章", "影视剧场"];
    let legend: Vec<LegendItem> = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let c = piano_core::sphere::PALETTE[i];
            LegendItem { name: (*n).into(), tint: slint::Color::from_rgb_u8(c[0], c[1], c[2]) }
        })
        .collect();
    lib.set_legend(ModelRc::new(VecModel::from(legend)));
    println!("-- globe view shown");
    render(window, "globe");
    println!("-- search opened");
    lib.set_searching(true);
    render(window, "globe-search");
    println!("-- search closed");
    lib.set_searching(false);
    slint::platform::update_timers_and_animations();
    render(window, "globe-after-search");
    lib.set_view(1);
    lib.set_path_text("12 bookmarked songs".into());
    render(window, "bookmarks");
    lib.set_view(0);

    lib.set_searching(true);
    lib.set_query("xyz".into());
    lib.set_search_status("27 matches".into());
    lib.set_results(entries(27, false));
    render(window, "search");
    lib.set_searching(false);

    // Play page with a keyboard and a few falling notes.
    let p = ui.global::<PlayerVM>();
    let whites = (21..=108u8).filter(|&k| !is_black(k)).count() as f32;
    let ww = 1248.0 / whites;
    let mut keys = Vec::new();
    let mut blacks = Vec::new();
    let mut wi = 0.0f32;
    for k in 21..=108u8 {
        let state = if [60, 64, 67].contains(&k) { 9 } else if k == 48 { 2 } else { 0 };
        let label: SharedString = if k % 12 == 0 { format!("C{}", k / 12 - 1).into() } else { "".into() };
        if is_black(k) {
            blacks.push(KeyView { x: wi * ww - ww * 0.3, w: ww * 0.6, black: true, state, label });
        } else {
            keys.push(KeyView { x: wi * ww, w: ww, black: false, state, label });
            wi += 1.0;
        }
    }
    keys.extend(blacks);
    p.set_keys(ModelRc::new(VecModel::from(keys)));
    p.set_roll(ModelRc::new(VecModel::from(vec![
        RollNote { x: 552.0, y: 180.0, w: 22.0, h: 96.0, kind: 2 },
        RollNote { x: 600.0, y: 120.0, w: 22.0, h: 60.0, kind: 0 },
        RollNote { x: 360.0, y: 40.0, w: 22.0, h: 200.0, kind: 1 },
    ])));
    p.set_now_line(278.0);
    p.set_loaded(true);
    p.set_title("洋娃娃之梦".into());
    p.set_subtitle("奥斯丁".into());
    p.set_state(2);
    p.set_prompt("Play  C4 · E4 · G4".into());
    p.set_progress(0.4);
    p.set_loop_a(0.3);
    p.set_loop_b(0.55);
    p.set_looping(true);
    p.set_loop_text("#2".into());
    p.set_accuracy("92%".into());
    p.set_steps("41/180".into());
    p.set_streak("7".into());
    app.set_page(1);
    render(window, "play");

    let s = ui.global::<SettingsVM>();
    s.set_library_root("/sdcard/midi".into());
    s.set_panel("volume 5".into());
    s.set_transport(1);
    s.set_sound(2);
    app.set_page(2);
    render(window, "settings");

    app.set_confirm_shutdown(true);
    render(window, "shutdown-dialog");
}

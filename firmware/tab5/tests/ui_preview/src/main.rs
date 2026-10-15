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

const W: u32 = 1280;
const H: u32 = 720;

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn render(window: &MinimalSoftwareWindow, name: &str) {
    window.request_redraw();
    let mut px = vec![Rgb8Pixel::default(); (W * H) as usize];
    window.draw_if_needed(|r| {
        r.render(&mut px, W as usize);
    });
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
        })
        .collect();
    ModelRc::new(VecModel::from(v))
}

fn main() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = AppWindow::new().unwrap();
    ui.show().unwrap();
    window.set_size(PhysicalSize::new(W, H));
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 1.0 });

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
    app.set_page(0);
    render(&window, "library");

    lib.set_searching(true);
    lib.set_query("xyz".into());
    lib.set_search_status("27 matches".into());
    lib.set_results(entries(27, false));
    render(&window, "search");
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
    render(&window, "play");

    let s = ui.global::<SettingsVM>();
    s.set_library_root("/sdcard/midi".into());
    s.set_panel("volume 5".into());
    s.set_transport(1);
    app.set_page(2);
    render(&window, "settings");

    app.set_confirm_shutdown(true);
    render(&window, "shutdown-dialog");
}

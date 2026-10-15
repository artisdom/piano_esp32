//! Render the mini UI's screens on the host and save PNGs.
//!
//!   cargo +stable run --release --target x86_64-unknown-linux-gnu
//!
//! Output: $UI_PREVIEW_DIR (default /tmp/piano-ui-preview), mini-<shape>-<screen>.png.
//! The round screens are masked to the panel's circle.

use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::Platform;
use slint::{ComponentHandle, ModelRc, PhysicalSize, Rgb8Pixel, VecModel};

slint::include_modules!();

struct Headless(Rc<MinimalSoftwareWindow>);
impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

fn save(window: &MinimalSoftwareWindow, w: u32, h: u32, round: bool, name: &str) {
    window.request_redraw();
    let mut px = vec![Rgb8Pixel::default(); (w * h) as usize];
    window.draw_if_needed(|r| {
        r.render(&mut px, w as usize);
    });
    if round {
        let c = w as f32 / 2.0;
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
                if dx * dx + dy * dy > c * c {
                    px[(y * w + x) as usize] = Rgb8Pixel { r: 255, g: 255, b: 255 };
                }
            }
        }
    }
    let dir = std::env::var("UI_PREVIEW_DIR").unwrap_or_else(|_| "/tmp/piano-ui-preview".into());
    std::fs::create_dir_all(&dir).unwrap();
    let path = format!("{dir}/mini-{name}.png");
    let mut enc = png::Encoder::new(std::fs::File::create(&path).unwrap(), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let data: Vec<u8> = px.iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("{path}");
}

fn row(icon: &str, title: &str, sub: &str, value: &str) -> Row {
    Row {
        icon: icon.into(),
        title: title.into(),
        sub: sub.into(),
        value: value.into(),
    }
}

fn main() {
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
    let ui = MiniApp::new().unwrap();
    ui.show().unwrap();
    for (shape, w, h, round) in [("bar", 320u32, 170u32, false), ("round", 240, 240, true)] {
        window.set_size(PhysicalSize::new(w, h));
        let vm = ui.global::<MiniVM>();
        vm.set_round(round);
        vm.set_connected(true);
        vm.set_toast("".into());

        vm.set_page(0);
        vm.set_header("儿时回忆".into());
        vm.set_footer(if round { "2 / 42".into() } else { "2 / 42  ·  tap ›  2× ‹  hold OK".into() });
        vm.set_rows(ModelRc::new(VecModel::from(vec![
            row("back", "Back", "", ""),
            row("note", "卖报歌", "聂耳", "★★☆"),
            row("note", "虫儿飞（加长版）", "郑尹健", ""),
        ])));
        save(&window, w, h, round, &format!("{shape}-songs"));

        vm.set_header("卖报歌".into());
        vm.set_footer("1 / 6".into());
        vm.set_rows(ModelRc::new(VecModel::from(vec![
            row("back", "Back", "", ""),
            row("learn", "Learn", "best 86%", "★★☆"),
            row("follow", "Follow", "", ""),
        ])));
        save(&window, w, h, round, &format!("{shape}-song"));

        vm.set_header("Settings".into());
        vm.set_rows(ModelRc::new(VecModel::from(vec![
            row("check", "Easy mode", "any key advances", "Off"),
            row("bulb", "Hint off on hit", "", "On"),
            row("note", "Accompaniment", "", "On"),
        ])));
        vm.set_toast("Piano connected · GZUT-MIDI".into());
        save(&window, w, h, round, &format!("{shape}-settings"));
        vm.set_toast("".into());

        vm.set_page(1);
        vm.set_song("虫儿飞（加长版）".into());
        vm.set_mode("Learn · Both · 100%".into());
        vm.set_state(2);
        vm.set_progress(0.37);
        vm.set_time("0:41 / 1:52".into());
        vm.set_cue("C4 E4 G4".into());
        vm.set_cue_color(slint::Color::from_rgb_u8(0xff, 0x3b, 0x4f));
        vm.set_hint("Play the lit keys".into());
        vm.set_stats("✓ 42   ✕ 3".into());
        save(&window, w, h, round, &format!("{shape}-play"));

        vm.set_state(4);
        vm.set_progress(1.0);
        vm.set_stars(2);
        vm.set_result("86%  ·  New best!".into());
        save(&window, w, h, round, &format!("{shape}-done"));
    }
}

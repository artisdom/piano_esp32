//! Build script: board sanity checks, ESP-IDF environment, Slint UI.
//!
//! Boards are declared by `boards/<board>/board.env` (FEATURE=, MCU=, …);
//! adding a board never means editing this file.

use std::collections::BTreeMap;
use std::path::Path;

/// One boards/<board>/board.env.
struct Board {
    dir: String,
    feature: String,
    mcu: String,
    /// `full` (1280-wide design, framebuffer), `mini` (small SPI panels) or
    /// `headless` (no display: keys, tones and a Wi-Fi web page).
    ui: String,
    /// Hardware the application may use: `usb` (USB-MIDI host), `sd`
    /// (microSD library), `speaker`.
    has: Vec<String>,
}

const HAS: [&str; 3] = ["usb", "sd", "speaker"];

fn boards() -> Vec<Board> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("boards").expect("boards/ directory").flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let env = e.path().join("board.env");
        println!("cargo:rerun-if-changed={}", env.display());
        let Ok(text) = std::fs::read_to_string(&env) else { continue };
        let kv: BTreeMap<&str, &str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim().trim_matches('"')))
            .collect();
        let dir = e.file_name().to_string_lossy().into_owned();
        let get = |k: &str| kv.get(k).unwrap_or_else(|| panic!("{} lacks {k}=", env.display())).to_string();
        let has: Vec<String> = kv.get("HAS").unwrap_or(&"").split_whitespace().map(str::to_string).collect();
        if let Some(bad) = has.iter().find(|h| !HAS.contains(&h.as_str())) {
            panic!("{}: unknown HAS entry `{bad}` (known: {})", env.display(), HAS.join(" "));
        }
        let ui = kv.get("UI").unwrap_or(&"full").to_string();
        if !["full", "mini", "headless"].contains(&ui.as_str()) {
            panic!("{}: UI must be `full`, `mini` or `headless`, not `{ui}`", env.display());
        }
        out.push(Board {
            feature: get("FEATURE"),
            mcu: get("MCU"),
            dir,
            ui,
            has,
        });
    }
    out.sort_by(|a, b| a.dir.cmp(&b.dir));
    out
}

fn main() {
    let all = boards();
    let selected: Vec<_> = all
        .iter()
        .filter(|b| {
            let var = format!("CARGO_FEATURE_{}", b.feature.to_uppercase().replace('-', "_"));
            println!("cargo:rerun-if-env-changed={var}");
            std::env::var_os(var).is_some()
        })
        .collect();
    let b = match selected.as_slice() {
        [one] => *one,
        [] => panic!(
            "select one board feature ({}) — or use tools/board.sh <board>",
            all.iter().map(|b| b.feature.as_str()).collect::<Vec<_>>().join(", ")
        ),
        many => panic!(
            "board features are exclusive, but {} are enabled (a plain --features keeps the \
             default board: pass --no-default-features, or use tools/board.sh)",
            many.iter().map(|b| b.feature.as_str()).collect::<Vec<_>>().join(" and ")
        ),
    };
    let (board, feature, want_mcu) = (&b.dir, &b.feature, &b.mcu);
    if !Path::new(&format!("src/board/{board}/mod.rs")).exists() {
        panic!("board `{board}` has a profile but no src/board/{board}/mod.rs");
    }
    // The Rust side picks the board from the feature, ESP-IDF picks its target
    // from $MCU: a mismatch otherwise shows up as baffling bindgen errors.
    println!("cargo:rerun-if-env-changed=MCU");
    if let Ok(mcu) = std::env::var("MCU") {
        if mcu != *want_mcu {
            panic!("`{feature}` needs MCU={want_mcu}, but MCU={mcu}: run `tools/board.sh {board} …`");
        }
    }

    embuild::espidf::sysenv::output();

    // What the application compiles in for this board: `ui_full` / `ui_mini`
    // and one `has_<x>` per HAS entry.
    println!("cargo::rustc-check-cfg=cfg(ui_full)");
    println!("cargo::rustc-check-cfg=cfg(ui_mini)");
    println!("cargo::rustc-check-cfg=cfg(ui_headless)");
    // esp-idf-sys emits one cfg per sdkconfig option; this one is tested
    // in threads.rs / audio.rs and is absent on boards without it.
    println!("cargo::rustc-check-cfg=cfg(esp_idf_spiram_allow_stack_external_memory)");
    for h in HAS {
        println!("cargo::rustc-check-cfg=cfg(has_{h})");
    }
    if b.ui == "full" {
        // The full UI has a speaker page, a microSD library and a USB/BLE switch.
        for need in ["usb", "sd", "speaker"] {
            if !b.has.iter().any(|h| h == need) {
                panic!("boards/{board}: UI=full needs HAS to include `{need}` (use UI=mini)");
            }
        }
    }
    println!("cargo:rustc-cfg=ui_{}", b.ui);
    for h in &b.has {
        println!("cargo:rustc-cfg=has_{h}");
    }

    // Glyphs are pre-rendered for the software renderer (no fonts on the
    // device). SDF stores one distance field per glyph, scaled at draw time.
    // The mini UI imports `@board/glyphs.slint`: the characters of that
    // board's song pack (written by tools/songs.sh).
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
        .with_sdf_fonts(true)
        .with_library_paths(
            [("board".to_string(), std::fs::canonicalize(format!("boards/{board}")).unwrap())].into(),
        );
    // No display, no Slint UI: keys + a web page, served gzipped.
    if b.ui == "headless" {
        gzip_page("web/index.html");
        return;
    }
    let main = if b.ui == "mini" { "ui/mini/app.slint" } else { "ui/app.slint" };
    slint_build::compile_with_config(main, config).expect("slint build failed");
}

/// `web/index.html` → `$OUT_DIR/index.html.gz` (gzip around raw deflate).
fn gzip_page(src: &str) {
    println!("cargo:rerun-if-changed={src}");
    let raw = std::fs::read(src).expect("web page");
    let mut crc = !0u32;
    for &b in &raw {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    let mut gz = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 2, 255];
    gz.extend(miniz_oxide::deflate::compress_to_vec(&raw, 9));
    gz.extend((!crc).to_le_bytes());
    gz.extend((raw.len() as u32).to_le_bytes());
    let out = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("index.html.gz");
    std::fs::write(out, gz).unwrap();
}

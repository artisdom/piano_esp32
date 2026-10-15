//! Build script: board sanity checks, ESP-IDF environment, Slint UI.
//!
//! Boards are declared by `boards/<board>/board.env` (FEATURE=, MCU=, …);
//! adding a board never means editing this file.

use std::collections::BTreeMap;
use std::path::Path;

/// (board dir, FEATURE, MCU) for every boards/*/board.env.
fn boards() -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for e in std::fs::read_dir("boards").expect("boards/ directory").flatten() {
        let env = e.path().join("board.env");
        println!("cargo:rerun-if-changed={}", env.display());
        let Ok(text) = std::fs::read_to_string(&env) else { continue };
        let kv: BTreeMap<&str, &str> = text
            .lines()
            .filter(|l| !l.trim_start().starts_with('#'))
            .filter_map(|l| l.split_once('='))
            .map(|(k, v)| (k.trim(), v.trim()))
            .collect();
        let name = e.file_name().to_string_lossy().into_owned();
        let feature = kv.get("FEATURE").unwrap_or_else(|| panic!("{} lacks FEATURE=", env.display()));
        let mcu = kv.get("MCU").unwrap_or_else(|| panic!("{} lacks MCU=", env.display()));
        out.push((name, feature.to_string(), mcu.to_string()));
    }
    out.sort();
    out
}

fn main() {
    let all = boards();
    let selected: Vec<_> = all
        .iter()
        .filter(|(_, f, _)| {
            let var = format!("CARGO_FEATURE_{}", f.to_uppercase().replace('-', "_"));
            println!("cargo:rerun-if-env-changed={var}");
            std::env::var_os(var).is_some()
        })
        .collect();
    let (board, feature, want_mcu) = match selected.as_slice() {
        [one] => *one,
        [] => panic!(
            "select one board feature ({}) — or use tools/board.sh <board>",
            all.iter().map(|b| b.1.as_str()).collect::<Vec<_>>().join(", ")
        ),
        many => panic!(
            "board features are exclusive, but {} are enabled (a plain --features keeps the \
             default board: pass --no-default-features, or use tools/board.sh)",
            many.iter().map(|b| b.1.as_str()).collect::<Vec<_>>().join(" and ")
        ),
    };
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

    // Glyphs are pre-rendered for the software renderer (no fonts on the
    // device). SDF stores one distance field per glyph, scaled at draw time.
    let config = slint_build::CompilerConfiguration::new()
        .embed_resources(slint_build::EmbedResourcesKind::EmbedForSoftwareRenderer)
        .with_sdf_fonts(true);
    slint_build::compile_with_config("ui/app.slint", config).expect("slint build failed");
}

//! Build a song pack (see `piano_core::songpack`) from a folder of MIDI
//! files, for boards that keep their songs in flash.
//!
//! ```text
//! cargo +stable run --release --bin songpack -- <library> -o songs.bin
//!     [--max-bytes 1400K] [--max-notes 2500] [--prefer <sub/path>]...
//!     [--glyphs glyphs.slint]
//! ```
//!
//! Every file is parsed with the firmware's own parser; files that fail or
//! have no notes are skipped. With a budget, songs under the `--prefer`
//! paths (in the order given) go in first, then the rest in library order,
//! until the next song would not fit. `--max-notes` keeps songs whose
//! engine data fits a small RAM. `--glyphs` writes a Slint global listing
//! every character of the packed names plus ASCII, Latin-1 and the UI's
//! symbols, so slint-build embeds exactly those glyphs (the font,
//! ui/fonts/NotoSansSC-Piano.otf, covers the bundled library).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::exit;

use piano_core::library::{is_midi, natural_cmp};
use piano_core::song::Song;
use piano_core::songpack::{self, Input, Pack};

fn usage() -> ! {
    eprintln!(
        "usage: songpack <library dir> -o <pack.bin> [--max-bytes N[K|M]] [--max-notes N] \
         [--prefer <sub/path>]... [--glyphs <file.slint>]"
    );
    exit(2)
}

fn parse_size(s: &str) -> Option<usize> {
    let s = s.trim();
    let (num, mul) = match s.chars().last()? {
        'K' | 'k' => (&s[..s.len() - 1], 1024),
        'M' | 'm' => (&s[..s.len() - 1], 1024 * 1024),
        _ => (s, 1),
    };
    num.parse::<f64>().ok().map(|n| (n * mul as f64) as usize)
}

fn walk(dir: &Path, rel: &str, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let r = if rel.is_empty() { name.clone() } else { format!("{rel}/{name}") };
        let p = e.path();
        if p.is_dir() {
            walk(&p, &r, out);
        } else if is_midi(&name) {
            out.push((r, p));
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let (mut root, mut out, mut max_bytes, mut max_notes, mut prefer, mut chars) =
        (None, None, None, None, Vec::new(), None);
    while let Some(a) = args.next() {
        match a.as_str() {
            "-o" | "--out" => out = args.next(),
            "--max-bytes" => max_bytes = Some(args.next().and_then(|s| parse_size(&s)).unwrap_or_else(|| usage())),
            "--max-notes" => max_notes = Some(args.next().and_then(|s| s.parse::<usize>().ok()).unwrap_or_else(|| usage())),
            "--prefer" => prefer.push(args.next().unwrap_or_else(|| usage())),
            "--glyphs" => chars = args.next(),
            "-h" | "--help" => usage(),
            _ if root.is_none() => root = Some(PathBuf::from(a)),
            _ => usage(),
        }
    }
    let (Some(mut root), Some(out)) = (root, out) else { usage() };

    // A library that is one folder (e.g. "JustPiano2-20160326/") is packed
    // from inside it, so the device's top level shows its categories.
    loop {
        let entries: Vec<_> = std::fs::read_dir(&root)
            .unwrap_or_else(|e| {
                eprintln!("songpack: {}: {e}", root.display());
                exit(1)
            })
            .flatten()
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .collect();
        match entries.as_slice() {
            [only] if only.path().is_dir() => root = only.path(),
            _ => break,
        }
    }

    let mut files = Vec::new();
    walk(&root, "", &mut files);
    let rank = |p: &str| prefer.iter().position(|q| p.starts_with(q.trim_matches('/'))).unwrap_or(prefer.len());
    files.sort_by(|a, b| rank(&a.0).cmp(&rank(&b.0)).then_with(|| natural_cmp(&a.0, &b.0)));

    let (mut picked, mut total, mut skipped_bad, mut skipped_notes, mut skipped_budget) =
        (Vec::new(), 0usize, 0, 0, 0);
    for (rel, path) in &files {
        let Ok(midi) = std::fs::read(path) else {
            skipped_bad += 1;
            continue;
        };
        let song = match Song::parse(&midi) {
            Ok(s) if !s.notes.is_empty() => s,
            _ => {
                skipped_bad += 1;
                continue;
            }
        };
        if max_notes.is_some_and(|m| song.notes.len() > m) {
            skipped_notes += 1;
            continue;
        }
        // Index entry + name + compressed data.
        let cost = 28 + rel.len() + miniz_oxide::deflate::compress_to_vec_zlib(&midi, 9).len();
        if max_bytes.is_some_and(|m| total + cost + 32 > m) {
            skipped_budget += 1;
            continue;
        }
        total += cost;
        picked.push(Input {
            path: rel.clone(),
            midi,
            duration_ms: song.duration_ms,
        });
    }

    let pack = songpack::build(&picked);
    let p = Pack::new(&pack).expect("freshly built pack must validate");
    std::fs::write(&out, &pack).unwrap_or_else(|e| {
        eprintln!("songpack: {out}: {e}");
        exit(1)
    });
    eprintln!(
        "songpack: {} songs, {} folders, {:.2} MB → {out} (skipped: {skipped_bad} unreadable/empty, \
         {skipped_notes} over the note limit, {skipped_budget} over the size budget)",
        picked.len(),
        p.len() - picked.len(),
        pack.len() as f64 / (1024.0 * 1024.0),
    );

    if let Some(path) = chars {
        let mut set: BTreeSet<char> = BTreeSet::new();
        set.extend((0x20u32..0x7f).chain(0xa0..0x100).chain(0x2010..0x2040).filter_map(char::from_u32));
        set.extend("♪♫♩♬▶■●○◆★☆✓✔✕×÷←↑→↓·　、。《》「」『』【】（），：；！？".chars());
        let names = (0..p.len() as u32).flat_map(|i| p.node(i).name.chars()).collect::<BTreeSet<char>>();
        let n_names = names.len();
        set.extend(names);
        let lit: String = set
            .iter()
            .map(|&c| match c {
                '"' => "\\\"".to_string(),
                '\\' => "\\\\".to_string(),
                c => c.to_string(),
            })
            .collect();
        let s = format!(
            "// GENERATED by `songpack --glyphs` (tools/songs.sh) — do not edit.\n\
             // Every character the mini UI can draw: its own text plus the names in\n\
             // this board's song pack ({} songs, {n_names} distinct characters).\n\
             export global Glyphs {{\n    out property <string> all: \"{lit}\";\n}}\n",
            picked.len()
        );
        std::fs::write(&path, s).unwrap_or_else(|e| {
            eprintln!("songpack: {path}: {e}");
            exit(1)
        });
    }
}

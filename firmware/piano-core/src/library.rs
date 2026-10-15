//! Naming and ordering of library entries read from the SD card.
//!
//! The bundled library (JustPiano) names folders `分类（41）` — a category
//! with its song count in full-width parentheses — and songs
//! `标题 - 作者.mid`. Both are split into a title and a subtitle for the UI.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// File-system name (used to build paths).
    pub name: String,
    pub is_dir: bool,
    pub title: String,
    /// Folder: "41 songs"; song: the composer/artist ("" if none).
    pub subtitle: String,
    /// Folder: song count parsed from the name, if present.
    pub count: Option<u32>,
}

pub fn is_midi(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.ends_with(".mid") || n.ends_with(".midi") || n.ends_with(".kar") || n.ends_with(".rmi")
}

/// Split `"name（41）"` / `"name(41)"` into ("name", Some(41)).
fn split_count(name: &str) -> (String, Option<u32>) {
    let trimmed = name.trim_end();
    for (open, close) in [('（', '）'), ('(', ')')] {
        if let Some(inner) = trimmed.strip_suffix(close) {
            if let Some(i) = inner.rfind(open) {
                if let Ok(n) = inner[i + open.len_utf8()..].trim().parse::<u32>() {
                    return (inner[..i].trim_end().to_string(), Some(n));
                }
            }
        }
    }
    (trimmed.to_string(), None)
}

fn strip_ext(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

pub fn entry(name: &str, is_dir: bool) -> Entry {
    if is_dir {
        let (title, count) = split_count(name);
        // Drop the JustPiano "小项A：" (sub-section A:) prefix.
        let title = match title.find('：') {
            Some(i) if title.starts_with("小项") => title[i + '：'.len_utf8()..].to_string(),
            _ => title,
        };
        Entry {
            name: name.to_string(),
            is_dir,
            subtitle: count.map(|n| format!("{n} songs")).unwrap_or_default(),
            title,
            count,
        }
    } else {
        let stem = strip_ext(name);
        let (title, subtitle) = match stem.rfind(" - ") {
            Some(i) => (stem[..i].trim().to_string(), stem[i + 3..].trim().to_string()),
            None => (stem.trim().to_string(), String::new()),
        };
        Entry {
            name: name.to_string(),
            is_dir,
            title,
            subtitle,
            count: None,
        }
    }
}

/// Compare with digit runs as numbers ("a2" < "a10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n1 = String::new();
                while let Some(c) = x.peek().copied().filter(char::is_ascii_digit) {
                    n1.push(c);
                    x.next();
                }
                let mut n2 = String::new();
                while let Some(d) = y.peek().copied().filter(char::is_ascii_digit) {
                    n2.push(d);
                    y.next();
                }
                let (t1, t2) = (n1.trim_start_matches('0'), n2.trim_start_matches('0'));
                let o = t1.len().cmp(&t2.len()).then_with(|| t1.cmp(t2));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// Folders first, then songs, each in natural order. Hidden files and
/// non-MIDI files are dropped.
pub fn sort_entries(mut v: Vec<Entry>) -> Vec<Entry> {
    v.retain(|e| !e.name.starts_with('.') && (e.is_dir || is_midi(&e.name)));
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural_cmp(&a.name, &b.name)));
    v
}

/// "3:07" for a duration in ms.
pub fn fmt_time(ms: u32) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

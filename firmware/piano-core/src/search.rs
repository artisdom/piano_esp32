//! Song search over the whole SD-card library, typed with Latin letters.
//!
//! Titles are mostly Chinese, so every title and artist gets two Latin keys
//! from `pinyin_table`: full toneless pinyin ("xiaoyanzi") and initials
//! ("xyz"). Latin words contribute themselves and their first letters
//! ("Yesterday Once More" → "yesterdayoncemore" / "yom"). A query matches
//! if it is a substring of any key; spaces and punctuation are ignored.

use std::path::{Path, PathBuf};

use crate::library;
use crate::pinyin_table::PINYIN;

pub fn pinyin(c: char) -> Option<&'static str> {
    PINYIN.binary_search_by_key(&c, |e| e.0).ok().map(|i| PINYIN[i].1)
}

/// (full, initials) search keys for a display string.
pub fn keys(s: &str) -> (String, String) {
    let (mut full, mut init) = (String::new(), String::new());
    let mut in_word = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            let l = c.to_ascii_lowercase();
            full.push(l);
            if !in_word {
                init.push(l);
            }
            in_word = true;
        } else if let Some(py) = pinyin(c) {
            full.push_str(py);
            init.push_str(&py[..1]);
            in_word = false;
        } else {
            in_word = false;
        }
    }
    (full, init)
}

/// Lowercase letters and digits only.
pub fn normalize(q: &str) -> String {
    q.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[derive(Debug, Clone)]
pub struct SongRef {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    /// Display name of the containing folder.
    pub folder: String,
    title_full: String,
    title_init: String,
    artist_full: String,
    artist_init: String,
}

impl SongRef {
    pub fn new(path: PathBuf, folder: String) -> SongRef {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let e = library::entry(&name, false);
        let (title_full, title_init) = keys(&e.title);
        let (artist_full, artist_init) = keys(&e.subtitle);
        SongRef {
            path,
            title: e.title,
            artist: e.subtitle,
            folder,
            title_full,
            title_init,
            artist_full,
            artist_init,
        }
    }

    /// Lower is better; None = no match.
    fn rank(&self, q: &str) -> Option<u32> {
        if self.title_init.starts_with(q) || self.title_full.starts_with(q) {
            Some(0)
        } else if self.title_init.contains(q) || self.title_full.contains(q) {
            Some(1)
        } else if self.artist_init.starts_with(q) || self.artist_full.contains(q) {
            Some(2)
        } else {
            None
        }
    }
}

#[derive(Default)]
pub struct Index {
    pub songs: Vec<SongRef>,
}

impl Index {
    /// Walk `root` recursively (hidden entries skipped).
    pub fn build(root: &Path) -> Index {
        let mut songs = Vec::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            let folder = dir
                .file_name()
                .map(|n| library::entry(&n.to_string_lossy(), true).title)
                .unwrap_or_default();
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = e
                    .file_type()
                    .map(|t| t.is_dir())
                    .or_else(|_| e.metadata().map(|m| m.is_dir()))
                    .unwrap_or(false);
                if is_dir {
                    stack.push(e.path());
                } else if library::is_midi(&name) {
                    songs.push(SongRef::new(e.path(), folder.clone()));
                }
            }
        }
        songs.sort_by(|a, b| library::natural_cmp(&a.title, &b.title));
        Index { songs }
    }

    /// Best matches first (title-prefix, title-contains, artist), then by
    /// title length (shorter = closer), at most `limit`.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&SongRef> {
        let q = normalize(query);
        if q.is_empty() {
            return Vec::new();
        }
        let mut hits: Vec<(u32, usize, &SongRef)> = self
            .songs
            .iter()
            .filter_map(|s| s.rank(&q).map(|r| (r, s.title.chars().count(), s)))
            .collect();
        hits.sort_by_key(|h| (h.0, h.1));
        hits.into_iter().take(limit).map(|h| h.2).collect()
    }
}

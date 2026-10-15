//! Per-song bookmarks, best scores and play history.
//!
//! Stored on the SD card as a small tab-separated text file (one song per
//! line, keyed by its path relative to the library root) so it survives
//! firmware updates and can be read or edited on a PC:
//!
//! ```text
//! # piano-coach progress v1: path, bookmarked, best learn %, best follow %, plays, last played seq
//! 儿时回忆（41）/卖报歌 - 聂耳.mid	1	92	-	5	17
//! ```

use std::collections::BTreeMap;

const HEADER: &str =
    "# piano-coach progress v1: path, bookmarked, best learn %, best follow %, plays, last played seq";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub bookmarked: bool,
    pub best_learn: Option<u8>,
    pub best_follow: Option<u8>,
    pub plays: u32,
    /// Ordering key for "recently played" (higher = more recent).
    pub last: u64,
}

/// Which score a finished run counts towards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScoreKind {
    Learn,
    Follow,
}

/// 0–3 stars for an accuracy percentage.
pub fn stars(acc: u8) -> u8 {
    match acc {
        95.. => 3,
        80.. => 2,
        60.. => 1,
        _ => 0,
    }
}

pub fn star_text(acc: Option<u8>) -> String {
    match acc {
        Some(a) => {
            let n = stars(a) as usize;
            format!("{}{} {a}%", "★".repeat(n), "☆".repeat(3 - n))
        }
        None => "—".into(),
    }
}

#[derive(Debug, Clone, Default)]
pub struct Progress {
    songs: BTreeMap<String, Record>,
    seq: u64,
}

fn clean(path: &str) -> String {
    path.replace(['\t', '\n', '\r'], " ")
}

impl Progress {
    pub fn parse(text: &str) -> Progress {
        let mut p = Progress::default();
        for line in text.lines() {
            if line.starts_with('#') || line.trim().is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 6 {
                continue;
            }
            let pct = |s: &str| s.parse::<u8>().ok().filter(|&v| v <= 100);
            let r = Record {
                bookmarked: f[1] == "1",
                best_learn: pct(f[2]),
                best_follow: pct(f[3]),
                plays: f[4].parse().unwrap_or(0),
                last: f[5].parse().unwrap_or(0),
            };
            p.seq = p.seq.max(r.last);
            p.songs.insert(f[0].to_string(), r);
        }
        p
    }

    pub fn to_text(&self) -> String {
        let mut s = String::from(HEADER);
        s.push('\n');
        let pct = |v: Option<u8>| v.map(|v| v.to_string()).unwrap_or_else(|| "-".into());
        for (path, r) in &self.songs {
            s.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\n",
                path,
                r.bookmarked as u8,
                pct(r.best_learn),
                pct(r.best_follow),
                r.plays,
                r.last
            ));
        }
        s
    }

    pub fn get(&self, path: &str) -> Record {
        self.songs.get(path).cloned().unwrap_or_default()
    }

    fn entry(&mut self, path: &str) -> &mut Record {
        self.songs.entry(clean(path)).or_default()
    }

    /// Flip the bookmark; returns the new state.
    pub fn toggle_bookmark(&mut self, path: &str) -> bool {
        let r = self.entry(path);
        r.bookmarked = !r.bookmarked;
        let b = r.bookmarked;
        self.prune(path);
        b
    }

    pub fn record_play(&mut self, path: &str) {
        self.seq += 1;
        let seq = self.seq;
        let r = self.entry(path);
        r.plays += 1;
        r.last = seq;
    }

    /// Store a finished run's accuracy; returns true if it is a new best.
    pub fn record_score(&mut self, path: &str, kind: ScoreKind, acc: u8) -> bool {
        let acc = acc.min(100);
        let r = self.entry(path);
        let slot = match kind {
            ScoreKind::Learn => &mut r.best_learn,
            ScoreKind::Follow => &mut r.best_follow,
        };
        if slot.is_none_or(|b| acc > b) {
            *slot = Some(acc);
            true
        } else {
            false
        }
    }

    /// Bookmarked songs, alphabetical by path.
    pub fn bookmarks(&self) -> Vec<String> {
        self.songs.iter().filter(|(_, r)| r.bookmarked).map(|(p, _)| p.clone()).collect()
    }

    /// Most recently played first.
    pub fn recent(&self, n: usize) -> Vec<String> {
        let mut v: Vec<(&String, u64)> =
            self.songs.iter().filter(|(_, r)| r.last > 0).map(|(p, r)| (p, r.last)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v.into_iter().take(n).map(|(p, _)| p.clone()).collect()
    }

    /// Drop records that carry no information.
    fn prune(&mut self, path: &str) {
        if self.songs.get(path).is_some_and(|r| *r == Record::default()) {
            self.songs.remove(path);
        }
    }
}

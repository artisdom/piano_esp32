//! Piano-key remote control: the lowest and highest keys of the 88 act as
//! buttons (song, category, mode, hands, tempo, bookmark), so every board can
//! be driven from the piano alone.
//!
//! A control key works whenever nothing is being played. During a song it
//! is a note instead when the song plays that key, in a hand the learner
//! plays, close to where the song is ([`NOTE_BEFORE_MS`] / [`NOTE_AFTER_MS`])
//! — a real note is never taken for a command, and a key the song uses
//! only somewhere else still works. (A whole-song rule left F1 dead in 15 %
//! of the library and G1 in 24 %.)
//!
//! ```text
//! low end (left hand)                    high end (right hand)
//! A0  previous song                      D7  Left hand
//! A#0 tempo −10 %                        E7  Both hands
//! B0  next song                          F7  Right hand
//! C1  play / pause                       G7  Learn
//! C#1 tempo +10 %                        A7  Follow
//! D1  restart the song                   B7  Listen
//! E1  previous category (random song)    C8  bookmark the song
//! F1  next category (random song)
//! G1  random song in this folder
//! G#1 shuffle-play this folder (on / off)
//! ```
//!
//! Shuffle-play ([`Shuffle`]) plays every song of the folder once, in random
//! order, one after another; B0 / A0 then skip within that order, E1 / F1
//! jump to a random song of the next / previous category and shuffle its
//! folder, and choosing a song any other way ends it.
//!
//! Categories are the library's top-level folders (儿时回忆, 影视剧场, …):
//! below them the folders are often one film or composer with a single
//! song, so stepping those felt like stepping songs.

use crate::engine::Mode;
use crate::song::{HandSel, Song};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    PrevSong,
    NextSong,
    PlayPause,
    Restart,
    PrevCategory,
    NextCategory,
    RandomSong,
    ShuffleFolder,
    TempoDown,
    TempoUp,
    Mode(Mode),
    Hands(HandSel),
    Bookmark,
}

/// The key map (MIDI key → control), lowest key first.
pub const KEYS: [(u8, Control); 17] = [
    (21, Control::PrevSong),  // A0
    (22, Control::TempoDown), // A#0
    (23, Control::NextSong),  // B0
    (24, Control::PlayPause), // C1
    (25, Control::TempoUp),   // C#1
    (26, Control::Restart),   // D1
    (28, Control::PrevCategory), // E1
    (29, Control::NextCategory), // F1
    (31, Control::RandomSong), // G1
    (32, Control::ShuffleFolder), // G#1
    (98, Control::Hands(HandSel::Left)),   // D7
    (100, Control::Hands(HandSel::Both)),  // E7
    (101, Control::Hands(HandSel::Right)), // F7
    (103, Control::Mode(Mode::Learn)),     // G7
    (105, Control::Mode(Mode::Follow)),    // A7
    (107, Control::Mode(Mode::Listen)),    // B7
    (108, Control::Bookmark),              // C8
];

/// The control on `key`, if it is one.
pub fn control(key: u8) -> Option<Control> {
    KEYS.iter().find(|k| k.0 == key).map(|k| k.1)
}

/// The key of a control (for LED feedback).
pub fn key_of(c: Control) -> Option<u8> {
    KEYS.iter().find(|k| k.1 == c).map(|k| k.0)
}

/// Mid-song, a control key is a note if the song plays it from this long
/// before the current position (a late learner)…
pub const NOTE_BEFORE_MS: u32 = 2000;
/// …to this long after it (Follow lights a note up to 1.5 s ahead, plus
/// reaction). Song time: at 50 % tempo the window lasts twice as long.
pub const NOTE_AFTER_MS: u32 = 3000;

/// Where the song is, for [`press`]: the song, its position (song ms) and
/// the hands the learner plays.
pub type Playing<'a> = (&'a Song, u32, HandSel);

/// The note of `key` the learner may be playing at `pos_ms`: its start
/// relative to `pos_ms` (ms, negative = already started), the nearest one
/// in the window, only in `hands`.
pub fn note_near(song: &Song, key: u8, pos_ms: u32, hands: HandSel) -> Option<i64> {
    let from = pos_ms.saturating_sub(NOTE_BEFORE_MS);
    let to = pos_ms.saturating_add(NOTE_AFTER_MS);
    let first = song.notes.partition_point(|n| n.start_ms < from);
    song.notes[first..]
        .iter()
        .take_while(|n| n.start_ms <= to)
        .filter(|n| n.key == key && hands.includes(n.hand))
        .map(|n| n.start_ms as i64 - pos_ms as i64)
        .min_by_key(|d| d.abs())
}

/// What a press of `key` means: a control if the key is one and either
/// nothing is being played (`playing` None) or the song has no note of
/// that key near its position ([`note_near`]).
pub fn press(key: u8, playing: Option<Playing>) -> Option<Control> {
    let c = control(key)?;
    if playing.is_some_and(|(song, pos, hands)| note_near(song, key, pos, hands).is_some()) {
        return None;
    }
    Some(c)
}

/// Short name for toasts and logs.
pub fn label(c: Control) -> &'static str {
    match c {
        Control::PrevSong => "Previous song",
        Control::NextSong => "Next song",
        Control::PlayPause => "Play / pause",
        Control::Restart => "Restart",
        Control::PrevCategory => "Previous category",
        Control::NextCategory => "Next category",
        Control::RandomSong => "Random song",
        Control::ShuffleFolder => "Shuffle folder",
        Control::TempoDown => "Tempo −10 %",
        Control::TempoUp => "Tempo +10 %",
        Control::Mode(Mode::Learn) => "Learn",
        Control::Mode(Mode::Follow) => "Follow",
        Control::Mode(Mode::Listen) => "Listen",
        Control::Hands(HandSel::Both) => "Both hands",
        Control::Hands(HandSel::Right) => "Right hand",
        Control::Hands(HandSel::Left) => "Left hand",
        Control::Bookmark => "Bookmark",
    }
}

/// A folder's songs in a random order, each once (shuffle-play). Holds the
/// names; the caller keeps the folder.
#[derive(Debug, Clone)]
pub struct Shuffle {
    order: Vec<String>,
    pos: usize,
}

impl Shuffle {
    /// Shuffle `songs` with `seed`; `first`, if one of them, plays first
    /// (the song already playing when shuffle is switched on).
    pub fn new(mut songs: Vec<String>, first: Option<&str>, seed: u64) -> Shuffle {
        // xorshift64*; Fisher–Yates.
        let mut x = seed | 1;
        let mut next = || {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        };
        for i in (1..songs.len()).rev() {
            let j = (next() % (i as u64 + 1)) as usize;
            songs.swap(i, j);
        }
        if let Some(i) = first.and_then(|f| songs.iter().position(|s| s == f)) {
            let s = songs.remove(i);
            songs.insert(0, s);
        }
        Shuffle { order: songs, pos: 0 }
    }

    /// The song playing now.
    pub fn current(&self) -> Option<&str> {
        self.order.get(self.pos).map(String::as_str)
    }

    /// Move `d` songs on (back with `d` < 0, never before the first); None
    /// once past the last song (the folder is done).
    pub fn step(&mut self, d: isize) -> Option<&str> {
        let p = (self.pos as isize + d).max(0) as usize;
        if p >= self.order.len() {
            self.pos = self.order.len();
            return None;
        }
        self.pos = p;
        self.current()
    }

    pub fn len(&self) -> usize {
        self.order.len()
    }

    pub fn is_empty(&self) -> bool {
        self.order.is_empty()
    }

    /// Songs started so far (the current one included).
    pub fn played(&self) -> usize {
        (self.pos + 1).min(self.order.len())
    }
}

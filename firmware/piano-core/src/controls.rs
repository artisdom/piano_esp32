//! Piano-key remote control: the lowest and highest keys of the 88 act as
//! buttons (song, folder, mode, hands, tempo, bookmark), so every board can
//! be driven from the piano alone.
//!
//! A control key works whenever nothing is being played, and during a song
//! only if that song never uses the key — a real note is never taken for a
//! command. Nearly all songs stay inside C1–C7, so in practice the keys are
//! always there.
//!
//! ```text
//! low end (left hand)                    high end (right hand)
//! A0  previous song                      D7  Left hand
//! A#0 tempo −10 %                        E7  Both hands
//! B0  next song                          F7  Right hand
//! C1  play / pause                       G7  Learn
//! C#1 tempo +10 %                        A7  Follow
//! D1  restart the song                   B7  Listen
//! E1  previous folder (random song)      C8  bookmark the song
//! F1  next folder (random song)
//! G1  random song in this folder
//! ```

use crate::engine::Mode;
use crate::song::{HandSel, Song};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    PrevSong,
    NextSong,
    PlayPause,
    Restart,
    PrevFolder,
    NextFolder,
    RandomSong,
    TempoDown,
    TempoUp,
    Mode(Mode),
    Hands(HandSel),
    Bookmark,
}

/// The key map (MIDI key → control), lowest key first.
pub const KEYS: [(u8, Control); 16] = [
    (21, Control::PrevSong),  // A0
    (22, Control::TempoDown), // A#0
    (23, Control::NextSong),  // B0
    (24, Control::PlayPause), // C1
    (25, Control::TempoUp),   // C#1
    (26, Control::Restart),   // D1
    (28, Control::PrevFolder), // E1
    (29, Control::NextFolder), // F1
    (31, Control::RandomSong), // G1
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

/// Keys a song plays at all (any hand).
pub fn used_keys(song: &Song) -> [bool; 128] {
    let mut used = [false; 128];
    for n in &song.notes {
        used[n.key as usize & 127] = true;
    }
    used
}

/// What a press of `key` means: a control if the key is one and either
/// nothing is being played (`busy` false) or the song never uses it.
pub fn press(key: u8, busy: bool, used: Option<&[bool; 128]>) -> Option<Control> {
    let c = control(key)?;
    if busy && used.is_some_and(|u| u[key as usize & 127]) {
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
        Control::PrevFolder => "Previous folder",
        Control::NextFolder => "Next folder",
        Control::RandomSong => "Random song",
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

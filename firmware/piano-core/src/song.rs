//! A song as the player sees it: timed notes with a hand assignment, plus
//! the pedal controls worth forwarding to the piano.

use crate::smf::{self, Kind, Smf, TempoMap};

/// Lowest and highest keys of an 88-key piano.
pub const KEY_LO: u8 = 21;
pub const KEY_HI: u8 = 108;
/// Onsets closer than this are one chord ("step") for learning.
pub const CHORD_WINDOW_MS: u32 = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Right,
    Left,
}

/// Which notes the learner plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HandSel {
    #[default]
    Both,
    Right,
    Left,
}

impl HandSel {
    pub fn includes(self, h: Hand) -> bool {
        match self {
            HandSel::Both => true,
            HandSel::Right => h == Hand::Right,
            HandSel::Left => h == Hand::Left,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub start_ms: u32,
    pub end_ms: u32,
    pub key: u8,
    pub vel: u8,
    pub track: u8,
    pub hand: Hand,
}

/// Sustain/soft pedal changes (CC 64 / 66 / 67), already in ms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pedal {
    pub ms: u32,
    pub cc: u8,
    pub val: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitRule {
    /// Track names said left/right.
    TrackNames,
    /// Two (or more) note tracks: higher average pitch → right hand.
    TrackPitch,
    /// One note track: split at middle C.
    MiddleC,
}

/// Musical time: where the beats and bars are (for sheet music).
#[derive(Debug, Clone, Default)]
pub struct Meter {
    /// Tempo segments: (song ms, quarter notes, ms per quarter), by ms.
    pub tempo: Vec<(f64, f64, f64)>,
    /// Time signatures: (quarter-note position, numerator, denominator).
    pub time_sigs: Vec<(f64, u8, u8)>,
    /// Key signature at the start: sharps (> 0) or flats (< 0) — from the
    /// file, or estimated from the notes when the file has none.
    pub key: i8,
    /// The file declared the key signature.
    pub key_declared: bool,
}

/// The key signature (−6 flats … +6 sharps) whose major scale leaves the
/// fewest notes needing an accidental; ties go to fewer sharps/flats.
pub fn estimate_key(notes: &[Note]) -> i8 {
    let mut hist = [0u32; 12];
    for n in notes {
        hist[n.key as usize % 12] += 1;
    }
    let mut best = (u32::MAX, 0i8);
    for k in -6i8..=6 {
        // Tonic of the major key with k sharps: C + 7k semitones.
        let tonic = (7 * k as i32).rem_euclid(12) as usize;
        let outside: u32 = (0..12)
            .filter(|pc| ![0, 2, 4, 5, 7, 9, 11].contains(&((pc + 12 - tonic) % 12)))
            .map(|pc| hist[pc])
            .sum();
        if (outside, k.unsigned_abs()) < (best.0, best.1.unsigned_abs()) {
            best = (outside, k);
        }
    }
    best.1
}

impl Meter {
    /// Quarter notes elapsed at song time `ms`.
    pub fn q_at(&self, ms: f64) -> f64 {
        let i = self.tempo.partition_point(|s| s.0 <= ms).saturating_sub(1);
        match self.tempo.get(i) {
            Some(&(ms0, q0, mpq)) => q0 + (ms - ms0) / mpq,
            None => ms / 500.0,
        }
    }

    /// Song ms at quarter-note position `q`.
    pub fn ms_at(&self, q: f64) -> f64 {
        let i = self.tempo.partition_point(|s| s.1 <= q).saturating_sub(1);
        match self.tempo.get(i) {
            Some(&(ms0, q0, mpq)) => ms0 + (q - q0) * mpq,
            None => q * 500.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Song {
    /// Sorted by `start_ms`, then key.
    pub notes: Vec<Note>,
    pub pedals: Vec<Pedal>,
    pub duration_ms: u32,
    pub bpm: f64,
    pub note_tracks: usize,
    pub split: SplitRule,
    pub meter: Meter,
}

/// One learning step: the keys to press together at `ms`.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    pub ms: u32,
    pub keys: Vec<u8>,
}

fn name_hand(name: &str) -> Option<Hand> {
    let n = name.to_ascii_lowercase();
    let has = |w: &str| n.split(|c: char| !c.is_ascii_alphanumeric()).any(|t| t == w);
    if n.contains("right") || has("rh") || has("r") || name.contains('右') {
        Some(Hand::Right)
    } else if n.contains("left") || has("lh") || has("l") || name.contains('左') {
        Some(Hand::Left)
    } else {
        None
    }
}

impl Song {
    pub fn from_smf(smf: &Smf) -> Song {
        let tempo = TempoMap::new(smf);
        struct Raw {
            start: f64,
            end: f64,
            key: u8,
            vel: u8,
            track: u8,
        }
        let mut raw: Vec<Raw> = Vec::new();
        let mut pedals = Vec::new();
        let mut track_ids: Vec<u8> = Vec::new();
        let mut names: Vec<String> = Vec::new();

        for (ti, track) in smf.tracks.iter().enumerate() {
            let ti = ti.min(255) as u8;
            // Notes still held: (channel·128 + key, start ms, velocity), in
            // the order they started, so overlapping repeats of a key pair
            // first-on with first-off. Rarely more than a few dozen.
            let mut open: Vec<(u16, f64, u8)> = Vec::new();
            let mut any = false;
            for e in &track.events {
                match e.kind {
                    Kind::NoteOn { ch, key, vel } => {
                        // GM percussion has no place on a piano keyboard.
                        if ch == 9 {
                            continue;
                        }
                        open.push((ch as u16 * 128 + key as u16, tempo.ms(e.tick), vel));
                    }
                    Kind::NoteOff { ch, key } => {
                        let slot = ch as u16 * 128 + key as u16;
                        if let Some(i) = open.iter().position(|o| o.0 == slot) {
                            let (_, start, vel) = open.remove(i);
                            raw.push(Raw {
                                start,
                                end: tempo.ms(e.tick),
                                key,
                                vel,
                                track: ti,
                            });
                            any = true;
                        }
                    }
                    Kind::Control { ch, cc, val } if ch != 9 && matches!(cc, 64 | 66 | 67) => {
                        pedals.push(Pedal {
                            ms: tempo.ms(e.tick).round() as u32,
                            cc,
                            val,
                        });
                    }
                    _ => {}
                }
            }
            // Notes never released: end them at the track's last event.
            let last = track.events.last().map(|e| tempo.ms(e.tick)).unwrap_or(0.0);
            open.sort_by_key(|o| o.0);
            for &(slot, start, vel) in &open {
                raw.push(Raw {
                    start,
                    end: last.max(start + 250.0),
                    key: (slot % 128) as u8,
                    vel,
                    track: ti,
                });
                any = true;
            }
            if any {
                track_ids.push(ti);
                names.push(track.name.clone());
            }
        }

        // Hand assignment.
        let named: Vec<Option<Hand>> = names.iter().map(|n| name_hand(n)).collect();
        let (split, track_hand): (SplitRule, Vec<Option<Hand>>) =
            if track_ids.len() >= 2 && named.iter().all(|h| h.is_some()) {
                (SplitRule::TrackNames, named)
            } else if track_ids.len() >= 2 {
                let avg: Vec<f64> = track_ids
                    .iter()
                    .map(|&t| {
                        let ks: Vec<f64> =
                            raw.iter().filter(|r| r.track == t).map(|r| r.key as f64).collect();
                        ks.iter().sum::<f64>() / ks.len().max(1) as f64
                    })
                    .collect();
                if track_ids.len() == 2 {
                    let right = if avg[0] >= avg[1] { 0 } else { 1 };
                    (
                        SplitRule::TrackPitch,
                        (0..2)
                            .map(|i| Some(if i == right { Hand::Right } else { Hand::Left }))
                            .collect(),
                    )
                } else {
                    (
                        SplitRule::TrackPitch,
                        avg.iter()
                            .map(|&a| Some(if a >= 60.0 { Hand::Right } else { Hand::Left }))
                            .collect(),
                    )
                }
            } else {
                (SplitRule::MiddleC, vec![None; track_ids.len()])
            };

        let mut notes: Vec<Note> = raw
            .into_iter()
            .map(|r| {
                let hand = track_ids
                    .iter()
                    .position(|&t| t == r.track)
                    .and_then(|i| track_hand[i])
                    .unwrap_or(if r.key >= 60 { Hand::Right } else { Hand::Left });
                // Fold anything outside the 88 keys into range by octaves.
                let mut key = r.key;
                while key < KEY_LO {
                    key += 12;
                }
                while key > KEY_HI {
                    key -= 12;
                }
                let start_ms = r.start.round().max(0.0) as u32;
                Note {
                    start_ms,
                    end_ms: (r.end.round() as u32).max(start_ms + 20),
                    key,
                    vel: r.vel,
                    track: r.track,
                    hand,
                }
            })
            .collect();
        notes.sort_by_key(|n| (n.start_ms, n.key));
        pedals.sort_by_key(|p| p.ms);

        let duration_ms = notes.iter().map(|n| n.end_ms).max().unwrap_or(0);
        let mut time_sigs: Vec<(f64, u8, u8)> = Vec::new();
        let mut key = None;
        let ppq = match smf.division {
            crate::smf::Division::Ppq(p) => Some(p as f64),
            _ => None,
        };
        let mut sigs: Vec<(u64, u8, u8)> = Vec::new();
        for e in smf.tracks.iter().flat_map(|t| t.events.iter()) {
            match e.kind {
                Kind::TimeSig { num, den } => sigs.push((e.tick, num, den)),
                Kind::KeySig { sf, .. } if key.is_none() && e.tick == 0 => key = Some(sf),
                _ => {}
            }
        }
        sigs.sort_by_key(|s| s.0);
        if let Some(ppq) = ppq {
            for (tick, num, den) in sigs {
                let q = tick as f64 / ppq;
                match time_sigs.last_mut() {
                    Some(last) if (last.0 - q).abs() < 1e-6 => *last = (q, num, den),
                    Some(last) if last.1 == num && last.2 == den => {}
                    _ => time_sigs.push((q, num, den)),
                }
            }
        }
        let meter = Meter {
            tempo: tempo.quarter_segments(),
            time_sigs,
            key: key.unwrap_or_else(|| estimate_key(&notes)),
            key_declared: key.is_some(),
        };
        Song {
            notes,
            pedals,
            duration_ms,
            bpm: tempo.initial_bpm(),
            note_tracks: track_ids.len(),
            split,
            meter,
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Song, smf::SmfError> {
        Ok(Song::from_smf(&smf::parse(bytes)?))
    }

    /// Start of the latest note at or before `ms` (for a loop start that
    /// lands on a chord rather than mid-phrase); 0 if none.
    pub fn onset_at_or_before(&self, ms: u32) -> u32 {
        match self.notes.partition_point(|n| n.start_ms <= ms) {
            0 => 0,
            i => self.notes[i - 1].start_ms,
        }
    }

    /// Group the selected hand's notes into chords to press together.
    pub fn steps(&self, hands: HandSel) -> Vec<Step> {
        let mut steps: Vec<Step> = Vec::new();
        for n in self.notes.iter().filter(|n| hands.includes(n.hand)) {
            match steps.last_mut() {
                Some(s) if n.start_ms.saturating_sub(s.ms) <= CHORD_WINDOW_MS => {
                    if !s.keys.contains(&n.key) {
                        s.keys.push(n.key);
                    }
                }
                _ => steps.push(Step {
                    ms: n.start_ms,
                    keys: vec![n.key],
                }),
            }
        }
        steps
    }
}

/// "C4", "F#5" … for status lines.
pub fn key_name(key: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[(key % 12) as usize], key as i32 / 12 - 1)
}

pub fn is_black(key: u8) -> bool {
    matches!(key % 12, 1 | 3 | 6 | 8 | 10)
}

//! Standard MIDI File (SMF) reader: formats 0/1/2, PPQ or SMPTE timing,
//! running status, SysEx and meta events. Produces per-track events on
//! absolute ticks plus a merged tempo map; `crate::song` turns that into
//! timed notes.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmfError {
    NotMidi,
    Truncated,
    BadDivision,
}

impl fmt::Display for SmfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SmfError::NotMidi => write!(f, "not a MIDI file"),
            SmfError::Truncated => write!(f, "file is truncated"),
            SmfError::BadDivision => write!(f, "unsupported time division"),
        }
    }
}

impl std::error::Error for SmfError {}

/// Timing base from the header.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Division {
    /// Ticks per quarter note; tempo meta events give µs per quarter.
    Ppq(u16),
    /// Frames per second × ticks per frame — absolute, tempo-independent.
    Smpte { fps: u8, ticks_per_frame: u8 },
}

/// The channel-voice and meta events the player cares about.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    NoteOn { ch: u8, key: u8, vel: u8 },
    NoteOff { ch: u8, key: u8 },
    Control { ch: u8, cc: u8, val: u8 },
    Program { ch: u8, program: u8 },
    /// µs per quarter note.
    Tempo(u32),
    TrackName(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub tick: u64,
    pub kind: Kind,
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub name: String,
    pub events: Vec<Event>,
}

#[derive(Debug, Clone)]
pub struct Smf {
    pub format: u16,
    pub division: Division,
    pub tracks: Vec<Track>,
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> Reader<'a> {
    fn u8(&mut self) -> Result<u8, SmfError> {
        let v = *self.b.get(self.i).ok_or(SmfError::Truncated)?;
        self.i += 1;
        Ok(v)
    }
    fn peek(&self) -> Result<u8, SmfError> {
        self.b.get(self.i).copied().ok_or(SmfError::Truncated)
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], SmfError> {
        let end = self.i.checked_add(n).ok_or(SmfError::Truncated)?;
        let s = self.b.get(self.i..end).ok_or(SmfError::Truncated)?;
        self.i = end;
        Ok(s)
    }
    fn be(&mut self, n: usize) -> Result<u32, SmfError> {
        Ok(self.take(n)?.iter().fold(0u32, |a, &x| (a << 8) | x as u32))
    }
    /// Variable-length quantity (max 4 bytes per spec; tolerate a 5th).
    fn vlq(&mut self) -> Result<u32, SmfError> {
        let mut v = 0u32;
        for _ in 0..5 {
            let c = self.u8()?;
            v = (v << 7) | (c & 0x7f) as u32;
            if c & 0x80 == 0 {
                return Ok(v);
            }
        }
        Ok(v)
    }
}

/// Decode track-name bytes. Many files in the library were authored on
/// Chinese Windows and store GBK; we only need the ASCII words
/// (left/right/piano) for hand detection, so non-UTF-8 names keep their
/// ASCII characters and drop the rest.
fn decode_name(d: &[u8]) -> String {
    match std::str::from_utf8(d) {
        Ok(s) => s.trim_matches(char::from(0)).trim().to_string(),
        Err(_) => d
            .iter()
            .filter(|c| c.is_ascii() && **c >= 0x20)
            .map(|&c| c as char)
            .collect::<String>()
            .trim()
            .to_string(),
    }
}

pub fn parse(bytes: &[u8]) -> Result<Smf, SmfError> {
    let mut r = Reader { b: bytes, i: 0 };
    // RIFF-wrapped MIDI (RMID) — skip to the embedded MThd.
    if bytes.starts_with(b"RIFF") {
        let pos = bytes
            .windows(4)
            .position(|w| w == b"MThd")
            .ok_or(SmfError::NotMidi)?;
        r.i = pos;
    }
    if r.take(4)? != b"MThd" {
        return Err(SmfError::NotMidi);
    }
    let hlen = r.be(4)? as usize;
    let header = r.take(hlen)?;
    if header.len() < 6 {
        return Err(SmfError::Truncated);
    }
    let format = u16::from_be_bytes([header[0], header[1]]);
    let ntracks = u16::from_be_bytes([header[2], header[3]]) as usize;
    let div = u16::from_be_bytes([header[4], header[5]]);
    let division = if div & 0x8000 != 0 {
        let fps = (-((div >> 8) as i8)) as u8;
        let tpf = (div & 0xff) as u8;
        if fps == 0 || tpf == 0 {
            return Err(SmfError::BadDivision);
        }
        Division::Smpte {
            fps,
            ticks_per_frame: tpf,
        }
    } else {
        if div == 0 {
            return Err(SmfError::BadDivision);
        }
        Division::Ppq(div)
    };

    let mut tracks = Vec::with_capacity(ntracks);
    while tracks.len() < ntracks && r.i + 8 <= bytes.len() {
        let id = r.take(4)?;
        let len = r.be(4)? as usize;
        // A truncated last chunk is common in the wild: parse what exists.
        let avail = len.min(bytes.len() - r.i);
        let chunk = r.take(avail)?;
        if id != b"MTrk" {
            continue; // unknown chunk types must be skipped
        }
        tracks.push(parse_track(chunk));
    }
    if tracks.is_empty() {
        return Err(SmfError::Truncated);
    }
    Ok(Smf {
        format,
        division,
        tracks,
    })
}

/// Parse one MTrk body. Stops quietly at the first malformed byte, keeping
/// the events decoded so far.
fn parse_track(chunk: &[u8]) -> Track {
    let mut r = Reader { b: chunk, i: 0 };
    let mut t = Track::default();
    let mut tick = 0u64;
    let mut status = 0u8;
    let _ = (|| -> Result<(), SmfError> {
        while r.i < chunk.len() {
            tick += r.vlq()? as u64;
            let mut c = r.peek()?;
            if c >= 0x80 {
                r.i += 1;
            } else if status == 0 {
                return Err(SmfError::Truncated); // data byte with no status
            } else {
                c = status; // running status
            }
            match c {
                0xff => {
                    let ty = r.u8()?;
                    let len = r.vlq()? as usize;
                    let d = r.take(len)?;
                    match ty {
                        0x2f => break,
                        0x51 if d.len() == 3 => {
                            let us = (d[0] as u32) << 16 | (d[1] as u32) << 8 | d[2] as u32;
                            if us > 0 {
                                t.events.push(Event {
                                    tick,
                                    kind: Kind::Tempo(us),
                                });
                            }
                        }
                        0x03 => {
                            let name = decode_name(d);
                            if t.name.is_empty() {
                                t.name = name.clone();
                            }
                            t.events.push(Event {
                                tick,
                                kind: Kind::TrackName(name),
                            });
                        }
                        _ => {}
                    }
                    // Meta and SysEx events cancel running status.
                    status = 0;
                }
                0xf0 | 0xf7 => {
                    let len = r.vlq()? as usize;
                    r.take(len)?;
                    status = 0;
                }
                0x80..=0xef => {
                    status = c;
                    let ch = c & 0x0f;
                    let d1 = r.u8()? & 0x7f;
                    let hi = c & 0xf0;
                    let d2 = if hi == 0xc0 || hi == 0xd0 {
                        0
                    } else {
                        r.u8()? & 0x7f
                    };
                    let kind = match hi {
                        0x90 if d2 > 0 => Some(Kind::NoteOn {
                            ch,
                            key: d1,
                            vel: d2,
                        }),
                        0x90 | 0x80 => Some(Kind::NoteOff { ch, key: d1 }),
                        0xb0 => Some(Kind::Control { ch, cc: d1, val: d2 }),
                        0xc0 => Some(Kind::Program { ch, program: d1 }),
                        _ => None, // aftertouch, pitch bend: not used
                    };
                    if let Some(kind) = kind {
                        t.events.push(Event { tick, kind });
                    }
                }
                // System common/real-time bytes do not belong in files;
                // skip the single byte and carry on.
                _ => {}
            }
        }
        Ok(())
    })();
    t
}

/// Tick → milliseconds converter built from every tempo event in the file
/// (format-1 files may place tempo changes outside track 0).
pub struct TempoMap {
    division: Division,
    /// (tick, ms at that tick, µs per quarter from that tick on)
    segments: Vec<(u64, f64, u32)>,
}

impl TempoMap {
    pub fn new(smf: &Smf) -> Self {
        let mut changes: Vec<(u64, u32)> = smf
            .tracks
            .iter()
            .flat_map(|t| t.events.iter())
            .filter_map(|e| match e.kind {
                Kind::Tempo(us) => Some((e.tick, us)),
                _ => None,
            })
            .collect();
        changes.sort_by_key(|c| c.0);
        let mut segments = vec![(0u64, 0.0f64, 500_000u32)];
        for (tick, us) in changes {
            let &(t0, ms0, us0) = segments.last().unwrap();
            let ms = ms0 + Self::span_ms(smf.division, tick - t0, us0);
            if tick == t0 {
                segments.last_mut().unwrap().2 = us;
            } else {
                segments.push((tick, ms, us));
            }
        }
        TempoMap {
            division: smf.division,
            segments,
        }
    }

    fn span_ms(division: Division, ticks: u64, us_per_q: u32) -> f64 {
        match division {
            Division::Ppq(ppq) => ticks as f64 * us_per_q as f64 / ppq as f64 / 1000.0,
            Division::Smpte {
                fps,
                ticks_per_frame,
            } => {
                // -29 means 29.97 drop-frame.
                let fps = if fps == 29 { 29.97 } else { fps as f64 };
                ticks as f64 * 1000.0 / (fps * ticks_per_frame as f64)
            }
        }
    }

    pub fn ms(&self, tick: u64) -> f64 {
        let idx = self.segments.partition_point(|s| s.0 <= tick).saturating_sub(1);
        let (t0, ms0, us) = self.segments[idx];
        ms0 + Self::span_ms(self.division, tick - t0, us)
    }

    /// Initial tempo in beats per minute (for display).
    pub fn initial_bpm(&self) -> f64 {
        60_000_000.0 / self.segments[0].2 as f64
    }
}

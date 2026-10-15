//! USB-MIDI 1.0 event-packet framing (USB Device Class Definition for MIDI
//! Devices 1.0, §4): every message travels as 4 bytes
//! `[cable<<4 | CIN, b0, b1, b2]`.

/// Encode one channel-voice / system message for cable 0.
pub fn encode(msg: &[u8]) -> Option<[u8; 4]> {
    let s = *msg.first()?;
    let cin = match s & 0xf0 {
        0x80 => 0x8,
        0x90 => 0x9,
        0xa0 => 0xa,
        0xb0 => 0xb,
        0xc0 => 0xc,
        0xd0 => 0xd,
        0xe0 => 0xe,
        _ => match s {
            0xf8..=0xff => 0xf,
            0xf1 | 0xf3 => 0x2,
            0xf2 => 0x3,
            _ => return None, // SysEx goes through `encode_sysex`
        },
    };
    let len = match cin {
        0xc | 0xd | 0x2 => 2,
        0xf => 1,
        _ => 3,
    };
    if msg.len() < len {
        return None;
    }
    let mut p = [cin, 0, 0, 0];
    p[1..1 + len].copy_from_slice(&msg[..len]);
    Some(p)
}

/// Split a complete `F0 … F7` SysEx into packets.
pub fn encode_sysex(sx: &[u8]) -> Vec<[u8; 4]> {
    let mut out = Vec::new();
    let mut chunks = sx.chunks(3).peekable();
    while let Some(c) = chunks.next() {
        let last = chunks.peek().is_none();
        let cin = if !last {
            0x4
        } else {
            match c.len() {
                1 => 0x5,
                2 => 0x6,
                _ => 0x7,
            }
        };
        let mut p = [cin, 0, 0, 0];
        p[1..1 + c.len()].copy_from_slice(c);
        out.push(p);
    }
    out
}

/// Something decoded from the IN endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Short(Vec<u8>),
    SysEx(Vec<u8>),
}

/// Reassembles SysEx across packets; short messages pass straight through.
#[derive(Default)]
pub struct Decoder {
    sysex: Vec<u8>,
}

impl Decoder {
    /// Feed a received buffer (any multiple of 4 bytes; zero padding ignored).
    pub fn feed(&mut self, buf: &[u8], mut f: impl FnMut(Incoming)) {
        for p in buf.chunks_exact(4) {
            let cin = p[0] & 0x0f;
            match cin {
                0x0 | 0x1 => {} // reserved / padding
                0x4 => self.sysex.extend_from_slice(&p[1..4]),
                0x5 | 0x6 | 0x7 => {
                    let n = (cin - 0x4) as usize;
                    // CIN 5 with a lone non-F7 byte is a 1-byte system common.
                    if cin == 0x5 && p[1] != 0xf7 && self.sysex.is_empty() {
                        f(Incoming::Short(vec![p[1]]));
                        continue;
                    }
                    self.sysex.extend_from_slice(&p[1..1 + n]);
                    f(Incoming::SysEx(std::mem::take(&mut self.sysex)));
                }
                0x2 | 0xc | 0xd => f(Incoming::Short(p[1..3].to_vec())),
                0xf => f(Incoming::Short(vec![p[1]])),
                _ => f(Incoming::Short(p[1..4].to_vec())),
            }
        }
    }
}

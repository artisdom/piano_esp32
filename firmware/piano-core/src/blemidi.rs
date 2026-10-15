//! BLE-MIDI packets as the GZUT piano wants them (doc/GZUT_piano_protocol.md
//! §2.2): the piano drops any packet whose header byte is not exactly
//! `0x80` and parses the rest as a plain MIDI byte stream, so we always
//! send `80 80` followed by complete messages and never insert further
//! timestamp bytes.

/// Largest write the piano accepted in tests was 257 bytes; stay below.
pub const MAX_PACKET: usize = 244;

/// Length of a channel/system message from its status byte.
pub fn msg_len(status: u8) -> usize {
    match status & 0xf0 {
        0xc0 | 0xd0 => 2,
        0x80..=0xe0 => 3,
        _ => match status {
            0xf1 | 0xf3 => 2,
            0xf2 => 3,
            _ => 1,
        },
    }
}

/// Packs messages into `80 80 …` packets of at most `limit` bytes.
pub struct Packer {
    limit: usize,
    cur: Vec<u8>,
}

impl Packer {
    pub fn new(limit: usize) -> Packer {
        Packer {
            limit: limit.clamp(5, MAX_PACKET),
            cur: Vec::new(),
        }
    }

    /// Add one message (`msg[0]` is the status byte; extra bytes beyond the
    /// status's length are ignored). Returns a full packet when `msg` did not
    /// fit into the current one.
    pub fn push(&mut self, msg: &[u8]) -> Option<Vec<u8>> {
        let Some(&s) = msg.first() else { return None };
        let n = msg_len(s).min(msg.len());
        let mut done = None;
        if !self.cur.is_empty() && self.cur.len() + n > self.limit {
            done = Some(std::mem::take(&mut self.cur));
        }
        if self.cur.is_empty() {
            self.cur.extend_from_slice(&[0x80, 0x80]);
        }
        self.cur.extend_from_slice(&msg[..n]);
        done
    }

    /// The pending packet, if any.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        (!self.cur.is_empty()).then(|| std::mem::take(&mut self.cur))
    }
}

/// Decode a received BLE-MIDI packet (standard framing: header, then
/// timestamp + status + data, running status allowed, SysEx ended by a
/// timestamp + F7) into complete MIDI messages.
pub fn decode(pkt: &[u8], mut f: impl FnMut(&[u8])) {
    if pkt.len() < 2 || pkt[0] & 0x80 == 0 {
        return;
    }
    let mut i = 1;
    let mut status = 0u8;
    let mut sysex: Option<Vec<u8>> = None;
    while i < pkt.len() {
        let b = pkt[i];
        if let Some(sx) = sysex.as_mut() {
            if b & 0x80 == 0 {
                sx.push(b);
                i += 1;
                continue;
            }
            // Timestamp byte; F7 (or another status) follows.
            i += 1;
            if i < pkt.len() && pkt[i] == 0xf7 {
                sx.push(0xf7);
                f(sx);
                sysex = None;
                i += 1;
            }
            continue;
        }
        let mut fresh = false;
        if b & 0x80 != 0 {
            // Timestamp, optionally followed by a status byte.
            i += 1;
            if i < pkt.len() && pkt[i] & 0x80 != 0 {
                status = pkt[i];
                fresh = true;
                i += 1;
                if status == 0xf0 {
                    sysex = Some(vec![0xf0]);
                    continue;
                }
            }
        }
        let need = msg_len(status) - 1;
        if status == 0 || (need == 0 && !fresh) {
            // Stray data byte: no status, or after a one-byte message.
            i += 1;
            continue;
        }
        if i + need > pkt.len() {
            break;
        }
        let mut m = [status, 0, 0];
        m[1..1 + need].copy_from_slice(&pkt[i..i + need]);
        f(&m[..1 + need]);
        i += need;
    }
}

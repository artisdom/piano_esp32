//! Song pack: a read-only song library in one blob, for boards without a
//! microSD card. The pack sits in a flash data partition and is read through
//! a memory map, so browsing costs no RAM; one song is inflated when loaded.
//!
//! Built on the PC by `src/bin/songpack.rs` from a folder of MIDI files
//! (with a size budget for small flash chips).
//!
//! Layout (all integers little-endian):
//!
//! ```text
//! header  32 B   "PCSP", u16 version (1), u16 0, u32 node count,
//!                u32 nodes offset, u32 names offset, u32 names length,
//!                u32 total pack length, u32 CRC-32 of nodes + names
//! nodes   28 B each, node 0 = root folder:
//!                u32 name offset (in names), u16 name length, u8 kind
//!                (0 folder, 1 song), u8 0, u32 parent,
//!                folder: u32 first child, u32 child count, u32 songs below, u32 0
//!                song:   u32 data offset (in pack), u32 zlib length,
//!                        u32 MIDI length, u32 duration ms
//! names   UTF-8 file names (folder name / song file name, as on disk)
//! data    zlib-compressed MIDI files
//! ```
//!
//! A folder's children are contiguous and already in library order
//! (folders first, natural sort — [`crate::library::sort_entries`]).

use std::ops::Range;

pub const MAGIC: &[u8; 4] = b"PCSP";
pub const VERSION: u16 = 1;
const HEADER_LEN: usize = 32;
const NODE_LEN: usize = 28;
const KIND_DIR: u8 = 0;
const KIND_SONG: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackError {
    TooShort,
    BadMagic,
    Version(u16),
    Corrupt(&'static str),
    Crc,
    NotASong(u32),
    Inflate,
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackError::TooShort => write!(f, "song pack is truncated"),
            PackError::BadMagic => write!(f, "no song pack (bad magic)"),
            PackError::Version(v) => write!(f, "song pack version {v} is not supported"),
            PackError::Corrupt(what) => write!(f, "song pack is corrupt ({what})"),
            PackError::Crc => write!(f, "song pack index checksum mismatch"),
            PackError::NotASong(i) => write!(f, "pack entry {i} is not a song"),
            PackError::Inflate => write!(f, "song data does not decompress"),
        }
    }
}

impl std::error::Error for PackError {}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

/// CRC-32 (IEEE, reflected), bitwise: only the index is checked, once.
pub fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// FNV-1a 32 of a song's path: a short, stable key for per-song settings.
pub fn path_key(path: &str) -> u32 {
    let mut h = 0x811c_9dc5u32;
    for b in path.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Song,
}

/// One entry of the pack.
#[derive(Debug, Clone, Copy)]
pub struct Node<'a> {
    pub index: u32,
    pub name: &'a str,
    pub kind: Kind,
    pub parent: u32,
    a: u32,
    b: u32,
    c: u32,
    d: u32,
}

impl Node<'_> {
    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Folder
    }
    /// Folder: its children's indices.
    pub fn children(&self) -> Range<u32> {
        if self.is_dir() {
            self.a..self.a + self.b
        } else {
            0..0
        }
    }
    /// Folder: songs anywhere below it. Song: 1.
    pub fn songs(&self) -> u32 {
        if self.is_dir() {
            self.c
        } else {
            1
        }
    }
    /// Song: duration in ms (0 for folders).
    pub fn duration_ms(&self) -> u32 {
        if self.is_dir() {
            0
        } else {
            self.d
        }
    }
    /// Song: size of the MIDI file.
    pub fn midi_len(&self) -> u32 {
        if self.is_dir() {
            0
        } else {
            self.c
        }
    }
}

/// A validated pack over borrowed bytes (a flash memory map on the device).
#[derive(Debug, Clone, Copy)]
pub struct Pack<'a> {
    bytes: &'a [u8],
    count: u32,
    nodes: usize,
    names: usize,
    names_len: usize,
}

impl<'a> Pack<'a> {
    /// Check the header and index (bounds, CRC, tree links). `bytes` may be
    /// longer than the pack (a whole partition).
    pub fn new(bytes: &'a [u8]) -> Result<Pack<'a>, PackError> {
        if bytes.len() < HEADER_LEN {
            return Err(PackError::TooShort);
        }
        if &bytes[0..4] != MAGIC {
            return Err(PackError::BadMagic);
        }
        let v = u16_at(bytes, 4);
        if v != VERSION {
            return Err(PackError::Version(v));
        }
        let count = u32_at(bytes, 8);
        let nodes = u32_at(bytes, 12) as usize;
        let names = u32_at(bytes, 16) as usize;
        let names_len = u32_at(bytes, 20) as usize;
        let total = u32_at(bytes, 24) as usize;
        if total > bytes.len() {
            return Err(PackError::TooShort);
        }
        let bytes = &bytes[..total];
        if count == 0 || nodes != HEADER_LEN || names != nodes + count as usize * NODE_LEN || names + names_len > total {
            return Err(PackError::Corrupt("index bounds"));
        }
        if crc32(&bytes[nodes..names + names_len]) != u32_at(bytes, 28) {
            return Err(PackError::Crc);
        }
        let p = Pack {
            bytes,
            count,
            nodes,
            names,
            names_len,
        };
        for i in 0..count {
            let o = p.nodes + i as usize * NODE_LEN;
            let (no, nl) = (u32_at(bytes, o) as usize, u16_at(bytes, o + 4) as usize);
            if no + nl > names_len || std::str::from_utf8(&bytes[names + no..names + no + nl]).is_err() {
                return Err(PackError::Corrupt("name"));
            }
            let (kind, parent) = (bytes[o + 6], u32_at(bytes, o + 8));
            let (a, b) = (u32_at(bytes, o + 12) as u64, u32_at(bytes, o + 16) as u64);
            match kind {
                KIND_DIR if a + b <= count as u64 => {}
                KIND_SONG if a + b <= total as u64 => {}
                _ => return Err(PackError::Corrupt("node")),
            }
            if parent >= count {
                return Err(PackError::Corrupt("parent"));
            }
        }
        if !p.node(0).is_dir() {
            return Err(PackError::Corrupt("root"));
        }
        Ok(p)
    }

    pub fn len(&self) -> usize {
        self.count as usize
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    /// Bytes the pack occupies.
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
    pub fn root(&self) -> Node<'a> {
        self.node(0)
    }

    /// Entry `i` (panics if out of range).
    pub fn node(&self, i: u32) -> Node<'a> {
        assert!(i < self.count, "pack node {i} out of range");
        let b = self.bytes;
        let o = self.nodes + i as usize * NODE_LEN;
        let (no, nl) = (u32_at(b, o) as usize, u16_at(b, o + 4) as usize);
        let s = self.names + no;
        debug_assert!(no + nl <= self.names_len);
        Node {
            index: i,
            // Validated in `new`.
            name: std::str::from_utf8(&b[s..s + nl]).unwrap_or(""),
            kind: if b[o + 6] == KIND_DIR { Kind::Folder } else { Kind::Song },
            parent: u32_at(b, o + 8),
            a: u32_at(b, o + 12),
            b: u32_at(b, o + 16),
            c: u32_at(b, o + 20),
            d: u32_at(b, o + 24),
        }
    }

    /// "folder/sub/song.mid" relative to the pack root.
    pub fn path(&self, i: u32) -> String {
        let mut parts = Vec::new();
        let mut n = self.node(i);
        // Bounded: a corrupt parent loop must not hang the device.
        for _ in 0..64 {
            if n.index == 0 {
                break;
            }
            parts.push(n.name);
            n = self.node(n.parent);
        }
        parts.reverse();
        parts.join("/")
    }

    /// Inflate song `i` back into its MIDI file.
    pub fn midi(&self, i: u32) -> Result<Vec<u8>, PackError> {
        let n = self.node(i);
        if n.is_dir() {
            return Err(PackError::NotASong(i));
        }
        let (off, len) = (n.a as usize, n.b as usize);
        let data = self.bytes.get(off..off + len).ok_or(PackError::Corrupt("data bounds"))?;
        // The decompressor is ~11 KB. miniz_oxide's helpers build it (and
        // the stream API a 32 KB window too) on the stack before boxing it,
        // which overflows a small player thread. Its `Default` is documented
        // as all fields zero (`State::Start` = 0), so a zeroed heap block is
        // that value, built in place. The whole song is decoded into one
        // buffer, which also serves as the window.
        use miniz_oxide::inflate::core::{decompress, inflate_flags, DecompressorOxide};
        use miniz_oxide::inflate::TINFLStatus;
        // SAFETY: an all-zero DecompressorOxide is its Default value.
        let mut r: Box<DecompressorOxide> = unsafe { Box::new_zeroed().assume_init() };
        let mut out = vec![0u8; n.c as usize];
        let flags = inflate_flags::TINFL_FLAG_PARSE_ZLIB_HEADER
            | inflate_flags::TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF
            | inflate_flags::TINFL_FLAG_COMPUTE_ADLER32;
        let (status, _, written) = decompress(&mut r, data, &mut out, 0, flags);
        if status != TINFLStatus::Done || written != out.len() {
            return Err(PackError::Inflate);
        }
        Ok(out)
    }

    /// All song indices, depth-first in library order.
    pub fn songs(&self) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![0u32];
        while let Some(i) = stack.pop() {
            let n = self.node(i);
            if n.is_dir() {
                stack.extend(n.children().rev());
            } else {
                out.push(i);
            }
        }
        out
    }
}

// ---- building (PC side) --------------------------------------------------

/// A song to put into a pack.
pub struct Input {
    /// Path relative to the library root, '/'-separated.
    pub path: String,
    pub midi: Vec<u8>,
    pub duration_ms: u32,
}

#[derive(Default)]
struct Dir {
    name: String,
    dirs: Vec<Dir>,
    songs: Vec<(String, usize)>,
}

impl Dir {
    fn insert(&mut self, parts: &[&str], song: usize) {
        match parts {
            [file] => self.songs.push((file.to_string(), song)),
            [dir, rest @ ..] => {
                let d = match self.dirs.iter().position(|d| d.name == *dir) {
                    Some(i) => &mut self.dirs[i],
                    None => {
                        self.dirs.push(Dir {
                            name: dir.to_string(),
                            ..Default::default()
                        });
                        self.dirs.last_mut().unwrap()
                    }
                };
                d.insert(rest, song);
            }
            [] => {}
        }
    }
    fn count(&self) -> u32 {
        self.songs.len() as u32 + self.dirs.iter().map(Dir::count).sum::<u32>()
    }
}

/// Build a pack from songs (any order). Compression level 9.
pub fn build(songs: &[Input]) -> Vec<u8> {
    use crate::library::natural_cmp;

    let mut root = Dir::default();
    for (i, s) in songs.iter().enumerate() {
        let parts: Vec<&str> = s.path.split('/').filter(|p| !p.is_empty()).collect();
        root.insert(&parts, i);
    }

    // Breadth-first: each folder's children land contiguously.
    struct Rec {
        name: String,
        kind: u8,
        parent: u32,
        a: u32,
        b: u32,
        c: u32,
        d: u32,
    }
    let mut recs = vec![Rec {
        name: String::new(),
        kind: KIND_DIR,
        parent: 0,
        a: 0,
        b: 0,
        c: root.count(),
        d: 0,
    }];
    let mut data: Vec<u8> = Vec::new();
    let mut song_data: Vec<(u32, usize)> = Vec::new(); // (record, input index)
    let mut queue = std::collections::VecDeque::from([(0u32, &root)]);
    while let Some((ri, dir)) = queue.pop_front() {
        let mut dirs: Vec<&Dir> = dir.dirs.iter().collect();
        dirs.sort_by(|x, y| natural_cmp(&x.name, &y.name));
        let mut files: Vec<&(String, usize)> = dir.songs.iter().collect();
        files.sort_by(|x, y| natural_cmp(&x.0, &y.0));
        let first = recs.len() as u32;
        for d in &dirs {
            recs.push(Rec {
                name: d.name.clone(),
                kind: KIND_DIR,
                parent: ri,
                a: 0,
                b: 0,
                c: d.count(),
                d: 0,
            });
        }
        for (name, si) in &files {
            song_data.push((recs.len() as u32, *si));
            recs.push(Rec {
                name: name.clone(),
                kind: KIND_SONG,
                parent: ri,
                a: 0,
                b: 0,
                c: songs[*si].midi.len() as u32,
                d: songs[*si].duration_ms,
            });
        }
        recs[ri as usize].a = first;
        recs[ri as usize].b = (dirs.len() + files.len()) as u32;
        for (k, d) in dirs.into_iter().enumerate() {
            queue.push_back((first + k as u32, d));
        }
    }

    let mut names = Vec::new();
    let mut name_at = Vec::new();
    for r in &recs {
        name_at.push((names.len() as u32, r.name.len() as u16));
        names.extend_from_slice(r.name.as_bytes());
    }
    let names_off = HEADER_LEN + recs.len() * NODE_LEN;
    let data_off = names_off + names.len();
    // Song data in index order (sequential reads when browsing in order).
    song_data.sort_by_key(|x| x.0);
    for &(ri, si) in &song_data {
        let z = miniz_oxide::deflate::compress_to_vec_zlib(&songs[si].midi, 9);
        recs[ri as usize].a = (data_off + data.len()) as u32;
        recs[ri as usize].b = z.len() as u32;
        data.extend(z);
    }

    let mut out = Vec::with_capacity(data_off + data.len());
    out.extend_from_slice(MAGIC);
    out.extend(VERSION.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend((recs.len() as u32).to_le_bytes());
    out.extend((HEADER_LEN as u32).to_le_bytes());
    out.extend((names_off as u32).to_le_bytes());
    out.extend((names.len() as u32).to_le_bytes());
    out.extend(((data_off + data.len()) as u32).to_le_bytes());
    out.extend(0u32.to_le_bytes()); // CRC, patched below
    for (r, (no, nl)) in recs.iter().zip(&name_at) {
        out.extend(no.to_le_bytes());
        out.extend(nl.to_le_bytes());
        out.push(r.kind);
        out.push(0);
        out.extend(r.parent.to_le_bytes());
        for v in [r.a, r.b, r.c, r.d] {
            out.extend(v.to_le_bytes());
        }
    }
    out.extend(&names);
    let crc = crc32(&out[HEADER_LEN..data_off]);
    out[28..32].copy_from_slice(&crc.to_le_bytes());
    out.extend(data);
    out
}

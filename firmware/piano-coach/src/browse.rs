//! The song library as a folder tree, whichever way a board stores it: the
//! microSD card, or (boards without one) the flash song pack shown as the
//! same kind of tree under [`PACK_ROOT`]. Paths here are library paths; the
//! player loads [`playable`] ones (`/pack/<index>` for pack songs).
//!
//! Also the moves the piano-key controls need (piano_core::controls):
//! next / previous song in a folder, next / previous folder that holds
//! songs (depth-first, library order, wrapping), a random song, the song a
//! board starts with, and shuffle-play of a folder ([`FolderShuffle`]).

// Each UI class uses its own subset (the full UI keeps its own library
// list and uses only the navigation part).
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use piano_core::controls::Shuffle;
use piano_core::engine::State;
use piano_core::library;
use piano_core::search::{Index, SongRef};
use piano_core::songpack::Pack;

/// Virtual root of the song pack's folder tree.
pub const PACK_ROOT: &str = "/songs";

/// Where the library lives.
#[cfg_attr(has_sd, allow(dead_code))] // card boards never build the pack
enum Source {
    Card,
    Pack(&'static Pack<'static>),
}

fn source() -> &'static Source {
    static SOURCE: OnceLock<Source> = OnceLock::new();
    SOURCE.get_or_init(|| {
        #[cfg(not(has_sd))]
        if let Ok(p) = crate::songs::open_pack() {
            return Source::Pack(p);
        }
        Source::Card
    })
}

/// The pack node at a path under [`PACK_ROOT`].
fn pack_node(pack: &Pack, path: &Path) -> Option<u32> {
    let rel = path.strip_prefix(PACK_ROOT).ok()?;
    let mut node = pack.root();
    for part in rel.components() {
        let name = part.as_os_str().to_str()?;
        if !node.is_dir() {
            return None;
        }
        node = node.children().map(|i| pack.node(i)).find(|n| n.name == name)?;
    }
    Some(node.index)
}

/// A song file (card) or song entry (pack) at `path`.
pub fn is_song(path: &Path) -> bool {
    match source() {
        Source::Card => path.is_file(),
        Source::Pack(p) => pack_node(p, path).is_some_and(|i| !p.node(i).is_dir()),
    }
}

/// What the player loads for the song at `path` (pack: `/pack/<index>`).
pub fn playable(path: &Path) -> PathBuf {
    match source() {
        Source::Card => path.to_path_buf(),
        #[cfg(not(has_sd))]
        Source::Pack(p) => pack_node(p, path).map(crate::songs::pack_path).unwrap_or_default(),
        #[cfg(has_sd)]
        Source::Pack(_) => PathBuf::new(),
    }
}

/// The search index over the whole library.
pub fn build_index(root: &Path) -> Index {
    match source() {
        Source::Card => Index::build(root),
        Source::Pack(p) => {
            let mut songs: Vec<SongRef> = p
                .songs()
                .into_iter()
                .map(|i| {
                    let path = Path::new(PACK_ROOT).join(p.path(i));
                    let folder = p.node(p.node(i).parent).name;
                    SongRef::new(path, library::entry(folder, true).title)
                })
                .filter(|s| s.path.starts_with(root))
                .collect();
            songs.sort_by(|a, b| library::natural_cmp(&a.title, &b.title));
            Index { songs }
        }
    }
}

/// The library root: a top-level `midi/` folder if there is one (as on the
/// full boards).
pub fn library_root() -> PathBuf {
    let mut root = match source() {
        Source::Pack(_) => PathBuf::from(PACK_ROOT),
        Source::Card => ["/sdcard/midi", "/sdcard/MIDI"]
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_dir())
            .unwrap_or_else(|| PathBuf::from("/sdcard")),
    };
    // A library that is one folder (e.g. "JustPiano2-20160326/") starts
    // inside it, so the web page opens on its categories.
    for _ in 0..4 {
        let (dirs, songs) = list_dir(&root);
        match (dirs.as_slice(), songs.is_empty()) {
            ([only], true) => root = root.join(only),
            _ => break,
        }
    }
    root
}

/// Sub-folders and songs (sorted) of a folder.
pub fn list_dir(dir: &Path) -> (Vec<String>, Vec<String>) {
    if let Source::Pack(p) = source() {
        // Pack folders are stored in library order already.
        let Some(node) = pack_node(p, dir).map(|i| p.node(i)).filter(|n| n.is_dir()) else {
            return Default::default();
        };
        let (mut dirs, mut songs) = (Vec::new(), Vec::new());
        for n in node.children().map(|i| p.node(i)) {
            if n.is_dir() { &mut dirs } else { &mut songs }.push(n.name.to_string());
        }
        return (dirs, songs);
    }
    let mut entries: Vec<library::Entry> = std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| library::entry(&e.file_name().to_string_lossy(), e.path().is_dir()))
                .collect()
        })
        .unwrap_or_default();
    entries = library::sort_entries(entries);
    let dirs = entries.iter().filter(|e| e.is_dir).map(|e| e.name.clone()).collect();
    let songs = entries.iter().filter(|e| !e.is_dir).map(|e| e.name.clone()).collect();
    (dirs, songs)
}


/// The library path of what the player loaded (inverse of [`playable`]).
pub fn from_playable(p: &Path) -> PathBuf {
    match (source(), crate::songs::pack_index(p)) {
        (Source::Pack(pack), Some(i)) if (i as usize) < pack.len() => Path::new(PACK_ROOT).join(pack.path(i)),
        _ => p.to_path_buf(),
    }
}

/// The pack entry of a library path (pack boards).
pub fn pack_entry(path: &Path) -> Option<u32> {
    match source() {
        Source::Pack(p) => pack_node(p, path).filter(|&i| !p.node(i).is_dir()),
        Source::Card => None,
    }
}

/// A uniformly random index below `n` (> 0).
fn random(n: usize) -> usize {
    (unsafe { esp_idf_svc::sys::esp_random() } as usize) % n
}

/// A random song of folder `dir`.
pub fn random_song(dir: &Path) -> Option<PathBuf> {
    let songs = list_dir(dir).1;
    (!songs.is_empty()).then(|| dir.join(&songs[random(songs.len())]))
}

/// The song `d` places from `song` in its folder (wrapping).
pub fn song_step(song: &Path, d: isize) -> Option<PathBuf> {
    let dir = song.parent()?;
    let songs = list_dir(dir).1;
    if songs.is_empty() {
        return None;
    }
    let name = song.file_name()?.to_str()?;
    let i = songs.iter().position(|n| n == name).unwrap_or(0) as isize;
    Some(dir.join(&songs[(i + d).rem_euclid(songs.len() as isize) as usize]))
}

/// The first (`d` > 0) or last folder in `dir`'s subtree, `dir` included,
/// that holds songs, in depth-first library order.
fn edge_folder(dir: &Path, d: isize, depth: u32) -> Option<PathBuf> {
    if depth > 8 {
        return None;
    }
    let (dirs, songs) = list_dir(dir);
    if d > 0 && !songs.is_empty() {
        return Some(dir.to_path_buf());
    }
    let found = if d > 0 {
        dirs.iter().find_map(|s| edge_folder(&dir.join(s), d, depth + 1))
    } else {
        dirs.iter().rev().find_map(|s| edge_folder(&dir.join(s), d, depth + 1))
    };
    found.or_else(|| (d < 0 && !songs.is_empty()).then(|| dir.to_path_buf()))
}

/// The next (`d` > 0) or previous folder after `dir` that holds songs, in
/// depth-first library order below `root`, wrapping around.
pub fn folder_step(root: &Path, dir: &Path, d: isize) -> Option<PathBuf> {
    let d = d.signum();
    if d > 0 {
        // A folder's own sub-folders come right after it.
        let subs = list_dir(dir).0;
        if let Some(f) = subs.iter().find_map(|s| edge_folder(&dir.join(s), 1, 1)) {
            return Some(f);
        }
    }
    let mut cur = dir.to_path_buf();
    while cur != root && cur.starts_with(root) {
        let parent = cur.parent()?.to_path_buf();
        let name = cur.file_name()?.to_string_lossy().into_owned();
        let (sibs, songs) = list_dir(&parent);
        let i = sibs.iter().position(|n| *n == name).unwrap_or(0);
        let next = if d > 0 {
            sibs[i + 1..].iter().find_map(|s| edge_folder(&parent.join(s), 1, 1))
        } else {
            sibs[..i].iter().rev().find_map(|s| edge_folder(&parent.join(s), -1, 1))
        };
        if next.is_some() {
            return next;
        }
        // Going back, the parent itself precedes its sub-folders.
        if d < 0 && !songs.is_empty() && parent != root {
            return Some(parent);
        }
        cur = parent;
    }
    edge_folder(root, d, 0)
}

/// A folder below `root` (breadth-first, a few levels) whose display title
/// is `title` (names like "儿时回忆（41）" carry a song count).
pub fn find_folder(root: &Path, title: &str) -> Option<PathBuf> {
    let mut level = vec![root.to_path_buf()];
    for _ in 0..4 {
        let mut next = Vec::new();
        for d in &level {
            for s in list_dir(d).0 {
                let p = d.join(&s);
                if library::entry(&s, true).title == title {
                    return Some(p);
                }
                next.push(p);
            }
        }
        level = next;
    }
    None
}

/// The folder every board starts in.
pub const START_FOLDER: &str = "儿时回忆";

/// The song a board starts with: a random one from [`START_FOLDER`], else
/// from the library's first folder of songs.
pub fn start_song(root: &Path) -> Option<PathBuf> {
    let dir = find_folder(root, START_FOLDER).or_else(|| edge_folder(root, 1, 0))?;
    random_song(&dir)
}

/// Pause after a song ends before the next shuffled one starts.
const SHUFFLE_GAP: Duration = Duration::from_millis(2500);

/// What shuffle-play wants after a [`FolderShuffle::tick`].
pub enum ShuffleEvent {
    Idle,
    /// Load and start this song (library path).
    Next(PathBuf),
    /// Every song of the folder has been played.
    Done,
    /// Another song was loaded meanwhile: shuffle-play is over.
    Overridden,
}

/// Shuffle-play of one folder: every song once, in random order, the next
/// one starting [`SHUFFLE_GAP`] after the last ended. The UI loads the
/// songs; this only decides which and when, watching the player.
pub struct FolderShuffle {
    pub dir: PathBuf,
    order: Shuffle,
    /// Playable path of the song asked for, and whether the player has it.
    expected: PathBuf,
    confirmed: bool,
    /// It has played since (a Finished before that is the previous song's).
    armed: bool,
    finished_at: Option<Instant>,
}

impl FolderShuffle {
    /// Shuffle `dir`; `current` (a library path) plays first if it is one
    /// of its songs. None if the folder has no songs.
    pub fn start(dir: &Path, current: Option<&Path>) -> Option<FolderShuffle> {
        let songs = list_dir(dir).1;
        if songs.is_empty() {
            return None;
        }
        let first = current
            .filter(|p| p.parent() == Some(dir))
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        let seed = unsafe { (esp_idf_svc::sys::esp_random() as u64) << 32 | esp_idf_svc::sys::esp_random() as u64 };
        let order = Shuffle::new(songs, first.as_deref(), seed);
        let cur = dir.join(order.current()?);
        Some(FolderShuffle {
            dir: dir.to_path_buf(),
            expected: playable(&cur),
            confirmed: false,
            armed: false,
            finished_at: None,
            order,
        })
    }

    /// The song shuffle-play is on (library path).
    pub fn current(&self) -> Option<PathBuf> {
        self.order.current().map(|n| self.dir.join(n))
    }

    /// (songs started so far, songs in the folder).
    pub fn progress(&self) -> (usize, usize) {
        (self.order.played(), self.order.len())
    }

    /// Skip `d` songs (B0 / A0); the song to load, None past the last.
    pub fn skip(&mut self, d: isize) -> Option<PathBuf> {
        let p = self.dir.join(self.order.step(d)?);
        self.expected = playable(&p);
        self.confirmed = false;
        self.armed = false;
        self.finished_at = None;
        Some(p)
    }

    /// Call every UI loop with the player's state and loaded (playable)
    /// path.
    pub fn tick(&mut self, state: State, loaded: Option<&Path>) -> ShuffleEvent {
        if loaded == Some(self.expected.as_path()) {
            self.confirmed = true;
        } else {
            return if self.confirmed { ShuffleEvent::Overridden } else { ShuffleEvent::Idle };
        }
        match state {
            State::Playing | State::Waiting => {
                self.armed = true;
                self.finished_at = None;
                ShuffleEvent::Idle
            }
            State::Finished if self.armed => {
                if self.finished_at.get_or_insert_with(Instant::now).elapsed() < SHUFFLE_GAP {
                    return ShuffleEvent::Idle;
                }
                match self.skip(1) {
                    Some(p) => ShuffleEvent::Next(p),
                    None => ShuffleEvent::Done,
                }
            }
            _ => ShuffleEvent::Idle,
        }
    }
}

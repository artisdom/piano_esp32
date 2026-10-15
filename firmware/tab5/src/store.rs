//! Bookmarks / scores / play history on the microSD card
//! (`/sdcard/.pianocoach/progress.tsv`, format in `piano_core::progress`).
//! Songs are keyed by their path relative to the library root, so the file
//! stays valid if the card is read on a PC.

use std::path::{Path, PathBuf};

use log::{info, warn};
use piano_core::progress::Progress;

const DIR: &str = "/sdcard/.pianocoach";
const FILE: &str = "/sdcard/.pianocoach/progress.tsv";

pub struct Store {
    pub progress: Progress,
    pub root: PathBuf,
}

impl Store {
    pub fn load(root: PathBuf) -> Store {
        let progress = match std::fs::read_to_string(FILE) {
            Ok(t) => {
                let p = Progress::parse(&t);
                info!("store: loaded {FILE} ({} bookmarks)", p.bookmarks().len());
                p
            }
            Err(_) => Progress::default(),
        };
        Store { progress, root }
    }

    /// Write via a temp file + rename so a power cut cannot leave half a file.
    pub fn save(&self) {
        let tmp = format!("{FILE}.tmp");
        let r = std::fs::create_dir_all(DIR)
            .and_then(|_| std::fs::write(&tmp, self.progress.to_text()))
            .and_then(|_| {
                let _ = std::fs::remove_file(FILE); // FAT rename does not overwrite
                std::fs::rename(&tmp, FILE)
            });
        if let Err(e) = r {
            warn!("store: cannot save {FILE}: {e}");
        }
    }

    pub fn rel(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned()
    }

    pub fn abs(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }
}

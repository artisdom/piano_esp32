//! Where song files come from: the microSD card (full boards) or the song
//! pack in the `songs` flash partition (boards without a card, see
//! `piano_core::songpack` and tools/songs.sh). Pack songs are addressed by
//! virtual paths `/pack/<index>`, so the player handles both alike.

use std::path::Path;
use std::sync::OnceLock;

use piano_core::songpack::Pack;

const PACK_PREFIX: &str = "/pack/";

static PACK: OnceLock<Option<Pack<'static>>> = OnceLock::new();

/// Map the `songs` partition and validate the pack (once).
#[cfg(not(has_sd))]
pub fn open_pack() -> Result<&'static Pack<'static>, String> {
    use esp_idf_svc::sys::bsp;
    let mut err = String::new();
    let pack = PACK.get_or_init(|| unsafe {
        // Found by name (its subtype is `undefined`, which espflash accepts).
        let part = bsp::esp_partition_find_first(
            bsp::esp_partition_type_t_ESP_PARTITION_TYPE_DATA,
            bsp::esp_partition_subtype_t_ESP_PARTITION_SUBTYPE_ANY,
            c"songs".as_ptr(),
        );
        if part.is_null() {
            err = "no `songs` partition".into();
            return None;
        }
        // Map only what the header says the pack occupies.
        let mut head = [0u8; 32];
        if bsp::esp_partition_read(part, 0, head.as_mut_ptr().cast(), head.len()) != 0 {
            err = "cannot read the songs partition".into();
            return None;
        }
        let size = (*part).size as usize;
        let total = u32::from_le_bytes([head[24], head[25], head[26], head[27]]) as usize;
        if &head[0..4] != piano_core::songpack::MAGIC {
            err = "no song pack in flash — run tools/songs.sh <board> flash".into();
            return None;
        }
        if total > size {
            err = format!("song pack ({total} B) is larger than its partition ({size} B)");
            return None;
        }
        let mut ptr: *const core::ffi::c_void = core::ptr::null();
        let mut handle: bsp::esp_partition_mmap_handle_t = 0;
        let r = bsp::esp_partition_mmap(
            part,
            0,
            total,
            bsp::esp_partition_mmap_memory_t_ESP_PARTITION_MMAP_DATA,
            &mut ptr,
            &mut handle,
        );
        if r != 0 {
            err = format!("cannot map the song pack (error {r})");
            return None;
        }
        let bytes: &'static [u8] = core::slice::from_raw_parts(ptr.cast(), total);
        match Pack::new(bytes) {
            Ok(p) => {
                log::info!("songs: pack of {} entries, {} songs, {} KB", p.len(), p.root().songs(), total / 1024);
                Some(p)
            }
            Err(e) => {
                err = e.to_string();
                None
            }
        }
    });
    pack.as_ref().ok_or_else(|| if err.is_empty() { "song pack unavailable".into() } else { err })
}

#[cfg(not(has_sd))]
pub fn pack_path(index: u32) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{PACK_PREFIX}{index}"))
}

/// Pack index of a `/pack/<index>` path.
pub fn pack_index(path: &Path) -> Option<u32> {
    path.to_str()?.strip_prefix(PACK_PREFIX)?.parse().ok()
}

/// The MIDI file at `path` (pack or file system).
pub fn read(path: &Path) -> Result<Vec<u8>, String> {
    if let Some(i) = pack_index(path) {
        let pack = PACK.get().and_then(Option::as_ref).ok_or("song pack not open")?;
        if i as usize >= pack.len() {
            return Err(format!("no pack entry {i}"));
        }
        return pack.midi(i).map_err(|e| e.to_string());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

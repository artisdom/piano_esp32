//! Heap a song needs on load: parse peak, the parsed song, and song + Learn
//! engine — measured with a counting allocator. Host numbers (64-bit) are
//! an upper bound for the 32-bit MCUs; used to choose the round board's
//! `--max-notes`.
//!
//!   cargo +stable run --release --example memprobe -- <file.mid>...

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
struct Count;
static CUR: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Count {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { let n = CUR.fetch_add(l.size(), Relaxed) + l.size(); PEAK.fetch_max(n, Relaxed); System.alloc(l) }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { CUR.fetch_sub(l.size(), Relaxed); System.dealloc(p, l) }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { if n > l.size() { let c = CUR.fetch_add(n - l.size(), Relaxed) + n - l.size(); PEAK.fetch_max(c + l.size(), Relaxed);} else { CUR.fetch_sub(l.size()-n, Relaxed);} System.realloc(p, l, n) }
}
#[global_allocator] static A: Count = Count;
fn main() {
    for f in std::env::args().skip(1) {
        let b = std::fs::read(&f).unwrap();
        let base = CUR.load(Relaxed); PEAK.store(base, Relaxed);
        let s = std::sync::Arc::new(piano_core::song::Song::parse(&b).unwrap());
        let parse_peak = PEAK.load(Relaxed) - base; let after_parse = CUR.load(Relaxed) - base;
        let e = piano_core::engine::Engine::new(s.clone(), piano_core::engine::Options { mode: piano_core::engine::Mode::Learn, ..Default::default() });
        println!("{f}: {} B midi, {} notes, parse peak {} KB, song {} KB, +engine total {} KB, peak {} KB",
            b.len(), s.notes.len(), parse_peak/1024, after_parse/1024, (CUR.load(Relaxed)-base)/1024, (PEAK.load(Relaxed)-base)/1024);
        drop(e);
    }
}

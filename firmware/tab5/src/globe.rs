//! Library "song globe" view: every song on the SD card is a dot on a
//! sphere (`piano_core::sphere`). This module ties it to the library: dot
//! order, categories, bookmarks, the selected song and touch handling.
//!
//! Drawing bypasses Slint: handing Slint a fresh 760×456 RGB image per frame
//! cost ~350 ms (our pixels + its convert/rotate). Instead the Slint page
//! leaves an empty panel and we paint the globe straight into the RGB565
//! shadow framebuffer after Slint has rendered: a cached background is
//! copied in row by row, then the dots are blended on top.

use std::path::{Path, PathBuf};
use std::time::Instant;

use piano_core::library;
use piano_core::search::Index;
use piano_core::sphere::{Canvas, Sphere, BACKGROUND, PALETTE};
use slint::platform::software_renderer::Rgb565Pixel;

use crate::board::{FRAME_HEIGHT, FRAME_WIDTH};
use crate::store::Store;

/// Must match `SphereGeom` in ui/library.slint.
pub const W: usize = 760;
pub const H: usize = 456;
/// Taps further than this from any dot select nothing.
const PICK_PX: f32 = 26.0;
/// A press that moved less than this (logical px) is a tap.
const TAP_SLOP: f32 = 12.0;

pub struct Globe {
    sphere: Sphere,
    /// Song file per dot (sphere order: category, then title).
    pub songs: Vec<PathBuf>,
    titles: Vec<String>,
    pub categories: Vec<(String, [u8; 3])>,
    first_of_cat: Vec<usize>,
    favs: Vec<bool>,
    pub selected: Option<usize>,
    dirty: bool,
    last: Option<(f32, f32, Instant)>,
    travel: f32,
    /// Logical (landscape) position of the panel on screen.
    origin: Option<(usize, usize)>,
    /// Origin just changed: Slint may still correct it once laid out (a
    /// re-created panel first reports a pre-layout position), so wait a frame.
    settle: bool,
    /// Where we last painted (for wiping it if the panel moves).
    painted_at: Option<(usize, usize)>,
    /// Background in framebuffer order: W logical columns × H pixels.
    bg: Vec<u16>,
    /// Squares painted last frame (erased from `bg` next frame).
    prev_rects: Vec<(i32, i32, usize)>,
    /// Restore the whole panel next paint (Slint drew over it).
    full_bg: bool,
    /// Paint profiling: order, background, dots (µs), frames.
    prof: (u64, u64, u64, u32),
}

fn rgb565(c: [u8; 3]) -> u16 {
    ((c[0] as u16 & 0xf8) << 8) | ((c[1] as u16 & 0xfc) << 3) | (c[2] as u16 >> 3)
}

/// Framebuffer index of logical (landscape) pixel (lx, ly). The UI renders
/// with a 270° rotation into the portrait panel: panel x = ly,
/// panel y = 1279 − lx (the same mapping as touch, board::poll_touch).
#[inline]
fn fb_index(lx: usize, ly: usize) -> usize {
    (FRAME_HEIGHT - 1 - lx) * FRAME_WIDTH + ly
}

/// Blends into the RGB565 shadow framebuffer at the panel's origin.
struct FbCanvas<'a> {
    fb: &'a mut [Rgb565Pixel],
    ox: usize,
    oy: usize,
}

impl Canvas for FbCanvas<'_> {
    #[inline]
    fn blend(&mut self, x: usize, y: usize, c: [u8; 3], a: f32) {
        let px = &mut self.fb[fb_index(self.ox + x, self.oy + y)].0;
        let (r, g, b) = ((*px >> 11) as f32, ((*px >> 5) & 0x3f) as f32, (*px & 0x1f) as f32);
        let mix = |old: f32, new: f32| old * (1.0 - a) + new * a;
        let r = mix(r, c[0] as f32 / 8.0) as u16;
        let g = mix(g, c[1] as f32 / 4.0) as u16;
        let b = mix(b, c[2] as f32 / 8.0) as u16;
        *px = (r.min(31) << 11) | (g.min(63) << 5) | b.min(31);
    }

    /// Integer RGB565 blend — the hot path for dot sprites (shifts, no
    /// divisions: those are slow on this core).
    #[inline]
    fn blend8(&mut self, x: usize, y: usize, c: [u8; 3], a: u8) {
        let px = &mut self.fb[fb_index(self.ox + x, self.oy + y)].0;
        let a = a as u32 + (a as u32 >> 7); // 0..=256
        let na = 256 - a;
        let o = *px as u32;
        let r = ((o >> 11) * na + (c[0] as u32 >> 3) * a) >> 8;
        let g = (((o >> 5) & 0x3f) * na + (c[1] as u32 >> 2) * a) >> 8;
        let b = ((o & 0x1f) * na + (c[2] as u32 >> 3) * a) >> 8;
        *px = ((r << 11) | (g << 5) | b) as u16;
    }
}

/// Builds the cached background (column-major = framebuffer row order).
struct BgCanvas<'a>(&'a mut [u16]);

impl Canvas for BgCanvas<'_> {
    fn blend(&mut self, x: usize, y: usize, c: [u8; 3], a: f32) {
        let px = &mut self.0[x * H + y];
        let old = *px;
        let (r, g, b) = ((old >> 11) as f32, ((old >> 5) & 0x3f) as f32, (old & 0x1f) as f32);
        let mix = |o: f32, n: f32| o * (1.0 - a) + n * a;
        *px = ((mix(r, c[0] as f32 / 8.0) as u16).min(31) << 11)
            | ((mix(g, c[1] as f32 / 4.0) as u16).min(63) << 5)
            | (mix(b, c[2] as f32 / 8.0) as u16).min(31);
    }
}

impl Globe {
    pub fn build(index: &Index, root: &Path, store: &Store) -> Globe {
        // Category = first folder below the folders every song shares.
        let dirs: Vec<Vec<String>> = index
            .songs
            .iter()
            .map(|s| {
                let rel = s.path.strip_prefix(root).unwrap_or(&s.path);
                let mut c: Vec<String> =
                    rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
                c.pop(); // file name
                c
            })
            .collect();
        let mut common = dirs.first().cloned().unwrap_or_default();
        for d in &dirs {
            let n = common.iter().zip(d).take_while(|(a, b)| a == b).count();
            common.truncate(n);
        }
        let cat_name = |d: &Vec<String>| {
            d.get(common.len())
                .map(|n| library::entry(n, true).title)
                .unwrap_or_else(|| "Other".into())
        };
        let mut names: Vec<String> = Vec::new();
        let mut cat_idx: Vec<usize> = Vec::new();
        for d in &dirs {
            let n = cat_name(d);
            let i = names.iter().position(|x| *x == n).unwrap_or_else(|| {
                names.push(n);
                names.len() - 1
            });
            cat_idx.push(i);
        }
        let mut order: Vec<usize> = (0..index.songs.len()).collect();
        order.sort_by(|&a, &b| {
            cat_idx[a]
                .cmp(&cat_idx[b])
                .then_with(|| library::natural_cmp(&index.songs[a].title, &index.songs[b].title))
        });
        let cats: Vec<u8> = order.iter().map(|&i| (cat_idx[i] % 256) as u8).collect();
        let mut first_of_cat = vec![0usize; names.len()];
        let mut count = vec![0usize; names.len()];
        for (pos, &c) in cats.iter().enumerate().rev() {
            first_of_cat[c as usize] = pos;
            count[c as usize] += 1;
        }
        // Middle of each band, so "turn to category" centres it.
        for (c, f) in first_of_cat.iter_mut().enumerate() {
            *f += count[c] / 2;
        }
        let songs: Vec<PathBuf> = order.iter().map(|&i| index.songs[i].path.clone()).collect();
        let titles = order
            .iter()
            .map(|&i| {
                let s = &index.songs[i];
                if s.artist.is_empty() {
                    s.title.clone()
                } else {
                    format!("{} · {}", s.title, s.artist)
                }
            })
            .collect();
        let categories =
            names.into_iter().enumerate().map(|(i, n)| (n, PALETTE[i % PALETTE.len()])).collect();
        let mut g = Globe {
            sphere: Sphere::new(cats),
            songs,
            titles,
            categories,
            first_of_cat,
            favs: Vec::new(),
            selected: None,
            dirty: true,
            last: None,
            travel: 0.0,
            origin: None,
            settle: false,
            painted_at: None,
            bg: Vec::new(),
            prof: (0, 0, 0, 0),
            prev_rects: Vec::new(),
            full_bg: true,
        };
        let mut bg = vec![rgb565(BACKGROUND); W * H];
        g.sphere.draw_background(&mut BgCanvas(&mut bg), W, H);
        g.bg = bg;
        g.refresh_favs(store);
        g
    }

    pub fn refresh_favs(&mut self, store: &Store) {
        self.favs = self.songs.iter().map(|p| store.progress.get(&store.rel(p)).bookmarked).collect();
        self.dirty = true;
    }

    pub fn select_path(&mut self, path: Option<&Path>) {
        self.selected = path.and_then(|p| self.songs.iter().position(|s| s == p));
        self.dirty = true;
    }

    /// Touch on the globe image (logical coordinates inside it). `gain` is
    /// the drag amplification applied before Slint sees moves. Returns the
    /// song tapped, if any.
    pub fn pointer(&mut self, kind: i32, x: f32, y: f32, gain: f32) -> Option<usize> {
        let now = Instant::now();
        match kind {
            0 => {
                self.last = Some((x, y, now));
                self.travel = 0.0;
                None
            }
            1 => {
                if let Some((lx, ly, lt)) = self.last {
                    let (dx, dy) = ((x - lx) / gain, (y - ly) / gain);
                    self.travel += (dx * dx + dy * dy).sqrt();
                    if self.travel > TAP_SLOP {
                        let dt = now.duration_since(lt).as_secs_f32() * 1000.0;
                        self.sphere.drag(dx, dy, dt.max(1.0));
                        self.dirty = true;
                    }
                    self.last = Some((x, y, now));
                }
                None
            }
            _ => {
                self.sphere.release();
                self.last = None;
                if self.travel <= TAP_SLOP {
                    let hit = self.sphere.pick(x, y, W, H, PICK_PX);
                    if hit.is_some() {
                        self.selected = hit;
                        self.dirty = true;
                    }
                    return hit;
                }
                None
            }
        }
    }

    /// Turn category `c`'s band towards the viewer.
    pub fn face_category(&mut self, c: usize) {
        if let Some(&i) = self.first_of_cat.get(c) {
            if i < self.songs.len() {
                self.sphere.face(i);
                self.dirty = true;
            }
        }
    }

    /// Where Slint placed the panel (logical px).
    pub fn set_origin(&mut self, x: f32, y: f32) {
        let o = Some((x.max(0.0) as usize, y.max(0.0) as usize));
        if o != self.origin {
            self.origin = o;
            self.settle = true;
            self.dirty = true;
        }
    }

    /// Advance the animation; true if the globe needs repainting.
    pub fn step(&mut self, dt_ms: f32) -> bool {
        if self.sphere.step(dt_ms) {
            self.dirty = true;
        }
        self.dirty
    }

    /// Framebuffer rows the panel covers, if placed.
    pub fn fb_rows(&self) -> Option<(usize, usize)> {
        let (ox, _) = self.origin?;
        Some((FRAME_HEIGHT - (ox + W).min(FRAME_HEIGHT), FRAME_HEIGHT - ox))
    }

    /// Paint into the shadow framebuffer; returns the rows written.
    pub fn paint(&mut self, fb: &mut [Rgb565Pixel]) -> Option<(usize, usize)> {
        let (ox, oy) = self.origin?;
        if std::mem::take(&mut self.settle) {
            return None;
        }
        if ox + W > FRAME_HEIGHT || oy + H > FRAME_WIDTH {
            return None;
        }
        let t0 = Instant::now();
        let order = self.sphere.depth_order(W, H);
        let t_order = t0.elapsed();
        // Erase last frame's dots from the cached background: whole panel
        // after Slint drew over it or it moved, otherwise just the squares the
        // previous dots and ring covered (most of the panel is unchanged
        // background; copying all 693 KB every frame cost ~25 ms).
        let tb = Instant::now();
        let copy_cols = |fb: &mut [Rgb565Pixel], bg: &[u16], x: usize, y0: usize, n: usize| {
            let row = fb_index(ox + x, oy + y0);
            // Rgb565Pixel is #[repr(transparent)] over u16: a plain memcpy.
            unsafe {
                core::ptr::copy_nonoverlapping(bg.as_ptr().add(x * H + y0), fb.as_mut_ptr().add(row) as *mut u16, n);
            }
        };
        if std::mem::take(&mut self.full_bg) || self.painted_at != self.origin {
            for x in 0..W {
                copy_cols(fb, &self.bg, x, 0, H);
            }
        } else {
            for &(x0, y0, size) in &self.prev_rects {
                let (xa, xb) = (x0.max(0) as usize, ((x0 + size as i32).max(0) as usize).min(W));
                let (ya, yb) = (y0.max(0) as usize, ((y0 + size as i32).max(0) as usize).min(H));
                if ya < yb {
                    for x in xa..xb {
                        copy_cols(fb, &self.bg, x, ya, yb - ya);
                    }
                }
            }
        }
        let t_bg = tb.elapsed();
        let td = Instant::now();
        let mut c = FbCanvas { fb: &mut *fb, ox, oy };
        self.sphere.draw_dots_in(&mut c, W, H, &order, &self.favs, self.selected, 0..W);
        let t_dots = td.elapsed();
        self.prev_rects.clear();
        for (i, p) in &order {
            self.prev_rects.push(self.sphere.dot_rect(*i, p, &self.favs));
        }
        if let Some(i) = self.selected {
            self.prev_rects.push(self.sphere.ring_rect(i, W, H));
        }
        self.prof.0 += t_order.as_micros() as u64;
        self.prof.1 += t_bg.as_micros() as u64;
        self.prof.2 += t_dots.as_micros() as u64;
        self.prof.3 += 1;
        if self.prof.3 == 30 {
            log::debug!(
                "globe paint: order {} µs · background {} µs · dots {} µs (avg of 30)",
                self.prof.0 / 30,
                self.prof.1 / 30,
                self.prof.2 / 30
            );
            self.prof = (0, 0, 0, 0);
        }
        self.dirty = false;
        self.painted_at = self.origin;
        self.fb_rows()
    }

    /// True once if dots were painted somewhere the panel no longer is —
    /// the caller must have Slint repaint everything to wipe them.
    pub fn take_stale_paint(&mut self) -> bool {
        if self.painted_at.is_some() && self.painted_at != self.origin {
            self.painted_at = None;
            return true;
        }
        false
    }

    /// Slint repainted (part of) the panel: restore all of it next paint.
    pub fn invalidate(&mut self) {
        self.full_bg = true;
        self.dirty = true;
    }

    /// The globe is being hidden: anything painted must be wiped.
    pub fn hide(&mut self) -> bool {
        self.dirty = true;
        self.full_bg = true;
        self.painted_at.take().is_some()
    }

    /// Title of the selected song, for the header.
    pub fn selected_title(&self) -> Option<&str> {
        self.selected.map(|i| self.titles[i].as_str())
    }

}

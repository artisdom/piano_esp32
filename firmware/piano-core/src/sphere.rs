//! "Song globe": every song is a dot on a sphere the user spins by finger.
//!
//! Dots are spread evenly with a Fibonacci lattice in library order (sorted
//! by category, then title), so each category forms a coloured band.
//! Rendering is plain pixel work into an RGB buffer — the firmware hands it
//! to Slint as an image — with perspective, depth-scaled and depth-shaded
//! dots, a highlight ring for the selected song and gold for bookmarks.

use std::f32::consts::PI;

/// Category colours (RGB), cycled.
pub const PALETTE: [[u8; 3]; 8] = [
    [0x34, 0xd3, 0xc2], // teal
    [0x9b, 0x87, 0xff], // violet
    [0xff, 0x8a, 0x4c], // orange
    [0x4d, 0xa3, 0xff], // sky
    [0xff, 0x5a, 0x9a], // pink
    [0xa3, 0xe6, 0x35], // lime
    [0xff, 0xd1, 0x66], // sand
    [0xe0, 0x6c, 0xff], // magenta
];
const GOLD: [u8; 3] = [0xff, 0xc8, 0x2e];
const BG: [u8; 3] = [0x16, 0x1b, 0x22];
const DISC: [u8; 3] = [0x1b, 0x22, 0x2c];

/// Something the globe can be drawn on (RGB888 image, rotated RGB565
/// framebuffer region, …). Coordinates are in the globe's own `w`×`h` space.
pub trait Canvas {
    /// Blend colour `c` over pixel (x, y) with coverage `a` (0..=1).
    fn blend(&mut self, x: usize, y: usize, c: [u8; 3], a: f32);
    /// Same with an 8-bit coverage; override with integer maths when hot.
    #[inline]
    fn blend8(&mut self, x: usize, y: usize, c: [u8; 3], a: u8) {
        self.blend(x, y, c, a as f32 / 255.0);
    }
}

/// Anti-aliased dot masks for radii 1.0, 1.25, … 7.0 px (centre on a pixel
/// centre). Dots are stamped from these instead of computing coverage per
/// pixel: on the ESP32-P4 that is the difference between ~85 ms and a few
/// ms per frame for 2449 dots.
struct Sprites {
    masks: Vec<(usize, Vec<u8>)>,
}

impl Sprites {
    const STEP: f32 = 0.25;
    const MIN: f32 = 1.0;
    const MAX: f32 = 7.0;

    fn new() -> Sprites {
        let n = ((Self::MAX - Self::MIN) / Self::STEP) as usize + 1;
        let masks = (0..n)
            .map(|k| {
                let r = Self::MIN + k as f32 * Self::STEP;
                let half = (r + 0.5).ceil() as usize;
                let size = 2 * half + 1;
                let c = half as f32 + 0.5;
                let mut m = vec![0u8; size * size];
                for y in 0..size {
                    for x in 0..size {
                        let d = ((x as f32 + 0.5 - c).powi(2) + (y as f32 + 0.5 - c).powi(2)).sqrt();
                        m[y * size + x] = ((r + 0.5 - d).clamp(0.0, 1.0) * 255.0) as u8;
                    }
                }
                (size, m)
            })
            .collect();
        Sprites { masks }
    }

    fn get(&self, r: f32) -> &(usize, Vec<u8>) {
        let k = ((r.clamp(Self::MIN, Self::MAX) - Self::MIN) / Self::STEP).round() as usize;
        &self.masks[k.min(self.masks.len() - 1)]
    }
}

/// Plain RGB888 buffer, row-major.
pub struct RgbCanvas<'a> {
    pub buf: &'a mut [u8],
    pub w: usize,
}

impl Canvas for RgbCanvas<'_> {
    #[inline]
    fn blend(&mut self, x: usize, y: usize, c: [u8; 3], a: f32) {
        let px = &mut self.buf[(y * self.w + x) * 3..][..3];
        for k in 0..3 {
            px[k] = (px[k] as f32 * (1.0 - a) + c[k] as f32 * a) as u8;
        }
    }
}

pub const BACKGROUND: [u8; 3] = BG;

pub struct Sphere {
    sprites: Sprites,
    pts: Vec<[f32; 3]>,
    cat: Vec<u8>,
    pub yaw: f32,
    pub pitch: f32,
    vel: f32,
    dragging: bool,
    idle_ms: f32,
}

/// A dot on screen.
#[derive(Clone, Copy, Debug)]
pub struct Projected {
    pub x: f32,
    pub y: f32,
    /// -1 (back) … 1 (front).
    pub z: f32,
    pub r: f32,
}

impl Sphere {
    /// `categories[i]` is song i's category index (songs already sorted).
    pub fn new(categories: Vec<u8>) -> Sphere {
        let n = categories.len().max(1);
        let golden = PI * (3.0 - 5f32.sqrt());
        let pts = (0..categories.len())
            .map(|i| {
                let y = 1.0 - 2.0 * (i as f32 + 0.5) / n as f32;
                let r = (1.0 - y * y).sqrt();
                let t = golden * i as f32;
                [r * t.cos(), y, r * t.sin()]
            })
            .collect();
        Sphere {
            sprites: Sprites::new(),
            pts,
            cat: categories,
            yaw: 0.0,
            pitch: 0.35,
            vel: 0.0,
            dragging: false,
            idle_ms: 0.0,
        }
    }

    pub fn len(&self) -> usize {
        self.pts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pts.is_empty()
    }

    fn trig(&self) -> [f32; 4] {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        [sy, cy, sp, cp]
    }

    #[inline]
    fn rotate_with([sy, cy, sp, cp]: [f32; 4], p: [f32; 3]) -> [f32; 3] {
        // Yaw about Y, then pitch about X.
        let x = p[0] * cy + p[2] * sy;
        let z = -p[0] * sy + p[2] * cy;
        let y = p[1] * cp - z * sp;
        let z = p[1] * sp + z * cp;
        [x, y, z]
    }

    pub fn project(&self, i: usize, w: usize, h: usize) -> Projected {
        self.project_with(self.trig(), i, w, h)
    }

    #[inline]
    fn project_with(&self, t: [f32; 4], i: usize, w: usize, h: usize) -> Projected {
        let [x, y, z] = Self::rotate_with(t, self.pts[i]);
        let radius = w.min(h) as f32 * 0.42;
        let s = 3.0 / (3.0 - z);
        Projected {
            x: w as f32 / 2.0 + radius * x * s,
            y: h as f32 / 2.0 - radius * y * s,
            z,
            r: 1.3 + 2.4 * (z + 1.0) / 2.0,
        }
    }

    /// Finger drag by (dx, dy) screen pixels.
    pub fn drag(&mut self, dx: f32, dy: f32, dt_ms: f32) {
        self.dragging = true;
        self.idle_ms = 0.0;
        let k = 0.006;
        self.yaw += dx * k;
        self.pitch = (self.pitch + dy * k).clamp(-1.3, 1.3);
        if dt_ms > 0.0 {
            self.vel = 0.7 * self.vel + 0.3 * (dx * k / dt_ms);
        }
    }

    pub fn release(&mut self) {
        self.dragging = false;
    }

    /// Advance inertia / idle spin; returns true if the view moved.
    pub fn step(&mut self, dt_ms: f32) -> bool {
        if self.dragging {
            return false;
        }
        self.idle_ms += dt_ms;
        if self.vel.abs() > 1e-5 {
            self.yaw += self.vel * dt_ms;
            self.vel *= 0.94f32.powf(dt_ms / 16.0);
            if self.vel.abs() < 2e-5 {
                self.vel = 0.0;
            }
            true
        } else if self.idle_ms > 4000.0 {
            // Gentle idle spin.
            self.yaw += 0.00012 * dt_ms;
            true
        } else {
            false
        }
    }

    /// Spin so that song `i` faces the viewer.
    pub fn face(&mut self, i: usize) {
        let [x, y, z] = self.pts[i];
        self.yaw = -x.atan2(z);
        self.pitch = y.asin().clamp(-1.3, 1.3);
        self.vel = 0.0;
        self.idle_ms = 0.0;
    }

    /// Nearest front-facing dot to (x, y) within `max_px`.
    pub fn pick(&self, x: f32, y: f32, w: usize, h: usize, max_px: f32) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for i in 0..self.pts.len() {
            let p = self.project(i, w, h);
            if p.z < -0.15 {
                continue;
            }
            let d = ((p.x - x).powi(2) + (p.y - y).powi(2)).sqrt() - p.z * 4.0;
            if d < max_px && best.is_none_or(|b| d < b.1) {
                best = Some((i, d));
            }
        }
        best.map(|b| b.0)
    }

    /// Draw into an RGB888 buffer `w`×`h` (background, then dots).
    pub fn render(&self, buf: &mut [u8], w: usize, h: usize, bookmarked: &[bool], selected: Option<usize>) {
        for px in buf.chunks_exact_mut(3) {
            px.copy_from_slice(&BG);
        }
        let mut c = RgbCanvas { buf, w };
        self.draw_background(&mut c, w, h);
        self.draw_dots(&mut c, w, h, bookmarked, selected);
    }

    /// The faint globe disc (draw on a canvas already filled with
    /// [`BACKGROUND`]); constant, so callers can cache it.
    pub fn draw_background(&self, c: &mut impl Canvas, w: usize, h: usize) {
        let radius = w.min(h) as f32 * 0.42 * 1.04;
        disc(c, w, h, w as f32 / 2.0, h as f32 / 2.0, radius, DISC, 1.0);
    }

    /// Dots back to front, then the selection ring.
    pub fn draw_dots(&self, c: &mut impl Canvas, w: usize, h: usize, bookmarked: &[bool], selected: Option<usize>) {
        let order = self.depth_order(w, h);
        self.draw_dots_in(c, w, h, &order, bookmarked, selected, 0..w);
    }

    /// All dots projected and sorted back to front (compute once per frame
    /// when drawing in tiles).
    pub fn depth_order(&self, w: usize, h: usize) -> Vec<(usize, Projected)> {
        let t = self.trig();
        let mut order: Vec<(usize, Projected)> = (0..self.pts.len()).map(|i| (i, self.project_with(t, i, w, h))).collect();
        order.sort_unstable_by(|a, b| a.1.z.partial_cmp(&b.1.z).unwrap());
        order
    }

    /// Pixel square `(x0, y0, size)` a dot covers when drawn (for erasing it
    /// next frame), clipped by the caller.
    pub fn dot_rect(&self, i: usize, p: &Projected, bookmarked: &[bool]) -> (i32, i32, usize) {
        let fav = bookmarked.get(i).copied().unwrap_or(false);
        let r = p.r + if fav { 1.2 } else { 0.0 };
        let size = self.sprites.get(r).0;
        let half = (size / 2) as i32;
        (p.x.floor() as i32 - half, p.y.floor() as i32 - half, size)
    }

    /// Square covered by the selection ring of song `i`.
    pub fn ring_rect(&self, i: usize, w: usize, h: usize) -> (i32, i32, usize) {
        let p = self.project(i, w, h);
        let o = (p.r + 7.0 + 2.5 + 2.0).ceil() as i32;
        (p.x as i32 - o, p.y as i32 - o, (2 * o + 1) as usize)
    }

    /// Draw only the columns `cols` (a tile), in depth order.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_dots_in(
        &self,
        c: &mut impl Canvas,
        w: usize,
        h: usize,
        order: &[(usize, Projected)],
        bookmarked: &[bool],
        selected: Option<usize>,
        cols: std::ops::Range<usize>,
    ) {
        let (cx0, cx1) = (cols.start as i32, cols.end.min(w) as i32);
        for &(i, p) in order {
            let fav = bookmarked.get(i).copied().unwrap_or(false);
            let r = p.r + if fav { 1.2 } else { 0.0 };
            let (size, mask) = self.sprites.get(r);
            let half = (size / 2) as i32;
            let (x0, y0) = (p.x.floor() as i32 - half, p.y.floor() as i32 - half);
            if x0 >= cx1 || x0 + *size as i32 <= cx0 {
                continue; // not in this tile
            }
            let base = if fav { GOLD } else { PALETTE[self.cat[i] as usize % PALETTE.len()] };
            let t = (p.z + 1.0) / 2.0;
            let light = 0.25 + 0.75 * t * t.sqrt(); // depth shading ≈ t^1.5
            let l = (light * 256.0) as u32;
            // Column-major stamping: on the Tab5 a logical column is one
            // framebuffer row, so this walks memory sequentially.
            for sx in 0..*size {
                let x = x0 + sx as i32;
                if x < cx0 || x >= cx1 {
                    continue;
                }
                for sy in 0..*size {
                    let y = y0 + sy as i32;
                    let m = mask[sy * size + sx];
                    if m == 0 || y < 0 || y >= h as i32 {
                        continue;
                    }
                    c.blend8(x as usize, y as usize, base, ((m as u32 * l) >> 8) as u8);
                }
            }
        }
        if let Some(i) = selected.filter(|&i| i < self.pts.len()) {
            let p = self.project(i, w, h);
            let o = p.r + 10.0;
            if p.x + o >= cx0 as f32 && p.x - o < cx1 as f32 {
                let mut clip = Clip { inner: c, cols: cx0 as usize..cx1 as usize };
                ring(&mut clip, w, h, p.x, p.y, p.r + 7.0, 2.5, [0xff, 0xff, 0xff]);
            }
        }
    }
}

/// Restricts drawing to a column range.
struct Clip<'a, C: Canvas> {
    inner: &'a mut C,
    cols: std::ops::Range<usize>,
}

impl<C: Canvas> Canvas for Clip<'_, C> {
    fn blend(&mut self, x: usize, y: usize, c: [u8; 3], a: f32) {
        if self.cols.contains(&x) {
            self.inner.blend(x, y, c, a);
        }
    }
}

/// Anti-aliased filled circle, colour `c` at opacity `alpha`. Square
/// roots only for edge pixels.
fn disc(cv: &mut impl Canvas, w: usize, h: usize, cx: f32, cy: f32, r: f32, c: [u8; 3], alpha: f32) {
    let (x0, x1) = ((cx - r - 1.0).max(0.0) as usize, ((cx + r + 1.0) as usize).min(w.saturating_sub(1)));
    let (y0, y1) = ((cy - r - 1.0).max(0.0) as usize, ((cy + r + 1.0) as usize).min(h.saturating_sub(1)));
    let inner = (r - 0.5).max(0.0).powi(2);
    let outer = (r + 0.5).powi(2);
    for y in y0..=y1 {
        let dy2 = (y as f32 + 0.5 - cy).powi(2);
        for x in x0..=x1 {
            let d2 = (x as f32 + 0.5 - cx).powi(2) + dy2;
            if d2 >= outer {
                continue;
            }
            let cov = if d2 <= inner { 1.0 } else { (r + 0.5 - d2.sqrt()).clamp(0.0, 1.0) };
            cv.blend(x, y, c, cov * alpha);
        }
    }
}

fn ring(cv: &mut impl Canvas, w: usize, h: usize, cx: f32, cy: f32, r: f32, t: f32, c: [u8; 3]) {
    let o = r + t;
    let (x0, x1) = ((cx - o - 1.0).max(0.0) as usize, ((cx + o + 1.0) as usize).min(w.saturating_sub(1)));
    let (y0, y1) = ((cy - o - 1.0).max(0.0) as usize, ((cy + o + 1.0) as usize).min(h.saturating_sub(1)));
    for y in y0..=y1 {
        for x in x0..=x1 {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let cov = (t / 2.0 + 0.5 - (d - r - t / 2.0).abs()).clamp(0.0, 1.0);
            if cov > 0.0 {
                cv.blend(x, y, c, cov);
            }
        }
    }
}

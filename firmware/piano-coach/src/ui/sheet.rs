//! Sheet music on the Play page: engraves the loaded song with
//! `piano_core::notation`, shows two systems (the one being played and the
//! next) as Slint primitive models, and places the play cursor. The page
//! turns — the next system moves up — when the cursor enters it.
//!
//! Shared with tests/ui_preview, which includes this file: it only needs
//! the generated Slint types from `super`.

use std::rc::Rc;

use piano_core::notation::{self, Params, Score, Shape};
use piano_core::song::Song;
use slint::VecModel;

use super::{PlayerVM, SheetDigits, SheetDot, SheetFlag, SheetGlyph, SheetRect, SheetTie};

/// Staff space (px): two grand-staff systems fit the roll + keyboard area
/// below a band for the prompt and score chip.
pub const SPACE: f32 = 9.0;
/// Sheet width (Geometry.keys-width) and inner margins.
const WIDTH: f32 = 1248.0;
const MARGIN_X: f32 = 18.0;
/// Top band kept free for the Learn prompt and the score chip.
const MARGIN_Y: f32 = 70.0;
// Glyph boxes for the images in ui/sheet/ (tools/make_music_glyphs.py).
include!("glyph_boxes.rs");

pub struct Sheet {
    rects: Rc<VecModel<SheetRect>>,
    glyphs: Rc<VecModel<SheetGlyph>>,
    dots: Rc<VecModel<SheetDot>>,
    flags: Rc<VecModel<SheetFlag>>,
    ties: Rc<VecModel<SheetTie>>,
    digits: Rc<VecModel<SheetDigits>>,
    /// The engraved song (kept: an address alone can be reused by the
    /// next song) and the first system shown.
    song: Option<std::sync::Arc<Song>>,
    score: Score,
    first: Option<usize>,
}

impl Sheet {
    pub fn new(vm: &PlayerVM) -> Sheet {
        let s = Sheet {
            rects: Rc::new(VecModel::default()),
            glyphs: Rc::new(VecModel::default()),
            dots: Rc::new(VecModel::default()),
            flags: Rc::new(VecModel::default()),
            ties: Rc::new(VecModel::default()),
            digits: Rc::new(VecModel::default()),
            song: None,
            score: Score::default(),
            first: None,
        };
        vm.set_sheet_space(SPACE);
        vm.set_sheet_rects(s.rects.clone().into());
        vm.set_sheet_glyphs(s.glyphs.clone().into());
        vm.set_sheet_dots(s.dots.clone().into());
        vm.set_sheet_flags(s.flags.clone().into());
        vm.set_sheet_ties(s.ties.clone().into());
        vm.set_sheet_digits(s.digits.clone().into());
        s
    }

    /// Follow the song at `ms` (engraving it first if it changed); `now`
    /// is the onset whose heads light up, `staves` which staves light.
    pub fn update(&mut self, vm: &PlayerVM, song: Option<&std::sync::Arc<Song>>, ms: u32, now: Option<u32>, staves: i32) {
        let Some(song) = song else {
            if self.song.is_some() {
                self.song = None;
                self.score = Score::default();
                self.first = None;
                self.clear();
            }
            vm.set_cursor_visible(false);
            vm.set_sheet_message("Pick a song in the Library to see its sheet music.".into());
            return;
        };
        if !self.song.as_ref().is_some_and(|s| std::sync::Arc::ptr_eq(s, song)) {
            let t = std::time::Instant::now();
            self.score = notation::engrave(
                song,
                Params {
                    width: WIDTH - 2.0 * MARGIN_X,
                    space: SPACE,
                },
            );
            log::info!(
                "sheet: {} systems, {} items in {:?}",
                self.score.systems.len(),
                self.score.systems.iter().map(|s| s.items.len()).sum::<usize>(),
                t.elapsed()
            );
            self.song = Some(song.clone());
            self.first = None;
            vm.set_sheet_message(if self.score.systems.is_empty() { "No notes to show." } else { "" }.into());
        }
        let Some((sys, x)) = self.score.locate(ms) else {
            vm.set_cursor_visible(false);
            return;
        };
        if self.first != Some(sys) {
            self.first = Some(sys);
            self.show(sys);
        }
        let s = &self.score.systems[sys];
        vm.set_cursor_x(MARGIN_X + x);
        vm.set_cursor_y(MARGIN_Y + s.top);
        vm.set_cursor_h(s.bottom - s.top);
        vm.set_cursor_visible(true);
        vm.set_sheet_now(now.map(|n| n as i32).unwrap_or(-1000));
        vm.set_sheet_staves(staves);
    }

    fn clear(&self) {
        self.rects.set_vec(Vec::new());
        self.glyphs.set_vec(Vec::new());
        self.dots.set_vec(Vec::new());
        self.flags.set_vec(Vec::new());
        self.ties.set_vec(Vec::new());
        self.digits.set_vec(Vec::new());
    }

    /// Fill the models with systems `first` and `first + 1`.
    fn show(&self, first: usize) {
        let (mut rects, mut glyphs, mut dots, mut flags, mut ties, mut digits) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let font = 4.0 * SPACE;
        let mut oy = MARGIN_Y;
        // Whole-pixel system offsets keep every system's lines identical.
        for sys in self.score.systems.iter().skip(first).take(2) {
            let ox = MARGIN_X;
            for it in &sys.items {
                match &it.shape {
                    // Snapped to whole pixels: hairlines at fractional
                    // positions would fade or vanish.
                    Shape::Rect { x, y, w, h } => {
                        let (x0, y0) = ((ox + x).round(), (oy + y).round());
                        rects.push(SheetRect {
                            x: x0,
                            y: y0,
                            w: ((ox + x + w).round() - x0).max(1.0),
                            h: ((oy + y + h).round() - y0).max(1.0),
                        })
                    }
                    Shape::Glyph { g, x, y } => {
                        // The image covers the glyph's box; (x, y) is its
                        // origin on the baseline.
                        let (x0, y0, x1, y1) = GLYPH_BOXES[g.index()];
                        let k = font / GLYPH_UPEM;
                        glyphs.push(SheetGlyph {
                        x: ox + x + x0 * k,
                        y: oy + y - y1 * k,
                        w: (x1 - x0) * k,
                        h: (y1 - y0) * k,
                        kind: g.index() as i32,
                        ms: it.note.map(|n| n.0 as i32).unwrap_or(-1),
                        staff: it.staff as i32,
                    })
                    }
                    Shape::Dot { x, y } => dots.push(SheetDot { x: ox + x, y: oy + y }),
                    Shape::Flag { x, y, up, n } => {
                        for k in 0..*n {
                            let d = k as f32 * 0.9 * SPACE;
                            flags.push(SheetFlag { x: ox + x, y: oy + if *up { y + d } else { y - d }, up: *up });
                        }
                    }
                    Shape::Tie { x0, x1, y, up } => ties.push(SheetTie { x: ox + x0, y: oy + y, w: x1 - x0, up: *up }),
                    Shape::Digits { text, x, y } => digits.push(SheetDigits {
                        x: ox + x,
                        y: oy + y,
                        text: text.as_str().into(),
                    }),
                }
            }
            oy += sys.height.round();
        }
        self.rects.set_vec(rects);
        self.glyphs.set_vec(glyphs);
        self.dots.set_vec(dots);
        self.flags.set_vec(flags);
        self.ties.set_vec(ties);
        self.digits.set_vec(digits);
    }
}

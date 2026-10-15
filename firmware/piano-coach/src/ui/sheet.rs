//! Sheet music on the Play page: engraves the loaded song with
//! `piano_core::notation` as large as the full-screen sheet allows, shows
//! two systems (the one being played and the next) as Slint primitive
//! models, and places the play cursor. The page turns — the next system
//! moves up — when the cursor enters it.
//!
//! Note heads can be coloured by pitch class (Boomwhackers or rainbow
//! colours, as in colour-coded beginner notation) and carry note names.
//!
//! Shared with tests/ui_preview, which includes this file: it only needs
//! the generated Slint types from `super`.

use std::rc::Rc;
use std::sync::Arc;

use piano_core::notation::{self, Glyph, Params, Score, Shape};
use piano_core::song::Song;
use slint::{Color, VecModel};

use super::{PlayerVM, SheetDigits, SheetDot, SheetFlag, SheetGlyph, SheetRect, SheetTie};

const MARGIN_X: f32 = 22.0;
const MARGIN_Y: f32 = 6.0;
/// A system's height in staff spaces (notation's grand staff + margins).
const SYSTEM_SPACES: f32 = 20.5;
// Glyph boxes for the images in ui/sheet/ (tools/make_music_glyphs.py).
include!("glyph_boxes.rs");

/// Pitch-class colours, C … B.
///
/// Boomwhackers: C red, D orange, E yellow, F lime, G green, A purple,
/// B magenta; the chromatic tubes sit between their neighbours. Yellow and
/// lime are a little darker than the tubes so they read on paper.
const BOOMWHACKERS: [u32; 12] = [
    0xE21C2B, 0xEE5A24, 0xF7941D, 0xF6B91D, 0xE3BE00, 0x8DBF22, 0x2EAA4A, 0x0B8A43, 0x0E9C9C, 0x6A3FA0, 0xA53C9E,
    0xD8287F,
];
/// Rainbow (Chroma-Notes style): C red, D orange, E yellow, F green,
/// G blue, A indigo, B violet.
const RAINBOW: [u32; 12] = [
    0xE53935, 0xF4511E, 0xFB8C00, 0xF6B400, 0xE0B400, 0x43A047, 0x00A39A, 0x1E88E5, 0x3D5AFE, 0x3F37C9, 0x7B3FBF,
    0x9C27B0,
];
const LETTERS: [&str; 7] = ["C", "D", "E", "F", "G", "A", "B"];
/// Fixed do (do = C), two letters so they fit a note head.
const SOLFEGE: [&str; 7] = ["Do", "Re", "Mi", "Fa", "So", "La", "Ti"];

fn rgb(c: u32) -> Color {
    Color::from_rgb_u8((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Light colours get dark names, dark ones white.
fn is_light(c: u32) -> bool {
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    (299 * r + 587 * g + 114 * b) / 1000 > 150
}

/// How the sheet is drawn: staff space and the colour / name options.
#[derive(Clone, Copy, PartialEq)]
struct Style {
    space: f32,
    width: f32,
    colors: i32,
    names: i32,
}

pub struct Sheet {
    rects: Rc<VecModel<SheetRect>>,
    glyphs: Rc<VecModel<SheetGlyph>>,
    dots: Rc<VecModel<SheetDot>>,
    flags: Rc<VecModel<SheetFlag>>,
    ties: Rc<VecModel<SheetTie>>,
    digits: Rc<VecModel<SheetDigits>>,
    /// The engraved song (kept: an address alone can be reused by the
    /// next song), its score, the style used, and the first system shown.
    song: Option<Arc<Song>>,
    score: Score,
    style: Option<Style>,
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
            style: None,
            first: None,
        };
        vm.set_sheet_rects(s.rects.clone().into());
        vm.set_sheet_glyphs(s.glyphs.clone().into());
        vm.set_sheet_dots(s.dots.clone().into());
        vm.set_sheet_flags(s.flags.clone().into());
        vm.set_sheet_ties(s.ties.clone().into());
        vm.set_sheet_digits(s.digits.clone().into());
        s
    }

    /// Follow the song at `ms` in a sheet area of `area` (w, h) logical
    /// pixels; `now` is the onset whose heads light up, `staves` which
    /// staves light (bit 0 treble, bit 1 bass). Colours and names come
    /// from the VM's settings.
    pub fn update(
        &mut self,
        vm: &PlayerVM,
        song: Option<&Arc<Song>>,
        ms: u32,
        now: Option<u32>,
        staves: i32,
        area: (f32, f32),
    ) {
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
        // Two systems fill the height.
        let space = ((area.1 - 2.0 * MARGIN_Y) / (2.0 * SYSTEM_SPACES)).floor().clamp(8.0, 20.0);
        let style = Style {
            space,
            width: area.0 - 2.0 * MARGIN_X,
            colors: vm.get_sheet_colors(),
            names: vm.get_sheet_names(),
        };
        let new_song = !self.song.as_ref().is_some_and(|s| Arc::ptr_eq(s, song));
        let relayout = self.style.is_none_or(|s| s.space != style.space || s.width != style.width);
        if new_song || relayout {
            let t = std::time::Instant::now();
            self.score = notation::engrave(song, Params { width: style.width, space });
            log::info!(
                "sheet: {} systems, {} items at {space} px staff space in {:?}",
                self.score.systems.len(),
                self.score.systems.iter().map(|s| s.items.len()).sum::<usize>(),
                t.elapsed()
            );
            self.song = Some(song.clone());
            self.first = None;
            vm.set_sheet_space(space);
            vm.set_sheet_message(if self.score.systems.is_empty() { "No notes to show." } else { "" }.into());
        }
        if self.style != Some(style) {
            self.style = Some(style);
            self.first = None; // redraw in the new colours / names
        }
        let Some((sys, x)) = self.score.locate(ms) else {
            vm.set_cursor_visible(false);
            return;
        };
        if self.first != Some(sys) {
            self.first = Some(sys);
            self.show(sys, song.meter.key, style);
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
    fn show(&self, first: usize, key_sig: i8, style: Style) {
        let space = style.space;
        let palette = match style.colors {
            1 => Some(&BOOMWHACKERS),
            2 => Some(&RAINBOW),
            _ => None,
        };
        let ink = 0x1D2128;
        let (mut rects, mut glyphs, mut dots, mut flags, mut ties, mut digits) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let font = 4.0 * space;
        // Whole-pixel system offsets keep every system's lines identical.
        let mut oy = MARGIN_Y;
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
                        let head = matches!(g, Glyph::HeadBlack | Glyph::HeadHalf | Glyph::HeadWhole);
                        let key = it.note.map(|n| n.1).filter(|_| head);
                        let fill = key.zip(palette).map(|(k, p)| p[k as usize % 12]);
                        let filled = *g == Glyph::HeadBlack;
                        let label = match (key, style.names) {
                            (Some(k), 1) => LETTERS[notation::spelling(k, key_sig).0 as usize],
                            (Some(k), 2) => SOLFEGE[notation::spelling(k, key_sig).0 as usize],
                            _ => "",
                        };
                        let label_color = match fill {
                            Some(c) if filled && !is_light(c) => 0xFFFFFF,
                            None if filled => 0xFFFFFF,
                            _ => ink,
                        };
                        glyphs.push(SheetGlyph {
                            x: ox + x + x0 * k,
                            y: oy + y - y1 * k,
                            w: (x1 - x0) * k,
                            h: (y1 - y0) * k,
                            kind: g.index() as i32,
                            ms: it.note.map(|n| n.0 as i32).unwrap_or(-1),
                            staff: it.staff as i32,
                            colored: fill.is_some(),
                            color: rgb(fill.unwrap_or(ink)),
                            outline: fill.is_some() && filled,
                            label: label.into(),
                            label_color: rgb(label_color),
                        })
                    }
                    Shape::Dot { x, y } => dots.push(SheetDot { x: ox + x, y: oy + y }),
                    Shape::Flag { x, y, up, n } => {
                        for k in 0..*n {
                            let d = k as f32 * 0.9 * space;
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

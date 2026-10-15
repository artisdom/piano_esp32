//! Sheet music from a [`Song`]: a grand staff (right hand on the treble
//! staff, left hand on the bass staff) engraved into lines of measures
//! ("systems") as simple drawing primitives the UI renders.
//!
//! Practical notation for a learner, not a full engraver:
//! * onsets and durations snap to a 16th-note grid (86 % of the bundled
//!   library's onsets are on it; triplets come out as the nearest 16ths);
//! * one voice per staff: a chord lasts until the staff's next onset;
//!   notes crossing a barline are split and tied; gaps become rests;
//! * accidentals follow the key signature and carry through the measure;
//!   black keys are spelled with sharps, or flats in flat keys;
//! * eighths and sixteenths are beamed per beat, lone ones get flags;
//! * horizontal spacing grows with duration, and every system is
//!   justified to the full width.
//!
//! Coordinates are pixels in the system's own frame; `Params::space` is
//! the staff space (distance between two staff lines).

use crate::song::{Hand, Song};

/// Grid: 16th notes.
const GRID: f64 = 4.0;

#[derive(Debug, Clone, Copy)]
pub struct Params {
    /// Width of a system (the score's drawing width).
    pub width: f32,
    /// Staff space in pixels.
    pub space: f32,
}

/// Music font glyphs (Unicode musical symbols, as in Noto Music). The
/// font's baseline is a staff's bottom line and a staff space is ¼ em.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    GClef,
    FClef,
    Sharp,
    Flat,
    Natural,
    HeadBlack,
    HeadHalf,
    HeadWhole,
    RestWhole,
    RestHalf,
    RestQuarter,
    Rest8,
    Rest16,
}

impl Glyph {
    /// Position in `ALL` (the UI's image table order).
    pub fn index(self) -> usize {
        Glyph::ALL.iter().position(|&g| g == self).unwrap_or(0)
    }
    pub const ALL: [Glyph; 13] = [
        Glyph::GClef,
        Glyph::FClef,
        Glyph::Sharp,
        Glyph::Flat,
        Glyph::Natural,
        Glyph::HeadBlack,
        Glyph::HeadHalf,
        Glyph::HeadWhole,
        Glyph::RestWhole,
        Glyph::RestHalf,
        Glyph::RestQuarter,
        Glyph::Rest8,
        Glyph::Rest16,
    ];
    pub fn char(self) -> char {
        match self {
            Glyph::GClef => '\u{1D11E}',
            Glyph::FClef => '\u{1D122}',
            Glyph::Sharp => '\u{266F}',
            Glyph::Flat => '\u{266D}',
            Glyph::Natural => '\u{266E}',
            Glyph::HeadBlack => '\u{1D158}',
            Glyph::HeadHalf => '\u{1D157}',
            Glyph::HeadWhole => '\u{1D15D}',
            Glyph::RestWhole => '\u{1D13B}',
            Glyph::RestHalf => '\u{1D13C}',
            Glyph::RestQuarter => '\u{1D13D}',
            Glyph::Rest8 => '\u{1D13E}',
            Glyph::Rest16 => '\u{1D13F}',
        }
    }
    /// Advance width in staff spaces.
    pub fn width(self) -> f32 {
        // Font units / 250 (one staff space).
        match self {
            Glyph::GClef => 2.84,
            Glyph::FClef => 3.16,
            Glyph::Sharp => 2.21,
            Glyph::Flat => 1.39,
            Glyph::Natural => 1.43,
            Glyph::HeadBlack | Glyph::HeadHalf => 1.59,
            Glyph::HeadWhole => 2.06,
            Glyph::RestWhole | Glyph::RestHalf => 3.13,
            Glyph::RestQuarter => 1.43,
            Glyph::Rest8 => 1.45,
            Glyph::Rest16 => 1.72,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    /// Filled rectangle: staff lines, stems, ledgers, barlines, beams.
    Rect { x: f32, y: f32, w: f32, h: f32 },
    /// A music glyph with its baseline-left origin at (x, y).
    Glyph { g: Glyph, x: f32, y: f32 },
    /// Augmentation dot centred at (x, y).
    Dot { x: f32, y: f32 },
    /// `n` flags hanging from a stem tip at (x, y).
    Flag { x: f32, y: f32, up: bool, n: u8 },
    /// Tie from x0 to x1 at height y, curving up or down.
    Tie { x0: f32, x1: f32, y: f32, up: bool },
    /// Time-signature digits centred at (x, y).
    Digits { text: String, x: f32, y: f32 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub shape: Shape,
    /// Note heads: the note's start (song ms) and key; tied continuations
    /// and everything else have none.
    pub note: Option<(u32, u8)>,
    /// Note heads: 0 right hand, 1 left hand. Otherwise the staff.
    pub staff: u8,
}

#[derive(Debug, Clone, Default)]
pub struct System {
    pub items: Vec<Item>,
    pub height: f32,
    /// Song ms → x anchors (ascending), for the play cursor.
    pub anchors: Vec<(u32, f32)>,
    /// Song ms span covered.
    pub ms0: u32,
    pub ms1: u32,
    /// Vertical extent of the staves (cursor), in system coordinates.
    pub top: f32,
    pub bottom: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Score {
    pub systems: Vec<System>,
}

impl Score {
    /// System index and x of the cursor at song time `ms`.
    pub fn locate(&self, ms: u32) -> Option<(usize, f32)> {
        if self.systems.is_empty() {
            return None;
        }
        let i = self.systems.partition_point(|s| s.ms0 <= ms).saturating_sub(1);
        let a = &self.systems[i].anchors;
        let j = a.partition_point(|p| p.0 <= ms);
        let x = match (j.checked_sub(1).map(|k| a[k]), a.get(j)) {
            (Some(p), Some(n)) if n.0 > p.0 => p.1 + (n.1 - p.1) * (ms - p.0) as f32 / (n.0 - p.0) as f32,
            (Some(p), _) => p.1,
            (None, Some(n)) => n.1,
            (None, None) => 0.0,
        };
        Some((i, x))
    }
}

// ---- musical model ------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Measure {
    q0: f64,
    len: f64,
    num: u8,
    den: u8,
    /// Time signature shown at its start.
    show_sig: bool,
}

/// A note value: base length in quarters (4, 2, 1, ½, ¼) and a dot.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Value {
    base: f64,
    dot: bool,
}

impl Value {
    fn len(self) -> f64 {
        if self.dot {
            self.base * 1.5
        } else {
            self.base
        }
    }
}

/// Split `len` quarters into note values, longest first.
fn values(mut len: f64) -> Vec<Value> {
    const V: [(f64, bool); 9] = [
        (4.0, false),
        (2.0, true),
        (2.0, false),
        (1.0, true),
        (1.0, false),
        (0.5, true),
        (0.5, false),
        (0.25, true),
        (0.25, false),
    ];
    let mut out = Vec::new();
    while len > 0.2 {
        let Some(&(base, dot)) = V.iter().find(|(b, d)| b * if *d { 1.5 } else { 1.0 } <= len + 1e-9) else { break };
        let v = Value { base, dot };
        out.push(v);
        len -= v.len();
    }
    out
}

#[derive(Debug, Clone)]
struct ChordNote {
    key: u8,
    ms: u32,
    /// 0 right hand, 1 left hand (highlighting follows the hand, even when
    /// the note sits on the other staff).
    hand: u8,
}

#[derive(Debug, Clone)]
enum Kind {
    Chord {
        notes: Vec<ChordNote>,
        /// Continues an earlier piece (no note identity, no accidental).
        tied_in: bool,
        tied_out: bool,
    },
    Rest {
        whole_measure: bool,
    },
}

#[derive(Debug, Clone)]
struct Event {
    /// Quarters from the measure start.
    off: f64,
    staff: u8,
    value: Value,
    kind: Kind,
}

fn quant(q: f64) -> f64 {
    (q * GRID).round() / GRID
}

/// Diatonic step (C4 = 0) and alteration of `key` in a key signature.
fn spell(key: u8, flats: bool) -> (i32, i8) {
    const SHARP: [(i32, i8); 12] =
        [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)];
    const FLAT: [(i32, i8); 12] =
        [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)];
    let (letter, alter) = if flats { FLAT } else { SHARP }[key as usize % 12];
    let octave = key as i32 / 12 - 1;
    ((octave - 4) * 7 + letter, alter)
}

/// Alteration the key signature gives a letter (0 = C … 6 = B).
fn key_alter(key: i8, letter: i32) -> i8 {
    const SHARPS: [i32; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B
    const FLATS: [i32; 7] = [6, 2, 5, 1, 4, 0, 3]; // B E A D G C F
    if key > 0 && SHARPS[..key as usize].contains(&letter) {
        1
    } else if key < 0 && FLATS[..(-key) as usize].contains(&letter) {
        -1
    } else {
        0
    }
}

fn measures(song: &Song, end_q: f64) -> Vec<Measure> {
    let sigs = if song.meter.time_sigs.is_empty() {
        vec![(0.0, 4, 4)]
    } else {
        song.meter.time_sigs.clone()
    };
    let mut out = Vec::new();
    let mut q = 0.0;
    let mut si = 0;
    let (mut num, mut den) = (4u8, 4u8);
    let mut shown = (0u8, 0u8);
    // A signature takes effect at the first barline at or after it.
    while q < end_q - 1e-9 && out.len() < 100_000 {
        while si < sigs.len() && sigs[si].0 <= q + 1e-6 {
            num = sigs[si].1;
            den = sigs[si].2;
            si += 1;
        }
        // On the 16th grid (x/32 signatures would put barlines between
        // grid points); the signature itself is shown as written.
        let len = ((num as f64 * 4.0 / den as f64) * GRID).round().max(1.0) / GRID;
        out.push(Measure {
            q0: q,
            len,
            num,
            den,
            show_sig: (num, den) != shown,
        });
        shown = (num, den);
        q += len;
    }
    out
}

/// Beat length (quarters) for beaming in a measure.
fn beat(m: &Measure) -> f64 {
    if m.den == 8 && m.num % 3 == 0 && m.num > 3 {
        1.5
    } else if m.den >= 8 {
        0.5 * 8.0 / m.den as f64 * 2.0
    } else {
        1.0
    }
}

/// Group the song into measure events per staff.
fn events(song: &Song, ms: &[Measure]) -> Vec<Vec<Event>> {
    let mut per_measure: Vec<Vec<Event>> = vec![Vec::new(); ms.len()];
    let measure_at = |q: f64| ms.partition_point(|m| m.q0 <= q + 1e-9).saturating_sub(1);
    // Each hand on its staff, unless a note is far outside it (ledger
    // lines past G3 below the treble / F4 above the bass): then it moves
    // to the other staff.
    let staff_of = |n: &crate::song::Note| -> u8 {
        let step = spell(n.key, song.meter.key < 0).0;
        match n.hand {
            Hand::Right if step < -3 => 1,
            Hand::Left if step > 3 => 0,
            Hand::Right => 0,
            Hand::Left => 1,
        }
    };
    for staff in 0..2u8 {
        // (onset, end, notes) per distinct onset.
        let mut chords: Vec<(f64, f64, Vec<ChordNote>)> = Vec::new();
        for n in song.notes.iter().filter(|n| staff_of(n) == staff) {
            let hand = if n.hand == Hand::Right { 0 } else { 1 };
            let on = quant(song.meter.q_at(n.start_ms as f64));
            let off = quant(song.meter.q_at(n.end_ms as f64)).max(on + 1.0 / GRID);
            match chords.iter_mut().rev().find(|c| (c.0 - on).abs() < 1e-9) {
                Some(c) => {
                    c.1 = c.1.max(off);
                    if !c.2.iter().any(|x| x.key == n.key) {
                        c.2.push(ChordNote { key: n.key, ms: n.start_ms, hand });
                    }
                }
                None => chords.push((on, off, vec![ChordNote { key: n.key, ms: n.start_ms, hand }])),
            }
        }
        chords.sort_by(|a, b| a.0.total_cmp(&b.0));
        // One voice: a chord ends where the next one starts.
        for i in 0..chords.len() {
            if let Some(next) = chords.get(i + 1).map(|c| c.0) {
                chords[i].1 = chords[i].1.min(next);
            }
            chords[i].2.sort_by_key(|n| n.key);
        }
        // Pieces within measures, tied across barlines.
        let mut busy: Vec<Vec<(f64, f64)>> = vec![Vec::new(); ms.len()];
        for (on, off, notes) in &chords {
            let mut q = *on;
            let mut first = true;
            while q < off - 1e-9 {
                let mi = measure_at(q);
                let m = &ms[mi];
                let end = off.min(m.q0 + m.len);
                if end <= q + 1e-9 {
                    break; // past the last barline (rounding): nothing left
                }
                let vals = values(end - q);
                let mut o = q - m.q0;
                for (k, v) in vals.iter().enumerate() {
                    let last_piece = end >= off - 1e-9 && k == vals.len() - 1;
                    per_measure[mi].push(Event {
                        off: o,
                        staff,
                        value: *v,
                        kind: Kind::Chord {
                            notes: notes.clone(),
                            tied_in: !first,
                            tied_out: !last_piece,
                        },
                    });
                    first = false;
                    o += v.len();
                }
                busy[mi].push((q - m.q0, end - m.q0));
                q = end;
            }
        }
        // Rests in the gaps.
        for (mi, m) in ms.iter().enumerate() {
            let mut b = busy[mi].clone();
            b.sort_by(|x, y| x.0.total_cmp(&y.0));
            if b.is_empty() {
                per_measure[mi].push(Event {
                    off: 0.0,
                    staff,
                    value: Value { base: m.len.min(4.0), dot: false },
                    kind: Kind::Rest { whole_measure: true },
                });
                continue;
            }
            let mut t = 0.0;
            for (s, e) in b.iter().copied().chain(std::iter::once((m.len, m.len))) {
                if s > t + 1e-9 {
                    let mut o = t;
                    for v in values(s - t) {
                        per_measure[mi].push(Event {
                            off: o,
                            staff,
                            value: v,
                            kind: Kind::Rest { whole_measure: false },
                        });
                        o += v.len();
                    }
                }
                t = t.max(e);
            }
        }
    }
    for e in &mut per_measure {
        e.sort_by(|a, b| a.off.total_cmp(&b.off).then(a.staff.cmp(&b.staff)));
    }
    per_measure
}

// ---- engraving ----------------------------------------------------------------

struct Geo {
    s: f32,
    /// Treble / bass top line y.
    yt: f32,
    yb: f32,
}

impl Geo {
    /// y of a diatonic step on a staff.
    fn y(&self, staff: u8, step: i32) -> f32 {
        if staff == 0 {
            // Top line F5 = step 10.
            self.yt + (10 - step) as f32 * self.s / 2.0
        } else {
            // Top line A3 = step -2.
            self.yb + (-2 - step) as f32 * self.s / 2.0
        }
    }
    fn middle(staff: u8) -> i32 {
        if staff == 0 {
            6
        } else {
            -6
        }
    }
    fn bottom_line(&self, staff: u8) -> f32 {
        if staff == 0 {
            self.yt + 4.0 * self.s
        } else {
            self.yb + 4.0 * self.s
        }
    }
}

/// Horizontal gap after an onset that lasts `dq` quarters (in spaces).
fn gap(dq: f64) -> f32 {
    2.6 + (dq.max(0.25) / 0.25).log2() as f32 * 1.1
}

struct MeasureLayout {
    /// Distinct onsets (quarters from the measure start) and their
    /// natural x (spaces, from the measure's left edge, before stretch).
    onsets: Vec<f64>,
    fixed: Vec<f32>,
    gaps: Vec<f32>,
    lead: f32,
}

impl MeasureLayout {
    fn new(m: &Measure, ev: &[Event], acc: &dyn Fn(&Event) -> bool) -> MeasureLayout {
        let mut onsets: Vec<f64> = ev.iter().map(|e| e.off).collect();
        onsets.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
        let mut fixed = Vec::new();
        let mut gaps = Vec::new();
        for (i, &o) in onsets.iter().enumerate() {
            let next = onsets.get(i + 1).copied().unwrap_or(m.len);
            let here: Vec<&Event> = ev.iter().filter(|e| (e.off - o).abs() < 1e-9).collect();
            let wide = here.iter().any(|e| matches!(e.value.base, b if b >= 4.0));
            let accidental = here.iter().any(|e| acc(e));
            let dotted = here.iter().any(|e| e.value.dot);
            fixed.push(if accidental { 1.6 } else { 0.0 });
            gaps.push(gap(next - o) + if wide { 0.6 } else { 0.0 } + if dotted { 0.6 } else { 0.0 });
        }
        MeasureLayout {
            onsets,
            fixed,
            gaps,
            lead: if m.show_sig { 1.2 + 2.6 } else { 1.2 },
        }
    }
    fn natural(&self) -> f32 {
        self.lead + self.fixed.iter().sum::<f32>() + self.gaps.iter().sum::<f32>()
    }
}

/// Engrave `song` into systems.
pub fn engrave(song: &Song, p: Params) -> Score {
    let s = p.space;
    let end_q = song
        .notes
        .iter()
        // As in `events`: every note lasts at least one grid step.
        .map(|n| quant(song.meter.q_at(n.end_ms as f64)).max(quant(song.meter.q_at(n.start_ms as f64)) + 1.0 / GRID))
        .fold(0.0, f64::max);
    if song.notes.is_empty() {
        return Score::default();
    }
    let ms = measures(song, end_q);
    let ev = events(song, &ms);
    let key = song.meter.key;
    let flats = key < 0;

    // Accidentals per measure (state per staff + step, reset each measure).
    let mut acc_of: Vec<Vec<Vec<Option<Glyph>>>> = Vec::new(); // [measure][event][note]
    for evs in &ev {
        let mut state: std::collections::HashMap<(u8, i32), i8> = Default::default();
        let mut per = Vec::new();
        for e in evs {
            let mut v = Vec::new();
            if let Kind::Chord { notes, tied_in, .. } = &e.kind {
                for n in notes {
                    let (step, alter) = spell(n.key, flats);
                    let cur = *state.get(&(e.staff, step)).unwrap_or(&key_alter(key, step.rem_euclid(7)));
                    if *tied_in || alter == cur {
                        v.push(None);
                    } else {
                        state.insert((e.staff, step), alter);
                        v.push(Some(match alter {
                            1 => Glyph::Sharp,
                            -1 => Glyph::Flat,
                            _ => Glyph::Natural,
                        }));
                    }
                }
            }
            per.push(v);
        }
        acc_of.push(per);
    }

    let layouts: Vec<MeasureLayout> = ms
        .iter()
        .enumerate()
        .map(|(mi, m)| {
            let evs = &ev[mi];
            let acc = &acc_of[mi];
            let has_acc = |e: &Event| {
                let i = evs.iter().position(|x| std::ptr::eq(x, e)).unwrap_or(0);
                acc.get(i).is_some_and(|a| a.iter().any(Option::is_some))
            };
            MeasureLayout::new(m, evs, &has_acc)
        })
        .collect();

    // Header: clefs + key signature.
    let header = 0.6 + 3.4 + key.unsigned_abs() as f32 * 1.0 + 0.8;
    let avail = p.width / s - header - 0.5;
    let mut systems_m: Vec<std::ops::Range<usize>> = Vec::new();
    let mut i = 0;
    while i < ms.len() {
        let mut w = 0.0;
        let mut j = i;
        while j < ms.len() && (j == i || w + layouts[j].natural() <= avail) {
            w += layouts[j].natural();
            j += 1;
        }
        systems_m.push(i..j);
        i = j;
    }

    // Room for ledger lines: 3.5 spaces above the treble staff, 6 between
    // the staves (middle C and its neighbours), 3 below the bass staff.
    let geo = Geo {
        s,
        yt: 3.5 * s,
        yb: 3.5 * s + 4.0 * s + 6.0 * s,
    };
    let height = geo.yb + 4.0 * s + 3.0 * s;
    let ms_of = |q: f64| song.meter.ms_at(q).max(0.0).round() as u32;

    let mut systems = Vec::new();
    for r in systems_m {
        let mut items_v: Vec<Item> = Vec::new();
        let items = &mut items_v;
        let line = 0.11 * s;
        // Staves.
        for staff in 0..2u8 {
            let y0 = if staff == 0 { geo.yt } else { geo.yb };
            for k in 0..5 {
                add(items, Shape::Rect { x: 0.0, y: y0 + k as f32 * s - line / 2.0, w: p.width, h: line }, None, staff);
            }
        }
        // System barline + clefs + key signature.
        let sys_top = geo.yt;
        let sys_bot = geo.yb + 4.0 * s;
        add(items, Shape::Rect { x: 0.0, y: sys_top, w: 0.3 * s, h: sys_bot - sys_top }, None, 0);
        add(items, Shape::Glyph { g: Glyph::GClef, x: 0.6 * s, y: geo.bottom_line(0) - 1.0 * s }, None, 0);
        add(items, Shape::Glyph { g: Glyph::FClef, x: 0.6 * s, y: geo.bottom_line(1) - 1.0 * s }, None, 1);
        let mut x = (0.6 + 3.4) * s;
        for k in 0..key.unsigned_abs() as usize {
            const SH: [i32; 7] = [10, 7, 11, 8, 5, 9, 6];
            const FL: [i32; 7] = [6, 9, 5, 8, 4, 7, 3];
            let st = if key > 0 { SH[k] } else { FL[k] };
            let g = if key > 0 { Glyph::Sharp } else { Glyph::Flat };
            for staff in 0..2u8 {
                let step = if staff == 0 { st } else { st - 14 };
                add(items, Shape::Glyph { g, x, y: acc_baseline(&geo, staff, step, g) }, None, staff);
            }
            x += 1.0 * s;
        }
        let x_start = header * s;

        // Justify: stretch only the gaps.
        let natural: f32 = r.clone().map(|mi| layouts[mi].natural()).sum();
        let gaps: f32 = r.clone().map(|mi| layouts[mi].gaps.iter().sum::<f32>()).sum();
        let last_system = r.end == ms.len();
        let room = (p.width / s - header - 0.5) - natural;
        // A short last system is not stretched beyond 1.6×.
        let k = if gaps > 0.0 {
            let k = 1.0 + room / gaps;
            if last_system {
                k.min(1.6)
            } else {
                k
            }
        } else {
            1.0
        };

        let mut anchors: Vec<(u32, f32)> = Vec::new();
        let mut mx = x_start;
        for mi in r.clone() {
            let m = &ms[mi];
            let lay = &layouts[mi];
            let evs = &ev[mi];
            let accs = &acc_of[mi];
            if m.show_sig {
                let tx = mx + 1.4 * s;
                for staff in 0..2u8 {
                    let top = if staff == 0 { geo.yt } else { geo.yb };
                    add(items, Shape::Digits { text: m.num.to_string(), x: tx, y: top + 1.0 * s }, None, staff);
                    add(items, Shape::Digits { text: m.den.to_string(), x: tx, y: top + 3.0 * s }, None, staff);
                }
            }
            // Onset x positions.
            let mut ox = Vec::new();
            let mut cx = mx + lay.lead * s;
            for (i, _) in lay.onsets.iter().enumerate() {
                cx += lay.fixed[i] * s;
                ox.push(cx);
                cx += lay.gaps[i] * k * s;
            }
            let m_end = cx;
            for (i, &o) in lay.onsets.iter().enumerate() {
                anchors.push((ms_of(m.q0 + o), ox[i]));
            }

            // Beam groups per staff: runs of flagged chords within a beat.
            let bt = beat(m);
            let mut beamed = vec![false; evs.len()];
            for staff in 0..2u8 {
                let mut groups: Vec<Vec<usize>> = Vec::new();
                let mut g: Vec<usize> = Vec::new();
                for i in (0..evs.len()).filter(|&i| evs[i].staff == staff) {
                    let e = &evs[i];
                    let flagged = matches!(e.kind, Kind::Chord { .. }) && e.value.base <= 0.5;
                    let same_beat = g.first().is_some_and(|&f| (evs[f].off / bt).floor() == (e.off / bt).floor());
                    if !(flagged && (g.is_empty() || same_beat)) {
                        groups.push(std::mem::take(&mut g));
                    }
                    if flagged {
                        g.push(i);
                    }
                }
                groups.push(g);
                for g in groups.iter().filter(|g| g.len() >= 2) {
                    draw_beamed(g, evs, &lay.onsets, &ox, &geo, flats, items, &mut beamed);
                }
            }

            for (i, e) in evs.iter().enumerate() {
                let x = ox[lay.onsets.iter().position(|o| (o - e.off).abs() < 1e-9).unwrap_or(0)];
                match &e.kind {
                    Kind::Rest { whole_measure } => {
                        let (g, xx) = if *whole_measure {
                            (Glyph::RestWhole, (mx + m_end) / 2.0 - 1.5 * s)
                        } else {
                            (rest_glyph(e.value.base), x)
                        };
                        let y = rest_baseline(&geo, e.staff, g);
                        add(items, Shape::Glyph { g, x: xx, y }, None, e.staff);
                        if e.value.dot && !whole_measure {
                            add(items, Shape::Dot { x: xx + g.width() * s + 0.6 * s, y: geo.y(e.staff, Geo::middle(e.staff) + 1) }, None, e.staff);
                        }
                    }
                    Kind::Chord { notes, tied_in, tied_out } => {
                        draw_chord(
                            items,
                            &geo,
                            e,
                            notes,
                            &accs[i],
                            x,
                            flats,
                            *tied_in,
                            *tied_out,
                            beamed[i],
                            ox.get(lay.onsets.iter().position(|o| (o - e.off).abs() < 1e-9).unwrap_or(0) + 1)
                                .copied()
                                .unwrap_or(m_end + lay.lead * s),
                        );
                    }
                }
            }
            // Barline at the measure's end.
            add(items, Shape::Rect { x: m_end - 0.06 * s, y: sys_top, w: 0.12 * s, h: sys_bot - sys_top }, None, 0);
            anchors.push((ms_of(m.q0 + m.len), m_end));
            mx = m_end;
        }
        let ms0 = ms_of(ms[r.start].q0);
        let ms1 = ms_of(ms[r.end - 1].q0 + ms[r.end - 1].len);
        anchors.sort_by_key(|a| a.0);
        anchors.dedup_by_key(|a| a.0);
        systems.push(System {
            items: items_v,
            height,
            anchors,
            ms0,
            ms1,
            top: sys_top - 1.5 * s,
            bottom: sys_bot + 1.5 * s,
        });
    }
    Score { systems }
}

fn add(items: &mut Vec<Item>, shape: Shape, note: Option<(u32, u8)>, staff: u8) {
    items.push(Item { shape, note, staff });
}

fn rest_glyph(base: f64) -> Glyph {
    match base {
        b if b >= 4.0 => Glyph::RestWhole,
        b if b >= 2.0 => Glyph::RestHalf,
        b if b >= 1.0 => Glyph::RestQuarter,
        b if b >= 0.5 => Glyph::Rest8,
        _ => Glyph::Rest16,
    }
}

/// Baseline for a rest glyph (positions follow the font's staff design:
/// baseline = bottom line).
fn rest_baseline(g: &Geo, staff: u8, glyph: Glyph) -> f32 {
    let bottom = g.bottom_line(staff);
    match glyph {
        // The whole rest hangs from the 4th line, the half sits on the 3rd.
        Glyph::RestWhole => bottom - 1.0 * g.s,
        Glyph::RestHalf => bottom - 2.0 * g.s,
        _ => bottom - 0.5 * g.s,
    }
}

/// Baseline for an accidental glyph so it centres on `step`.
fn acc_baseline(g: &Geo, staff: u8, step: i32, glyph: Glyph) -> f32 {
    let y = g.y(staff, step);
    match glyph {
        Glyph::Flat => y + 0.55 * g.s,
        Glyph::Sharp => y + 0.4 * g.s,
        _ => y + 0.4 * g.s,
    }
}

fn head_glyph(v: Value) -> Glyph {
    match v.base {
        b if b >= 4.0 => Glyph::HeadWhole,
        b if b >= 2.0 => Glyph::HeadHalf,
        _ => Glyph::HeadBlack,
    }
}

fn stem_up(staff: u8, steps: &[i32]) -> bool {
    let mid = Geo::middle(staff);
    let lo = steps.iter().min().copied().unwrap_or(mid);
    let hi = steps.iter().max().copied().unwrap_or(mid);
    // The note farther from the middle line decides.
    (mid - lo) >= (hi - mid)
}

/// Heads, ledgers, accidentals, dots, stem, flags and ties of one chord.
#[allow(clippy::too_many_arguments)]
fn draw_chord(
    items: &mut Vec<Item>,
    g: &Geo,
    e: &Event,
    notes: &[ChordNote],
    accs: &[Option<Glyph>],
    x: f32,
    flats: bool,
    tied_in: bool,
    tied_out: bool,
    beamed: bool,
    next_x: f32,
) {
    let s = g.s;
    let head = head_glyph(e.value);
    let hw = head.width() * s;
    let steps: Vec<i32> = notes.iter().map(|n| spell(n.key, flats).0).collect();
    let up = stem_up(e.staff, &steps);

    // Seconds: alternate heads to the other side of the stem.
    let mut dx = vec![0.0f32; notes.len()];
    for i in 1..steps.len() {
        if steps[i] - steps[i - 1] == 1 && dx[i - 1] == 0.0 {
            if up {
                dx[i] = hw - 0.1 * s;
            } else {
                dx[i - 1] = -(hw - 0.1 * s);
            }
        }
    }

    let ledger = 0.11 * s;
    for (i, n) in notes.iter().enumerate() {
        let step = steps[i];
        let y = g.y(e.staff, step);
        let hx = x + dx[i];
        // Ledger lines.
        let (top, bottom) = if e.staff == 0 { (10, 2) } else { (-2, -10) };
        let mut l = top + 2;
        while l <= step {
            add(items, Shape::Rect { x: hx - 0.4 * s, y: g.y(e.staff, l) - ledger / 2.0, w: hw + 0.8 * s, h: ledger }, None, e.staff);
            l += 2;
        }
        let mut l = bottom - 2;
        while l >= step {
            add(items, Shape::Rect { x: hx - 0.4 * s, y: g.y(e.staff, l) - ledger / 2.0, w: hw + 0.8 * s, h: ledger }, None, e.staff);
            l -= 2;
        }
        // Head: glyph spans 0..1.08 spaces above its baseline.
        add(
            items,
            Shape::Glyph { g: head, x: hx, y: y + 0.54 * s },
            if tied_in { None } else { Some((n.ms, n.key)) },
            n.hand,
        );
        if let Some(a) = accs.get(i).copied().flatten() {
            add(items, Shape::Glyph { g: a, x: x - a.width() * s - 0.3 * s + dx[i].min(0.0), y: acc_baseline(g, e.staff, step, a) }, None, e.staff);
        }
        if e.value.dot {
            // In the space above when the head is on a line.
            let dy = if step.rem_euclid(2) == 0 { -0.5 * s } else { 0.0 };
            add(items, Shape::Dot { x: x + hw + 0.5 * s + dx.iter().copied().fold(0.0, f32::max), y: y + dy }, None, e.staff);
        }
        if tied_out {
            let tx0 = hx + hw + 0.1 * s;
            let tx1 = (next_x - 0.1 * s).max(tx0 + 1.0 * s);
            let ty = if up { y + 0.9 * s } else { y - 0.9 * s };
            add(items, Shape::Tie { x0: tx0, x1: tx1, y: ty, up: !up }, None, e.staff);
        }
    }

    // Stem + flags (beamed chords get theirs from draw_beamed).
    if e.value.base < 4.0 && !beamed {
        let lo = g.y(e.staff, *steps.iter().min().unwrap());
        let hi = g.y(e.staff, *steps.iter().max().unwrap());
        let len = 3.5 * s;
        let sw = 0.13 * s;
        if up {
            let sx = x + hw - sw;
            add(items, Shape::Rect { x: sx, y: hi - len, w: sw, h: lo - (hi - len) }, None, e.staff);
            if e.value.base <= 0.5 {
                add(items, Shape::Flag { x: sx + sw, y: hi - len, up: true, n: if e.value.base <= 0.25 { 2 } else { 1 } }, None, e.staff);
            }
        } else {
            let sx = x;
            add(items, Shape::Rect { x: sx, y: hi, w: sw, h: lo + len - hi }, None, e.staff);
            if e.value.base <= 0.5 {
                add(items, Shape::Flag { x: sx, y: lo + len, up: false, n: if e.value.base <= 0.25 { 2 } else { 1 } }, None, e.staff);
            }
        }
    }
}

/// Stems and beams for a group of flagged chords (indices into `evs`).
#[allow(clippy::too_many_arguments)]
fn draw_beamed(
    group: &[usize],
    evs: &[Event],
    onsets: &[f64],
    ox: &[f32],
    g: &Geo,
    flats: bool,
    items: &mut Vec<Item>,
    beamed: &mut [bool],
) {
    let s = g.s;
    let staff = evs[group[0]].staff;
    let chord_steps = |i: usize| -> Vec<i32> {
        match &evs[i].kind {
            Kind::Chord { notes, .. } => notes.iter().map(|n| spell(n.key, flats).0).collect(),
            _ => vec![],
        }
    };
    let all: Vec<i32> = group.iter().flat_map(|&i| chord_steps(i)).collect();
    let up = stem_up(staff, &all);
    let hw = Glyph::HeadBlack.width() * s;
    let sw = 0.13 * s;
    let x_of = |i: usize| ox[onsets.iter().position(|o| (o - evs[i].off).abs() < 1e-9).unwrap_or(0)];
    // Horizontal beam beyond the most extreme head.
    let beam_y = if up {
        group.iter().map(|&i| g.y(staff, *chord_steps(i).iter().max().unwrap())).fold(f32::MAX, f32::min) - 3.2 * s
    } else {
        group.iter().map(|&i| g.y(staff, *chord_steps(i).iter().min().unwrap())).fold(f32::MIN, f32::max) + 3.2 * s
    };
    let bh = 0.5 * s;
    let stem_x = |i: usize| if up { x_of(i) + hw - sw } else { x_of(i) };
    for &i in group {
        let st = chord_steps(i);
        let lo = g.y(staff, *st.iter().min().unwrap());
        let hi = g.y(staff, *st.iter().max().unwrap());
        let sx = stem_x(i);
        if up {
            add(items, Shape::Rect { x: sx, y: beam_y, w: sw, h: lo - beam_y }, None, staff);
        } else {
            add(items, Shape::Rect { x: sx, y: hi, w: sw, h: beam_y - hi }, None, staff);
        }
        beamed[i] = true;
    }
    let (x0, x1) = (stem_x(group[0]), stem_x(*group.last().unwrap()) + sw);
    let by = if up { beam_y } else { beam_y - bh };
    add(items, Shape::Rect { x: x0, y: by, w: x1 - x0, h: bh }, None, staff);
    // Second beam for runs of 16ths; a lone 16th gets a stub.
    let by2 = if up { by + 0.75 * s } else { by - 0.75 * s };
    let mut k = 0;
    while k < group.len() {
        if evs[group[k]].value.base > 0.25 {
            k += 1;
            continue;
        }
        let mut j = k;
        while j + 1 < group.len() && evs[group[j + 1]].value.base <= 0.25 {
            j += 1;
        }
        let (a, b) = if j > k {
            (stem_x(group[k]), stem_x(group[j]) + sw)
        } else if k + 1 < group.len() {
            (stem_x(group[k]), stem_x(group[k]) + 1.2 * s)
        } else {
            (stem_x(group[k]) - 1.2 * s + sw, stem_x(group[k]) + sw)
        };
        add(items, Shape::Rect { x: a, y: by2, w: b - a, h: bh }, None, staff);
        k = j + 1;
    }
}

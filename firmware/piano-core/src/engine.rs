//! Play / learn engine as a pure state machine.
//!
//! The firmware feeds it the monotonic clock (`tick`), keyboard input
//! (`key_down` / `key_up`) and UI commands; it returns the MIDI messages to
//! send to the piano. Nothing here touches hardware, so the whole behaviour
//! is unit-tested on the host.
//!
//! Piano facts this relies on (doc/GZUT_piano_protocol.md):
//! * Note On lights a key's LED; velocity 1 lights it without sound.
//! * Note Off (or CC 123) turns LEDs off; host LEDs ignore physical keys.
//! * Everything is sent on MIDI channel 1.

use std::sync::Arc;

use crate::song::{Hand, HandSel, Note, Song, Step};

pub type Msg = [u8; 3];

pub const fn note_on(key: u8, vel: u8) -> Msg {
    [0x90, key, vel]
}
pub const fn note_off(key: u8) -> Msg {
    [0x80, key, 0]
}
pub const ALL_NOTES_OFF: Msg = [0xB0, 123, 0];
pub const SUSTAIN_OFF: Msg = [0xB0, 64, 0];

/// Lowest velocity the piano makes audible is between 8 and 16; keep played
/// notes clearly above it.
const MIN_AUDIBLE_VEL: u8 = 20;
/// Follow mode: a tracked hand counts as played by the learner from their
/// first hit on one of its notes until this long (playing time) without one;
/// meanwhile its notes sound softly as a guide, otherwise at full velocity.
pub const FOLLOW_IDLE_MS: u64 = 4000;
/// Follow mode guide: the learner's notes at this fraction of their
/// velocity, kept audible but well under what a played key sounds like.
const GUIDE_VEL_PCT: u32 = 40;
const GUIDE_VEL_MIN: u8 = 24;
const GUIDE_VEL_MAX: u8 = 48;
/// Follow mode: a press this early/late still counts as a hit.
const HIT_EARLY_MS: u32 = 250;
const HIT_LATE_MS: u32 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// The piano plays the song (sound + LEDs follow the notes).
    #[default]
    Listen,
    /// LEDs show the next chord; the song waits until it is played.
    Learn,
    /// Plays like Listen, with LEDs lit ahead of the tracked hands' notes;
    /// a tracked hand the learner is playing (hits within the last
    /// [`FOLLOW_IDLE_MS`]) sounds softly as a guide and is scored. The song
    /// does not wait.
    Follow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Options {
    pub mode: Mode,
    /// Learn: the hand(s) the learner plays. Follow: the hands tracked
    /// (lit ahead, played softly while the learner plays them, scored).
    /// Listen: hands played.
    pub hands: HandSel,
    /// 25..=200, percent of the file's tempo.
    pub tempo_pct: u16,
    /// Learn: the piano plays the other hand (Follow always does).
    pub accompany: bool,
    /// Learn: any key advances (young children).
    pub easy: bool,
    /// Learn: turn a hint LED off as soon as its key is pressed.
    pub led_off_on_hit: bool,
    /// Follow: how long before a note its LED comes on (song ms).
    pub lead_ms: u32,
    /// Velocity used for silent LED hints.
    pub hint_vel: u8,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            mode: Mode::Listen,
            hands: HandSel::Both,
            tempo_pct: 100,
            accompany: true,
            easy: false,
            led_off_on_hit: true,
            lead_ms: 700,
            hint_vel: 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Stopped,
    Playing,
    /// Learn mode: clock halted at a chord until the learner plays it.
    Waiting,
    Paused,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Stats {
    pub steps_done: u32,
    pub steps_total: u32,
    pub correct: u32,
    pub wrong: u32,
    /// Follow mode.
    pub hits: u32,
    pub misses: u32,
    pub streak: u32,
    pub best_streak: u32,
}

impl Stats {
    /// 0..=100 accuracy for the summary card.
    pub fn accuracy(&self) -> u32 {
        let (good, total) = if self.hits + self.misses > 0 {
            (self.hits, self.hits + self.misses + self.wrong)
        } else {
            (self.correct, self.correct + self.wrong)
        };
        if total == 0 {
            100
        } else {
            good * 100 / total
        }
    }
}

/// A timed output: notes sounding (`vel` > 0) or hint LEDs (`hint`).
#[derive(Debug, Clone, Copy)]
struct Timed {
    ms: u32,
    kind: TimedKind,
}

#[derive(Debug, Clone, Copy)]
enum TimedKind {
    On { key: u8, vel: u8 },
    Off { key: u8 },
    Pedal { cc: u8, val: u8 },
    /// Follow mode: light / clear a hint LED for note `idx`.
    HintOn { idx: u32 },
    HintOff { idx: u32 },
    /// Follow mode: sound / release target `idx` — softly while the
    /// learner plays its hand, else at full velocity; not at all if the
    /// learner already played it (early).
    GuideOn { idx: u32 },
    GuideOff { idx: u32 },
}

/// Follow-mode target.
#[derive(Debug, Clone, Copy)]
struct Target {
    ms: u32,
    key: u8,
    vel: u8,
    hand: Hand,
    hit: bool,
    /// Its note is sounding (GuideOn played it).
    guided: bool,
}

fn hand_ix(h: Hand) -> usize {
    match h {
        Hand::Right => 0,
        Hand::Left => 1,
    }
}

pub struct Engine {
    song: Arc<Song>,
    opts: Options,
    state: State,
    /// Song position in ms (tempo-scaled song time).
    pos: f64,
    last_now: Option<u64>,
    /// Wall-clock ms spent playing (Follow's idle timeout ignores pauses).
    played_ms: u64,
    /// Follow: `played_ms` of the last hit per hand (right, left).
    last_hit: [Option<u64>; 2],
    timeline: Vec<Timed>,
    cursor: usize,
    steps: Vec<Step>,
    step: usize,
    /// Keys of the current step already played.
    step_hit: Vec<u8>,
    targets: Vec<Target>,
    /// Ref-counts of audible notes and hint LEDs we turned on.
    sounding: [u8; 128],
    hints: [u8; 128],
    pressed: [bool; 128],
    /// UI flash for a wrong key: (key, song ms).
    pub last_wrong: Option<(u8, u32)>,
    stats: Stats,
    /// Practice section: song ms [start, end); playback jumps back to
    /// `start` on reaching `end`.
    loop_range: Option<(u32, u32)>,
    loops: u32,
}

/// Shortest loop accepted (song ms).
pub const MIN_LOOP_MS: u32 = 500;

impl Engine {
    pub fn new(song: Arc<Song>, opts: Options) -> Engine {
        let mut e = Engine {
            song,
            opts,
            state: State::Stopped,
            pos: 0.0,
            last_now: None,
            played_ms: 0,
            last_hit: [None; 2],
            timeline: Vec::new(),
            cursor: 0,
            steps: Vec::new(),
            step: 0,
            step_hit: Vec::new(),
            targets: Vec::new(),
            sounding: [0; 128],
            hints: [0; 128],
            pressed: [false; 128],
            last_wrong: None,
            stats: Stats::default(),
            loop_range: None,
            loops: 0,
        };
        e.rebuild();
        e
    }

    fn rebuild(&mut self) {
        let o = self.opts;
        let mut tl = Vec::with_capacity(self.song.notes.len() * 2 + self.song.pedals.len());
        let mut targets = Vec::new();
        let audible = |n: &Note| match o.mode {
            Mode::Listen => o.hands.includes(n.hand),
            Mode::Learn => o.accompany && !o.hands.includes(n.hand),
            // Tracked hands sound through GuideOn instead.
            Mode::Follow => !o.hands.includes(n.hand),
        };
        for n in self.song.notes.iter() {
            if audible(n) {
                tl.push(Timed {
                    ms: n.start_ms,
                    kind: TimedKind::On {
                        key: n.key,
                        vel: n.vel.max(MIN_AUDIBLE_VEL),
                    },
                });
                tl.push(Timed {
                    ms: n.end_ms,
                    kind: TimedKind::Off { key: n.key },
                });
            }
            if o.mode == Mode::Follow && o.hands.includes(n.hand) {
                let idx = targets.len() as u32;
                targets.push(Target {
                    ms: n.start_ms,
                    key: n.key,
                    vel: n.vel,
                    hand: n.hand,
                    hit: false,
                    guided: false,
                });
                tl.push(Timed {
                    ms: n.start_ms.saturating_sub(o.lead_ms),
                    kind: TimedKind::HintOn { idx },
                });
                tl.push(Timed {
                    ms: n.start_ms + HIT_LATE_MS,
                    kind: TimedKind::HintOff { idx },
                });
                // After the hint, so a note at the very start lights first.
                tl.push(Timed {
                    ms: n.start_ms,
                    kind: TimedKind::GuideOn { idx },
                });
                tl.push(Timed {
                    ms: n.end_ms,
                    kind: TimedKind::GuideOff { idx },
                });
            }
        }
        // The pedal belongs to whoever is playing the sound.
        if o.mode != Mode::Learn || o.accompany {
            for p in &self.song.pedals {
                tl.push(Timed {
                    ms: p.ms,
                    kind: TimedKind::Pedal { cc: p.cc, val: p.val },
                });
            }
        }
        // Stable sort: at equal times Offs (pushed after Ons of *earlier*
        // notes) must precede Ons of the next note on the same key.
        tl.sort_by_key(|t| {
            let order = match t.kind {
                TimedKind::Off { .. } | TimedKind::HintOff { .. } | TimedKind::GuideOff { .. } => 0,
                TimedKind::Pedal { .. } => 1,
                _ => 2,
            };
            (t.ms, order)
        });
        self.timeline = tl;
        self.targets = targets;
        self.steps = if o.mode == Mode::Learn {
            self.song.steps(o.hands)
        } else {
            Vec::new()
        };
        self.stats = Stats {
            steps_total: if o.mode == Mode::Learn {
                self.steps.len() as u32
            } else {
                self.targets.len() as u32
            },
            ..Stats::default()
        };
    }

    pub fn song(&self) -> &Arc<Song> {
        &self.song
    }
    pub fn options(&self) -> Options {
        self.opts
    }
    pub fn state(&self) -> State {
        self.state
    }
    pub fn stats(&self) -> Stats {
        self.stats
    }
    pub fn position_ms(&self) -> u32 {
        self.pos as u32
    }
    /// Keys whose LED we are holding on (hints or sounding notes).
    pub fn led(&self, key: u8) -> bool {
        self.hints[key as usize & 127] > 0 || self.sounding[key as usize & 127] > 0
    }
    pub fn is_hint(&self, key: u8) -> bool {
        self.hints[key as usize & 127] > 0
    }
    pub fn is_pressed(&self, key: u8) -> bool {
        self.pressed[key as usize & 127]
    }
    /// Learn mode: the chord being waited for (keys not yet played).
    pub fn waiting_keys(&self) -> Vec<u8> {
        match self.steps.get(self.step) {
            Some(s) => s.keys.iter().copied().filter(|k| !self.step_hit.contains(k)).collect(),
            None => Vec::new(),
        }
    }
    pub fn current_step(&self) -> Option<&Step> {
        self.steps.get(self.step)
    }

    /// Change options; restarts from the current position.
    pub fn set_options(&mut self, opts: Options, out: &mut Vec<Msg>) {
        let pos = self.pos as u32;
        let was = self.state;
        self.silence(out);
        self.opts = opts;
        self.rebuild();
        self.seek_inner(pos, out);
        self.state = match was {
            State::Finished => State::Stopped,
            State::Waiting => State::Playing,
            s => s,
        };
    }

    /// Set or clear the practice loop. A range shorter than [`MIN_LOOP_MS`]
    /// is rejected (returns false). If playing outside the new range, jumps
    /// to its start.
    pub fn set_loop(&mut self, range: Option<(u32, u32)>, out: &mut Vec<Msg>) -> bool {
        match range {
            None => {
                self.loop_range = None;
                self.loops = 0;
                true
            }
            Some((a, b)) => {
                let b = b.min(self.song.duration_ms);
                if b < a + MIN_LOOP_MS {
                    return false;
                }
                self.loop_range = Some((a, b));
                self.loops = 0;
                let pos = self.pos as u32;
                if pos < a || pos >= b {
                    self.silence(out);
                    self.seek_inner(a, out);
                    if self.state == State::Waiting || self.state == State::Finished {
                        self.state = State::Playing;
                    }
                }
                true
            }
        }
    }

    pub fn loop_range(&self) -> Option<(u32, u32)> {
        self.loop_range
    }

    /// Completed passes through the loop.
    pub fn loops(&self) -> u32 {
        self.loops
    }

    /// Only tempo changed: no restart needed.
    pub fn set_tempo(&mut self, pct: u16) {
        self.opts.tempo_pct = pct.clamp(25, 200);
    }

    pub fn play(&mut self, now: u64, out: &mut Vec<Msg>) {
        match self.state {
            State::Stopped | State::Finished => {
                self.silence(out);
                self.rebuild();
                self.loops = 0;
                self.last_hit = [None; 2];
                self.seek_inner(self.loop_range.map(|r| r.0).unwrap_or(0), out);
            }
            _ => {}
        }
        self.state = State::Playing;
        self.last_now = Some(now);
        self.tick(now, out);
    }

    pub fn pause(&mut self, out: &mut Vec<Msg>) {
        if matches!(self.state, State::Playing | State::Waiting) {
            // Release sounding notes; hint LEDs stay so the learner can see
            // where they were.
            self.release_sounding(out);
            self.state = State::Paused;
            self.last_now = None;
        }
    }

    pub fn stop(&mut self, out: &mut Vec<Msg>) {
        self.silence(out);
        self.state = State::Stopped;
        self.pos = 0.0;
        self.cursor = 0;
        self.step = 0;
        self.last_now = None;
    }

    /// Jump to `ms` (song time). Keeps playing if it was playing.
    pub fn seek(&mut self, ms: u32, out: &mut Vec<Msg>) {
        self.silence(out);
        self.seek_inner(ms.min(self.song.duration_ms), out);
        if self.state == State::Finished || self.state == State::Waiting {
            self.state = State::Playing;
        }
    }

    fn seek_inner(&mut self, ms: u32, out: &mut Vec<Msg>) {
        self.pos = ms as f64;
        self.cursor = self.timeline.partition_point(|t| t.ms < ms);
        self.step = self.steps.partition_point(|s| s.ms < ms);
        self.step_hit.clear();
        for t in self.targets.iter_mut() {
            t.hit = t.ms < ms;
            t.guided = false;
        }
        if self.opts.mode == Mode::Learn {
            self.light_step(out);
        }
    }

    fn silence(&mut self, out: &mut Vec<Msg>) {
        let any = self.sounding.iter().chain(self.hints.iter()).any(|&c| c > 0);
        self.sounding = [0; 128];
        self.hints = [0; 128];
        if any || self.state != State::Stopped {
            out.push(SUSTAIN_OFF);
            out.push(ALL_NOTES_OFF);
        }
    }

    fn release_sounding(&mut self, out: &mut Vec<Msg>) {
        for t in self.targets.iter_mut() {
            t.guided = false;
        }
        for k in 0..128u8 {
            if self.sounding[k as usize] > 0 {
                self.sounding[k as usize] = 0;
                if self.hints[k as usize] == 0 {
                    out.push(note_off(k));
                }
            }
        }
        out.push(SUSTAIN_OFF);
    }

    fn sound_on(&mut self, key: u8, vel: u8, out: &mut Vec<Msg>) {
        let k = key as usize;
        if self.sounding[k] > 0 {
            // Re-strike: the piano retriggers on a second Note On.
            out.push(note_off(key));
        }
        self.sounding[k] = self.sounding[k].saturating_add(1);
        out.push(note_on(key, vel));
    }

    fn sound_off(&mut self, key: u8, out: &mut Vec<Msg>) {
        let k = key as usize;
        if self.sounding[k] == 0 {
            return;
        }
        self.sounding[k] -= 1;
        if self.sounding[k] == 0 {
            out.push(note_off(key));
            // A hint on the same key was cleared by that Note Off: restore.
            if self.hints[k] > 0 {
                out.push(note_on(key, self.opts.hint_vel));
            }
        }
    }

    fn hint_on(&mut self, key: u8, out: &mut Vec<Msg>) {
        let k = key as usize;
        if self.hints[k] == 0 && self.sounding[k] == 0 {
            out.push(note_on(key, self.opts.hint_vel));
        }
        self.hints[k] = self.hints[k].saturating_add(1);
    }

    fn hint_off(&mut self, key: u8, out: &mut Vec<Msg>) {
        let k = key as usize;
        if self.hints[k] == 0 {
            return;
        }
        self.hints[k] -= 1;
        if self.hints[k] == 0 && self.sounding[k] == 0 {
            out.push(note_off(key));
        }
    }

    /// Learn mode: light the LEDs of the current step.
    fn light_step(&mut self, out: &mut Vec<Msg>) {
        if let Some(s) = self.steps.get(self.step) {
            // Not past the practice loop: that chord is never played.
            if self.loop_range.is_some_and(|(_, b)| s.ms >= b) {
                return;
            }
            let keys = s.keys.clone();
            for k in keys {
                self.hint_on(k, out);
            }
        }
    }

    fn finish_step(&mut self, out: &mut Vec<Msg>) {
        if let Some(s) = self.steps.get(self.step) {
            let keys = s.keys.clone();
            for k in keys {
                if !(self.opts.led_off_on_hit && self.step_hit.contains(&k)) {
                    self.hint_off(k, out);
                }
            }
        }
        self.stats.steps_done += 1;
        self.step += 1;
        self.step_hit.clear();
        self.light_step(out);
        if self.state == State::Waiting {
            self.state = State::Playing;
        }
    }

    /// Advance the clock and emit everything due.
    pub fn tick(&mut self, now: u64, out: &mut Vec<Msg>) {
        if self.state != State::Playing && self.state != State::Waiting {
            self.last_now = None;
            return;
        }
        let dt = self.last_now.map(|l| now.saturating_sub(l)).unwrap_or(0);
        self.last_now = Some(now);
        self.played_ms += dt;
        if self.state == State::Playing {
            self.pos += dt as f64 * self.opts.tempo_pct as f64 / 100.0;
        }
        // Practice loop: back to the start once the end is reached. Checked
        // before the Learn halt, so a chord at or past the end is never
        // waited for.
        if let Some((a, b)) = self.loop_range {
            if self.pos >= b as f64 {
                self.silence(out);
                self.seek_inner(a, out);
                self.loops += 1;
                self.state = State::Playing;
                return;
            }
        }
        // Learn: halt at the next unplayed chord.
        if self.opts.mode == Mode::Learn {
            if let Some(s) = self.steps.get(self.step) {
                if self.pos >= s.ms as f64 {
                    self.pos = s.ms as f64;
                    self.state = State::Waiting;
                }
            }
        }
        let pos = self.pos as u32;
        while let Some(&t) = self.timeline.get(self.cursor) {
            // While waiting, release notes ending exactly at the chord but do
            // not start anything at or past it.
            if t.ms > pos || (self.state == State::Waiting && t.ms == pos && !t.is_release()) {
                break;
            }
            self.cursor += 1;
            match t.kind {
                TimedKind::On { key, vel } => self.sound_on(key, vel, out),
                TimedKind::Off { key } => self.sound_off(key, out),
                TimedKind::Pedal { cc, val } => out.push([0xB0, cc, val]),
                TimedKind::HintOn { idx } => {
                    let tg = self.targets[idx as usize];
                    if !tg.hit {
                        self.hint_on(tg.key, out);
                    }
                }
                TimedKind::HintOff { idx } => {
                    let tg = self.targets[idx as usize];
                    if !tg.hit {
                        self.hint_off(tg.key, out);
                        // Only a hand the learner is playing can miss.
                        if self.hand_active(tg.hand) {
                            self.stats.misses += 1;
                            self.stats.streak = 0;
                        }
                    }
                }
                TimedKind::GuideOn { idx } => {
                    let tg = self.targets[idx as usize];
                    // Already played (early): the learner's own note sounds.
                    if !tg.hit {
                        let vel = if self.hand_active(tg.hand) {
                            ((tg.vel as u32 * GUIDE_VEL_PCT / 100) as u8).clamp(GUIDE_VEL_MIN, GUIDE_VEL_MAX)
                        } else {
                            tg.vel.max(MIN_AUDIBLE_VEL)
                        };
                        self.targets[idx as usize].guided = true;
                        self.sound_on(tg.key, vel, out);
                    }
                }
                TimedKind::GuideOff { idx } => {
                    let tg = &mut self.targets[idx as usize];
                    if tg.guided {
                        tg.guided = false;
                        let key = tg.key;
                        self.sound_off(key, out);
                    }
                }
            }
        }
        let done = self.cursor >= self.timeline.len()
            && self.step >= self.steps.len()
            && pos >= self.song.duration_ms;
        if done {
            self.silence(out);
            self.state = State::Finished;
        }
    }

    pub fn key_down(&mut self, key: u8, out: &mut Vec<Msg>) {
        self.pressed[key as usize & 127] = true;
        if self.state != State::Playing && self.state != State::Waiting {
            return;
        }
        match self.opts.mode {
            Mode::Listen => {}
            Mode::Learn => self.learn_press(key, out),
            Mode::Follow => self.follow_press(key, out),
        }
    }

    pub fn key_up(&mut self, key: u8) {
        self.pressed[key as usize & 127] = false;
    }

    fn learn_press(&mut self, key: u8, out: &mut Vec<Msg>) {
        let Some(step) = self.steps.get(self.step) else {
            return;
        };
        let wanted = step.keys.contains(&key);
        let need = step.keys.len();
        if self.opts.easy {
            self.stats.correct += 1;
            self.bump_streak();
            self.finish_step(out);
            return;
        }
        if wanted && !self.step_hit.contains(&key) {
            self.step_hit.push(key);
            self.stats.correct += 1;
            self.bump_streak();
            if self.opts.led_off_on_hit {
                self.hint_off(key, out);
            }
            if self.step_hit.len() >= need {
                self.finish_step(out);
            }
        } else if !wanted {
            self.stats.wrong += 1;
            self.stats.streak = 0;
            self.last_wrong = Some((key, self.pos as u32));
        }
    }

    fn follow_press(&mut self, key: u8, out: &mut Vec<Msg>) {
        let pos = self.pos as u32;
        let lo = pos.saturating_sub(HIT_LATE_MS);
        let hi = pos + HIT_EARLY_MS;
        let first = self.targets.partition_point(|t| t.ms < lo);
        let found = self.targets[first..]
            .iter()
            .take_while(|t| t.ms <= hi)
            .position(|t| t.key == key && !t.hit)
            .map(|i| i + first);
        match found {
            Some(i) => {
                self.targets[i].hit = true;
                self.last_hit[hand_ix(self.targets[i].hand)] = Some(self.played_ms);
                self.stats.hits += 1;
                self.bump_streak();
                // Its LED may already be lit (inside the lead window).
                if self.targets[i].ms.saturating_sub(self.opts.lead_ms) <= pos {
                    self.hint_off(key, out);
                }
            }
            // Keys played while just listening are not scored.
            None if self.hand_active(Hand::Right) || self.hand_active(Hand::Left) => {
                self.stats.wrong += 1;
                self.stats.streak = 0;
                self.last_wrong = Some((key, pos));
            }
            None => {}
        }
    }

    /// Follow: the learner is playing this hand (a hit on one of its notes
    /// within the last [`FOLLOW_IDLE_MS`] of playing time).
    fn hand_active(&self, h: Hand) -> bool {
        self.last_hit[hand_ix(h)].is_some_and(|t| self.played_ms.saturating_sub(t) < FOLLOW_IDLE_MS)
    }

    /// Follow: whether the learner is playing the right / left hand now.
    pub fn following(&self) -> (bool, bool) {
        (self.hand_active(Hand::Right), self.hand_active(Hand::Left))
    }

    fn bump_streak(&mut self) {
        self.stats.streak += 1;
        self.stats.best_streak = self.stats.best_streak.max(self.stats.streak);
    }
}

impl Timed {
    fn is_release(&self) -> bool {
        matches!(self.kind, TimedKind::Off { .. } | TimedKind::HintOff { .. } | TimedKind::GuideOff { .. })
    }
}

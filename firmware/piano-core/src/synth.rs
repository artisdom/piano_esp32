//! A small polyphonic piano synthesizer for the Tab5's speaker.
//!
//! Additive model of a struck string: each voice sums [`PARTIALS`] sine
//! partials at slightly stretched (inharmonic) frequencies, each with its own
//! exponential decay — upper partials die faster, bass strings ring longer —
//! so a note starts bright and mellows like a piano. Velocity sets loudness
//! and brightness; a few milliseconds of filtered noise add the hammer
//! knock. Note Off engages the damper (fast decay) unless the sustain pedal
//! is down.
//!
//! Cost matters on the ESP32-P4 (heap and code live in PSRAM): phases are
//! 32-bit integers that wrap for free, sine is a short polynomial (no table,
//! so no cache misses), partials and voices are dropped once inaudible.

pub const PARTIALS: usize = 6;
pub const MAX_VOICES: usize = 20;
/// Partial / voice amplitude below which it is dropped (≈ -80 dB).
const SILENT: f32 = 1e-4;
/// String stiffness: partial k sounds at k·f·sqrt(1 + B·k²).
const INHARMONICITY: f32 = 0.0004;
/// Damper time constant after Note Off (seconds).
const DAMPER_S: f32 = 0.12;
const ATTACK_S: f32 = 0.002;
const HAMMER_S: f32 = 0.006;

#[derive(Clone, Copy)]
struct Voice {
    key: u8,
    active: bool,
    /// Key released (or note off received).
    released: bool,
    /// Held only by the sustain pedal.
    sustained: bool,
    phase: [u32; PARTIALS],
    step: [u32; PARTIALS],
    amp: [f32; PARTIALS],
    decay: [f32; PARTIALS],
    attack: f32,
    attack_step: f32,
    damper: f32,
    damp_coef: f32,
    hammer_left: u32,
    hammer_amp: f32,
    noise_lp: f32,
    age: u64,
}

impl Voice {
    const IDLE: Voice = Voice {
        key: 0,
        active: false,
        released: false,
        sustained: false,
        phase: [0; PARTIALS],
        step: [0; PARTIALS],
        amp: [0.0; PARTIALS],
        decay: [1.0; PARTIALS],
        attack: 0.0,
        attack_step: 0.0,
        damper: 1.0,
        damp_coef: 1.0,
        hammer_left: 0,
        hammer_amp: 0.0,
        noise_lp: 0.0,
        age: 0,
    };

    fn level(&self) -> f32 {
        self.amp.iter().sum::<f32>() * self.damper
    }
}

/// sin(2π·phase/2³²), max error ≈ 0.001 (parabola + one refinement).
#[inline(always)]
fn sine(phase: u32) -> f32 {
    // x in [-1, 1) for one period.
    let x = (phase as i32) as f32 * (1.0 / 2_147_483_648.0);
    let y = 4.0 * x - 4.0 * x * x.abs();
    0.225 * (y * y.abs() - y) + y
}

pub struct Synth {
    rate: f32,
    voices: [Voice; MAX_VOICES],
    sustain: bool,
    clock: u64,
    noise: u32,
    /// Master gain (0..=1).
    pub volume: f32,
}

impl Synth {
    pub fn new(sample_rate: u32) -> Synth {
        Synth {
            rate: sample_rate as f32,
            voices: [Voice::IDLE; MAX_VOICES],
            sustain: false,
            clock: 0,
            noise: 0x1234_5678,
            volume: 0.8,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.rate as u32
    }

    /// Number of voices still sounding.
    pub fn active_voices(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }

    /// Handle one MIDI channel message (any channel).
    pub fn midi(&mut self, msg: &[u8]) {
        let Some(&s) = msg.first() else { return };
        match (s & 0xf0, msg.get(1).copied(), msg.get(2).copied()) {
            (0x90, Some(k), Some(v)) if v > 0 => self.note_on(k, v),
            (0x90, Some(k), _) | (0x80, Some(k), _) => self.note_off(k),
            (0xb0, Some(64), Some(v)) => self.set_sustain(v >= 64),
            (0xb0, Some(120 | 123), _) => self.all_off(),
            _ => {}
        }
    }

    pub fn note_on(&mut self, key: u8, vel: u8) {
        self.clock += 1;
        // Re-strike: reuse the voice already playing this key.
        let idx = self
            .voices
            .iter()
            .position(|v| v.active && v.key == key)
            .or_else(|| self.voices.iter().position(|v| !v.active))
            .unwrap_or_else(|| {
                // Steal the quietest voice, preferring released ones.
                (0..MAX_VOICES)
                    .min_by(|&a, &b| {
                        let (va, vb) = (&self.voices[a], &self.voices[b]);
                        (!va.released, va.level())
                            .partial_cmp(&(!vb.released, vb.level()))
                            .unwrap()
                    })
                    .unwrap()
            });
        let f = 440.0 * 2f32.powf((key as f32 - 69.0) / 12.0);
        let v = vel.clamp(1, 127) as f32 / 127.0;
        let loud = v.powf(1.7) * 0.32;
        // Brightness: soft notes lose upper partials faster.
        let bright = 0.35 + 0.5 * v;
        // Fundamental decay time constant: ~3 s at middle C (≈20 s to
        // -60 dB), longer in the bass, shorter in the treble.
        let t1 = (3.0 * (261.6 / f).powf(0.6)).clamp(0.6, 7.0);
        let nyquist = self.rate * 0.45;
        let mut voice = Voice::IDLE;
        for k in 0..PARTIALS {
            let n = (k + 1) as f32;
            let fk = f * n * (1.0 + INHARMONICITY * n * n).sqrt();
            if fk >= nyquist {
                continue; // amp stays 0: no aliasing
            }
            voice.step[k] = (fk / self.rate * 4_294_967_296.0) as u32;
            voice.phase[k] = (k as u32).wrapping_mul(0x5f00_0000);
            voice.amp[k] = loud * bright.powi(k as i32) / n.sqrt();
            let tk = t1 / (1.0 + 0.9 * k as f32);
            voice.decay[k] = (-1.0 / (tk * self.rate)).exp();
        }
        voice.key = key;
        voice.active = true;
        voice.attack_step = 1.0 / (ATTACK_S * self.rate);
        voice.damp_coef = (-1.0 / (DAMPER_S * self.rate)).exp();
        voice.hammer_left = (HAMMER_S * self.rate) as u32;
        voice.hammer_amp = loud * 0.25 * v;
        voice.age = self.clock;
        self.voices[idx] = voice;
    }

    pub fn note_off(&mut self, key: u8) {
        let sustain = self.sustain;
        for v in self.voices.iter_mut().filter(|v| v.active && v.key == key && !v.released) {
            v.released = true;
            v.sustained = sustain;
        }
    }

    pub fn set_sustain(&mut self, down: bool) {
        self.sustain = down;
        if !down {
            for v in self.voices.iter_mut() {
                v.sustained = false;
            }
        }
    }

    /// Immediate silence (CC 120/123).
    pub fn all_off(&mut self) {
        self.voices = [Voice::IDLE; MAX_VOICES];
        self.sustain = false;
    }

    /// Mix the next `out.len()` mono samples.
    pub fn render(&mut self, out: &mut [i16]) {
        let mut mix = [0f32; 256];
        for chunk in out.chunks_mut(256) {
            let n = chunk.len();
            mix[..n].fill(0.0);
            for v in self.voices.iter_mut().filter(|v| v.active) {
                let damping = v.released && !v.sustained;
                // Only the partials still audible.
                let live: usize = (0..PARTIALS).rev().find(|&k| v.amp[k] > SILENT).map_or(0, |k| k + 1);
                for s in mix[..n].iter_mut() {
                    let mut x = 0.0;
                    for k in 0..live {
                        x += sine(v.phase[k]) * v.amp[k];
                        v.phase[k] = v.phase[k].wrapping_add(v.step[k]);
                        v.amp[k] *= v.decay[k];
                    }
                    if v.hammer_left > 0 {
                        self.noise ^= self.noise << 13;
                        self.noise ^= self.noise >> 17;
                        self.noise ^= self.noise << 5;
                        let white = (self.noise as i32 as f32) / i32::MAX as f32;
                        v.noise_lp += 0.25 * (white - v.noise_lp);
                        x += v.noise_lp * v.hammer_amp;
                        v.hammer_left -= 1;
                    }
                    if v.attack < 1.0 {
                        v.attack = (v.attack + v.attack_step).min(1.0);
                    }
                    if damping {
                        v.damper *= v.damp_coef;
                    }
                    *s += x * v.attack * v.damper;
                }
                for a in v.amp.iter_mut() {
                    if *a < SILENT {
                        *a = 0.0; // inaudible: stop computing it
                    }
                }
                if v.level() < SILENT && v.hammer_left == 0 {
                    v.active = false;
                }
            }
            for (o, &s) in chunk.iter_mut().zip(mix[..n].iter()) {
                // Soft clip: gentle saturation instead of hard wrap-around.
                let y = s * self.volume;
                let y = y / (1.0 + y.abs());
                *o = (y * 30_000.0) as i16;
            }
        }
    }
}

//! The Synth: a polyphonic synthesizer that plays a MIDI track's notes from a
//! patch, `aaw_model::Synth`. Oscillators, a filter a voice, envelopes, LFOs,
//! a modulation matrix and macros, with automation of any numeric field.
//!
//! A voice is a function of the patch, its note and the frames since it
//! started: its oscillators and filter run every frame, and every control
//! value (the envelopes, the LFOs, the matrix, the filter's coefficients, the
//! pitch with its glide) is worked out every `CONTROL` frames of the voice's
//! own time, so the output does not depend on how the stream is cut into
//! blocks and a render is the same bytes twice. Random phases and the
//! `random` source come from the patch's seed, the track and the note's place.
//!
//! The state of every voice is allocated when the synth is built; rendering
//! never allocates. A synth takes over from the one playing the previous
//! revision when the notes are the same: its voices carry on and the patch's
//! values glide to their new ones.

use crate::envelope::Envelope;
use crate::Frame;
use aaw_model::rules::{mod_source, mod_target, ModSource, ModTarget};
use aaw_model::schedule::NoteOn;
use aaw_model::{LfoShape, Synth as Spec, SynthFilterMode, Wave};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Frames of a voice between control updates.
pub const CONTROL: usize = 16;
/// The most voices a patch may ask for; every synth allocates this many.
pub const MAX_VOICES: usize = 16;
const MAX_OSC: usize = 4;
const MAX_ENV: usize = 4;
const MAX_LFO: usize = 4;
const MAX_MACRO: usize = 8;

// The control values of a patch, flat, so that automation, the matrix and a
// glide between two patches address them by index.
const GLIDE: usize = 0;
const VELOCITY: usize = 1;
const OSC: usize = 2;
const OSC_FIELDS: usize = 5; // level_db, pan, semitones, detune_cents, pulse_width
const CUTOFF: usize = OSC + MAX_OSC * OSC_FIELDS;
const RESONANCE: usize = CUTOFF + 1;
const DRIVE: usize = CUTOFF + 2;
const KEYTRACK: usize = CUTOFF + 3;
const ENV: usize = CUTOFF + 4;
const ENV_FIELDS: usize = 4; // attack_ms, decay_ms, sustain_percent, release_ms
const LFO: usize = ENV + MAX_ENV * ENV_FIELDS;
const LFO_FIELDS: usize = 2; // rate_hz, phase_percent
const MACRO: usize = LFO + MAX_LFO * LFO_FIELDS;
const LEN: usize = MACRO + MAX_MACRO;

/// Fields that move in equal ratios: a glide between patches blends them in
/// the log domain, as a lane does.
fn is_log(index: usize) -> bool {
    index == CUTOFF || ((LFO..MACRO).contains(&index) && (index - LFO) % LFO_FIELDS == 0)
}

#[derive(Clone, Copy, Debug)]
struct Values([f64; LEN]);

/// `splitmix64`: a well-spread hash of a few numbers.
fn mix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

fn hash(parts: &[u64]) -> u64 {
    parts.iter().fold(0x5EED_5EED_5EED_5EED, |h, p| mix(h ^ p.wrapping_mul(0x9E37_79B9_7F4A_7C15)))
}

/// A hash as a number from 0 up to but not including 1.
fn unit(h: u64) -> f64 {
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// A track's part of every seed: a hash of its ID.
pub fn track_seed(id: &str) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    id.hash(&mut h);
    h.finish()
}

/// `10 ** (db / 20)`.
#[inline]
fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// Equal-power gains for a mono source at a pan position.
#[inline]
fn pan_gains(pan: f64) -> [f64; 2] {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    [angle.cos(), angle.sin()]
}

#[inline]
fn hz(pitch: f64) -> f64 {
    440.0 * 2f64.powf((pitch - 69.0) / 12.0)
}

// ---------------------------------------------------------------------------
// The compiled patch

#[derive(Clone, Debug)]
struct OscSpec {
    wave: Wave,
    octave: f64,
    /// Where each note's cycle starts, 0 to 1, or None for a random place.
    phase: Option<f64>,
    filtered: bool,
}

#[derive(Clone, Debug)]
struct LfoSpec {
    shape: LfoShape,
    /// Beats a cycle takes, when the rate is in beats; the rate in Hz is
    /// then worked out from the tempo and kept in the values.
    retrigger: bool,
}

#[derive(Clone, Copy, Debug)]
struct Route {
    source: ModSource,
    target: ModTarget,
    amount: f64,
}

/// A patch as the synth plays it: the spec's values flattened, the matrix
/// resolved, and the lanes that move its fields.
#[derive(Debug)]
pub struct Patch {
    statics: Values,
    oscillators: Vec<OscSpec>,
    filter_enabled: bool,
    filter_mode: SynthFilterMode,
    /// 1 for 12 dB an octave, 2 for 24.
    sections: usize,
    envelopes: usize,
    lfos: Vec<LfoSpec>,
    routes: Vec<Route>,
    voices: usize,
    seed: u64,
    rate: f64,
    /// Lanes on the patch's fields, by the index of the value they move.
    lanes: Vec<(usize, Arc<Envelope>)>,
    /// What decides whether a playing synth can take a new patch in its
    /// stride: the parts whose change would jump the waveform. Levels and
    /// knobs are not part of it.
    pub signature: u64,
}

impl Patch {
    /// `lanes` are the track's lanes on `instrument.FIELD`, by FIELD. `seed`
    /// is the track's part of every random value.
    pub fn new(spec: &Spec, lanes: &BTreeMap<String, Arc<Envelope>>, rate: f64, tempo: f64, seed: u64) -> Patch {
        let mut v = Values([0.0; LEN]);
        v.0[GLIDE] = spec.glide_ms;
        v.0[VELOCITY] = spec.velocity_percent;
        let mut oscillators = Vec::with_capacity(MAX_OSC);
        for (i, o) in spec.oscillators.values().enumerate().take(MAX_OSC) {
            let at = OSC + i * OSC_FIELDS;
            v.0[at] = o.level_db;
            v.0[at + 1] = o.pan;
            v.0[at + 2] = o.semitones;
            v.0[at + 3] = o.detune_cents;
            v.0[at + 4] = o.pulse_width;
            oscillators.push(OscSpec {
                wave: o.wave,
                octave: o.octave as f64,
                phase: o.phase.map(|p| p / 100.0),
                filtered: o.filter,
            });
        }
        v.0[CUTOFF] = spec.filter.cutoff_hz;
        v.0[RESONANCE] = spec.filter.resonance_percent;
        v.0[DRIVE] = spec.filter.drive_db;
        v.0[KEYTRACK] = spec.filter.keytrack_percent;
        for (i, e) in spec.envelopes.values().enumerate().take(MAX_ENV) {
            let at = ENV + i * ENV_FIELDS;
            v.0[at] = e.attack_ms;
            v.0[at + 1] = e.decay_ms;
            v.0[at + 2] = e.sustain_percent;
            v.0[at + 3] = e.release_ms;
        }
        let mut lfos = Vec::with_capacity(MAX_LFO);
        for (i, l) in spec.lfos.values().enumerate().take(MAX_LFO) {
            let at = LFO + i * LFO_FIELDS;
            v.0[at] = match l.beats_exact().and_then(|b| num_traits::ToPrimitive::to_f64(&b)) {
                Some(beats) if beats > 0.0 => tempo / 60.0 / beats,
                _ => l.rate_hz,
            };
            v.0[at + 1] = l.phase_percent;
            lfos.push(LfoSpec {
                shape: l.shape,
                retrigger: l.retrigger,
            });
        }
        for (i, m) in spec.macros.values().enumerate().take(MAX_MACRO) {
            v.0[MACRO + i] = *m;
        }
        let routes: Vec<Route> = spec
            .modulation
            .iter()
            .filter_map(|m| {
                Some(Route {
                    source: mod_source(spec, &m.source).ok()?,
                    target: mod_target(spec, &m.target).ok()?,
                    amount: m.amount,
                })
            })
            .collect();
        let mut lane_slots = Vec::new();
        for (path, env) in lanes {
            if let Some(index) = slot(spec, path) {
                lane_slots.push((index, env.clone()));
            }
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for o in &oscillators {
            (o.wave, o.filtered, o.phase.is_none()).hash(&mut h);
        }
        (spec.filter.enabled, spec.filter.mode, spec.filter.slope_db_per_octave).hash(&mut h);
        spec.envelopes.len().hash(&mut h);
        for l in &lfos {
            (l.shape, l.retrigger).hash(&mut h);
        }
        for r in &routes {
            (r.source, r.target).hash(&mut h);
        }
        for (index, env) in &lane_slots {
            (index, env.constant().is_none()).hash(&mut h);
        }
        Patch {
            statics: v,
            oscillators,
            filter_enabled: spec.filter.enabled,
            filter_mode: spec.filter.mode,
            sections: if spec.filter.slope_db_per_octave >= 24 { 2 } else { 1 },
            envelopes: spec.envelopes.len().min(MAX_ENV),
            lfos,
            routes,
            voices: (spec.voices.max(1) as usize).min(MAX_VOICES),
            seed: hash(&[spec.seed as u64, seed]),
            rate,
            lanes: lane_slots,
            signature: h.finish(),
        }
    }

    /// The patch's values at a timeline frame: its static ones, with the
    /// lanes' values where lanes move them.
    fn base(&self, frame: i64, out: &mut Values) {
        *out = self.statics;
        for (index, env) in &self.lanes {
            out.0[*index] = env.at(frame);
        }
    }

    /// A free-running LFO's phase at a timeline frame, worked out from its
    /// static rate, for a locate.
    fn lfo_phase_at(&self, i: usize, frame: i64) -> f64 {
        let rate = self.statics.0[LFO + i * LFO_FIELDS];
        let phase0 = self.statics.0[LFO + i * LFO_FIELDS + 1] / 100.0;
        phase0 + frame.max(0) as f64 * rate / self.rate
    }
}

/// The index of a field's value by its path in the patch, or None for a
/// field that is not a number the synth reads from its values.
fn slot(spec: &Spec, path: &str) -> Option<usize> {
    let parts: Vec<&str> = path.split('.').collect();
    Some(match parts.as_slice() {
        ["glide_ms"] => GLIDE,
        ["velocity_percent"] => VELOCITY,
        ["oscillators", id, field] => {
            let i = spec.oscillators.get_index_of(*id).filter(|i| *i < MAX_OSC)?;
            OSC + i * OSC_FIELDS
                + match *field {
                    "level_db" => 0,
                    "pan" => 1,
                    "semitones" => 2,
                    "detune_cents" => 3,
                    "pulse_width" => 4,
                    _ => return None,
                }
        }
        ["filter", "cutoff_hz"] => CUTOFF,
        ["filter", "resonance_percent"] => RESONANCE,
        ["filter", "drive_db"] => DRIVE,
        ["filter", "keytrack_percent"] => KEYTRACK,
        ["envelopes", id, field] => {
            let i = spec.envelopes.get_index_of(*id).filter(|i| *i < MAX_ENV)?;
            ENV + i * ENV_FIELDS
                + match *field {
                    "attack_ms" => 0,
                    "decay_ms" => 1,
                    "sustain_percent" => 2,
                    "release_ms" => 3,
                    _ => return None,
                }
        }
        ["lfos", id, field] => {
            let i = spec.lfos.get_index_of(*id).filter(|i| *i < MAX_LFO)?;
            LFO + i * LFO_FIELDS
                + match *field {
                    "rate_hz" => 0,
                    "phase_percent" => 1,
                    _ => return None,
                }
        }
        ["macros", name] => MACRO + spec.macros.get_index_of(*name).filter(|i| *i < MAX_MACRO)?,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Envelopes, oscillators, the filter

/// An envelope's times in frames and its sustain level, as a voice holds
/// them from its start.
#[derive(Clone, Copy, Debug, Default)]
struct Env {
    attack: f64,
    decay: f64,
    sustain: f64,
    release: f64,
}

/// How far an exponential fall has gone at `x` of its time: from 1 to 0
/// exactly, 60 dB down at the end.
#[inline]
fn fall(x: f64) -> f64 {
    const LN_1000: f64 = 6.907_755_278_982_137;
    ((-x * LN_1000).exp() - 0.001) / 0.999
}

impl Env {
    /// The level while the note is held, `t` frames in.
    #[inline]
    fn held(&self, t: f64) -> f64 {
        if t < self.attack {
            t / self.attack
        } else if t < self.attack + self.decay {
            self.sustain + (1.0 - self.sustain) * fall((t - self.attack) / self.decay)
        } else {
            self.sustain
        }
    }

    /// The level `t` frames after the note started, with the note-off `off`
    /// frames after it.
    #[inline]
    fn level(&self, t: f64, off: f64) -> f64 {
        if t < off {
            return self.held(t);
        }
        let x = (t - off) / self.release;
        if !(x < 1.0) {
            return 0.0;
        }
        self.held(off) * fall(x)
    }

    /// Whether the envelope has fallen silent `t` frames in.
    #[inline]
    fn done(&self, t: f64, off: f64) -> bool {
        t >= off + self.release
    }
}

#[inline]
fn polyblep(t: f64, dt: f64) -> f64 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

/// A sample of a wave at phase `t` (0 to 1) advancing `dt` a frame, with
/// `width` the high part of a pulse. Saw, square and pulse are bandlimited.
#[inline]
fn wave_sample(wave: Wave, t: f64, dt: f64, width: f64, noise: &mut u64) -> f64 {
    match wave {
        Wave::Sine => (TAU * t).sin(),
        Wave::Triangle => {
            let x = 4.0 * t;
            if t < 0.25 {
                x
            } else if t < 0.75 {
                2.0 - x
            } else {
                x - 4.0
            }
        }
        Wave::Saw => 2.0 * t - 1.0 - polyblep(t, dt),
        Wave::Square | Wave::Pulse => {
            let w = if wave == Wave::Square { 0.5 } else { width };
            let mut y = if t < w { 1.0 } else { -1.0 };
            y += polyblep(t, dt);
            y -= polyblep((t + 1.0 - w) % 1.0, dt);
            // A pulse that is high longer than it is low has an offset; the
            // wave is kept centered.
            y - (2.0 * w - 1.0)
        }
        Wave::Noise => {
            let mut x = *noise;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            *noise = x;
            unit(x) * 2.0 - 1.0
        }
    }
}

/// An LFO's value at phase `p` (0 to 1); `held` is a sample-and-hold's value
/// for the cycle.
#[inline]
fn lfo_sample(shape: LfoShape, p: f64, held: f64) -> f64 {
    match shape {
        LfoShape::Sine => (TAU * p).sin(),
        LfoShape::Triangle => {
            let x = 4.0 * p;
            if p < 0.25 {
                x
            } else if p < 0.75 {
                2.0 - x
            } else {
                x - 4.0
            }
        }
        LfoShape::Saw => 1.0 - 2.0 * p,
        LfoShape::Square => {
            if p < 0.5 {
                1.0
            } else {
                -1.0
            }
        }
        LfoShape::SampleHold => held,
    }
}

/// The state-variable filter of one voice: up to two sections of Simper's
/// trapezoidal SVF, each on both channels, tuned every control tick.
#[derive(Clone, Copy, Debug, Default)]
struct VoiceFilter {
    ic1: [[f64; 2]; 2],
    ic2: [[f64; 2]; 2],
    k: f64,
    a1: f64,
    a2: f64,
    a3: f64,
}

impl VoiceFilter {
    #[inline]
    fn tune(&mut self, cutoff: f64, resonance: f64, rate: f64) {
        let g = (PI * cutoff.clamp(10.0, rate * 0.49) / rate).tan();
        // 0 is a Q of a half; 100 rings just short of self-oscillation.
        self.k = (2.0 - 2.0 * (resonance / 100.0).clamp(0.0, 1.0)).max(0.02);
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    #[inline]
    fn process(&mut self, x: [f64; 2], mode: SynthFilterMode, sections: usize) -> [f64; 2] {
        let mut y = x;
        for s in 0..sections {
            for c in 0..2 {
                let v0 = y[c];
                let v3 = v0 - self.ic2[s][c];
                let v1 = self.a1 * self.ic1[s][c] + self.a2 * v3;
                let v2 = self.ic2[s][c] + self.a2 * self.ic1[s][c] + self.a3 * v3;
                self.ic1[s][c] = 2.0 * v1 - self.ic1[s][c];
                self.ic2[s][c] = 2.0 * v2 - self.ic2[s][c];
                y[c] = match mode {
                    SynthFilterMode::Lowpass => v2,
                    SynthFilterMode::Bandpass => v1,
                    SynthFilterMode::Highpass => v0 - self.k * v1 - v2,
                    SynthFilterMode::Notch => v0 - self.k * v1,
                };
            }
        }
        y
    }
}

// ---------------------------------------------------------------------------
// Voices

const DONE: usize = usize::MAX;

/// One sounding note.
#[derive(Clone, Copy, Debug)]
struct Voice {
    /// The note's place among the track's notes.
    note: usize,
    /// The timeline frame it started on.
    start: i64,
    pitch: f64,
    velocity: f64,
    /// The note-off, in frames after the start.
    off: f64,
    /// Frames since it started: its own time.
    frames: usize,
    /// The pitch it glides from, and the one it is at.
    from: f64,
    at_pitch: f64,
    random: f64,
    seed: u64,
    env: [Env; MAX_ENV],
    phase: [f64; MAX_OSC],
    inc: [f64; MAX_OSC],
    gain: [[f64; 2]; MAX_OSC],
    width: [f64; MAX_OSC],
    noise: [u64; MAX_OSC],
    /// A retriggered LFO's phase, cycle and held value.
    lfo_phase: [f64; MAX_LFO],
    lfo_cycle: [u64; MAX_LFO],
    lfo_held: [f64; MAX_LFO],
    filter: VoiceFilter,
    drive: f64,
    /// The amp envelope's level, and how far it moves a frame until the next tick.
    amp: f64,
    amp_step: f64,
    /// How much velocity leaves of the level.
    level: f64,
    /// Frames since a locate picked the voice up, while it fades in.
    ramp: usize,
    /// Frames left of the fade of a stolen voice, or one let go at a stop.
    fade_left: usize,
    fade: usize,
}

impl Voice {
    fn blank() -> Voice {
        Voice {
            note: 0,
            start: 0,
            pitch: 60.0,
            velocity: 1.0,
            off: 0.0,
            frames: 0,
            from: 60.0,
            at_pitch: 60.0,
            random: 0.0,
            seed: 0,
            env: [Env::default(); MAX_ENV],
            phase: [0.0; MAX_OSC],
            inc: [0.0; MAX_OSC],
            gain: [[0.0; 2]; MAX_OSC],
            width: [0.5; MAX_OSC],
            noise: [1; MAX_OSC],
            lfo_phase: [0.0; MAX_LFO],
            lfo_cycle: [0; MAX_LFO],
            lfo_held: [0.0; MAX_LFO],
            filter: VoiceFilter::default(),
            drive: 1.0,
            amp: 0.0,
            amp_step: 0.0,
            level: 1.0,
            ramp: DONE,
            fade_left: 0,
            fade: 1,
        }
    }

    /// A voice for a note, at the note's start. `base` is the patch at that
    /// frame and `lfo` the free LFOs' values there, from which the envelopes'
    /// times are taken for the whole note.
    fn start(patch: &Patch, notes: &[NoteOn], note: usize, base: &Values, lfo: &[f64; MAX_LFO], from: f64, fade: usize) -> Voice {
        let n = &notes[note];
        let seed = hash(&[patch.seed, note as u64]);
        let mut v = Voice::blank();
        v.note = note;
        v.start = n.start;
        v.pitch = n.pitch as f64;
        v.velocity = (n.velocity as f64 / 127.0).clamp(0.0, 1.0);
        v.off = (n.end - n.start).max(0) as f64;
        // The first note of a song has nothing to glide from.
        v.from = if from.is_nan() { v.pitch } else { from };
        v.at_pitch = v.from;
        v.random = unit(hash(&[seed, 1]));
        v.seed = seed;
        v.fade = fade.max(1);
        let vp = base.0[VELOCITY] / 100.0;
        v.level = (1.0 - vp) + vp * v.velocity;
        for (i, o) in patch.oscillators.iter().enumerate() {
            v.phase[i] = o.phase.unwrap_or_else(|| unit(hash(&[seed, 2, i as u64])));
            v.noise[i] = hash(&[seed, 3, i as u64]) | 1;
        }
        for i in 0..patch.lfos.len() {
            v.lfo_phase[i] = base.0[LFO + i * LFO_FIELDS + 1] / 100.0;
            v.lfo_held[i] = unit(hash(&[seed, 4, i as u64, 0])) * 2.0 - 1.0;
        }
        // The envelopes' times, with what the matrix does to them, as the
        // sources stand at the start: an envelope's own level there is 1
        // with no attack and 0 otherwise.
        let mut times = [[0.0; ENV_FIELDS]; MAX_ENV];
        for i in 0..patch.envelopes {
            times[i].copy_from_slice(&base.0[ENV + i * ENV_FIELDS..ENV + (i + 1) * ENV_FIELDS]);
        }
        for r in &patch.routes {
            let (i, field) = match r.target {
                ModTarget::EnvAttack(i) => (i, 0),
                ModTarget::EnvDecay(i) => (i, 1),
                ModTarget::EnvSustain(i) => (i, 2),
                ModTarget::EnvRelease(i) => (i, 3),
                _ => continue,
            };
            let s = match r.source {
                ModSource::Envelope(j) => f64::from(base.0[ENV + j * ENV_FIELDS] == 0.0),
                ModSource::Lfo(j) => {
                    if patch.lfos[j].retrigger {
                        lfo_sample(patch.lfos[j].shape, v.lfo_phase[j], v.lfo_held[j])
                    } else {
                        lfo[j]
                    }
                }
                ModSource::Velocity => v.velocity,
                ModSource::Note => (v.pitch - 60.0) / 12.0,
                ModSource::Random => v.random,
                ModSource::Macro(j) => base.0[MACRO + j] / 100.0,
            };
            let d = r.amount * s;
            if field == 2 {
                times[i][field] += d;
            } else {
                times[i][field] *= 2f64.powf(d);
            }
        }
        let frames = |ms: f64| ms.max(0.0) * patch.rate / 1000.0;
        for i in 0..patch.envelopes {
            v.env[i] = Env {
                attack: frames(times[i][0]),
                decay: frames(times[i][1]),
                sustain: (times[i][2] / 100.0).clamp(0.0, 1.0),
                release: frames(times[i][3]),
            };
        }
        v
    }

    /// Whether the amp envelope has fallen silent.
    #[inline]
    fn done(&self) -> bool {
        self.env[0].done(self.frames as f64, self.off)
    }

    /// Whether the note-off has passed.
    #[inline]
    fn releasing(&self) -> bool {
        self.frames as f64 >= self.off
    }

    /// The value of a source now.
    #[inline]
    fn source(&self, s: ModSource, patch: &Patch, v: &Values, lfo: &[f64; MAX_LFO], t: f64) -> f64 {
        match s {
            ModSource::Envelope(i) => self.env[i].level(t, self.off),
            ModSource::Lfo(i) => {
                if patch.lfos[i].retrigger {
                    lfo_sample(patch.lfos[i].shape, self.lfo_phase[i], self.lfo_held[i])
                } else {
                    lfo[i]
                }
            }
            ModSource::Velocity => self.velocity,
            ModSource::Note => (self.pitch - 60.0) / 12.0,
            ModSource::Random => self.random,
            ModSource::Macro(i) => v.0[MACRO + i] / 100.0,
        }
    }

    /// Works out every control value for the next `CONTROL` frames: the
    /// matrix, the pitch with its glide, each oscillator's increment and
    /// gains, the filter's tuning and the amp envelope's path.
    fn tick(&mut self, patch: &Patch, base: &Values, lfo: &[f64; MAX_LFO]) {
        let rate = patch.rate;
        let t = self.frames as f64;
        let mut v = *base;
        // Retriggered LFOs run on the voice's time.
        for (i, l) in patch.lfos.iter().enumerate() {
            if !l.retrigger || self.frames == 0 {
                continue;
            }
            let p = self.lfo_phase[i] + v.0[LFO + i * LFO_FIELDS] * CONTROL as f64 / rate;
            if p >= 1.0 {
                self.lfo_cycle[i] += 1;
                self.lfo_held[i] = unit(hash(&[self.seed, 4, i as u64, self.lfo_cycle[i]])) * 2.0 - 1.0;
            }
            self.lfo_phase[i] = p.rem_euclid(1.0);
        }
        let mut pitch = [0.0; MAX_OSC];
        let mut all = 0.0;
        for r in &patch.routes {
            let d = r.amount * self.source(r.source, patch, &v, lfo, t);
            match r.target {
                ModTarget::Pitch => all += d,
                ModTarget::OscPitch(i) => pitch[i] += d,
                ModTarget::OscLevel(i) => v.0[OSC + i * OSC_FIELDS] += d,
                ModTarget::OscPan(i) => v.0[OSC + i * OSC_FIELDS + 1] += d,
                ModTarget::OscPulseWidth(i) => v.0[OSC + i * OSC_FIELDS + 4] += d,
                ModTarget::Cutoff => v.0[CUTOFF] *= 2f64.powf(d),
                ModTarget::Resonance => v.0[RESONANCE] += d,
                ModTarget::Drive => v.0[DRIVE] += d,
                ModTarget::EnvAttack(_) | ModTarget::EnvDecay(_) | ModTarget::EnvSustain(_) | ModTarget::EnvRelease(_) => {}
            }
        }
        // The glide from the last note's pitch, over glide_ms.
        let glide = v.0[GLIDE] * rate / 1000.0;
        self.at_pitch = if glide > 0.0 && t < glide { self.from + (self.pitch - self.from) * t / glide } else { self.pitch };
        for (i, o) in patch.oscillators.iter().enumerate() {
            let at = OSC + i * OSC_FIELDS;
            let semis = self.at_pitch + o.octave * 12.0 + v.0[at + 2] + v.0[at + 3] / 100.0 + pitch[i] + all;
            self.inc[i] = (hz(semis) / rate).clamp(0.0, 0.5);
            let g = amplitude(v.0[at]);
            let [l, r] = pan_gains(v.0[at + 1]);
            self.gain[i] = [g * l, g * r];
            self.width[i] = (v.0[at + 4] / 100.0).clamp(0.01, 0.99);
        }
        if patch.filter_enabled {
            let track = (v.0[KEYTRACK] / 100.0) * (self.at_pitch - 60.0) / 12.0;
            self.filter.tune(v.0[CUTOFF] * 2f64.powf(track), v.0[RESONANCE], rate);
            self.drive = amplitude(v.0[DRIVE].max(0.0));
        }
        let now = self.env[0].level(t, self.off);
        let next = self.env[0].level(t + CONTROL as f64, self.off);
        self.amp = now;
        self.amp_step = (next - now) / CONTROL as f64;
    }

    /// Adds the voice's frames `lo..hi` of a block to `out`. `song` is the
    /// timeline frame of `out[0]` and `step` how it advances; `base` is
    /// read at ticks for the patch's values at a frame.
    #[allow(clippy::too_many_arguments)]
    fn run(
        &mut self,
        patch: &Patch,
        out: &mut [Frame],
        lo: usize,
        hi: usize,
        song: i64,
        step: i64,
        lfo: &[Vec<f64>; MAX_LFO],
        mut base: impl FnMut(i64, &mut Values),
        values: &mut Values,
    ) {
        let n_osc = patch.oscillators.len();
        let mut free = [0.0; MAX_LFO];
        for k in lo..hi {
            if self.frames % CONTROL == 0 {
                base(song + k as i64 * step, values);
                for (i, buf) in lfo.iter().enumerate().take(patch.lfos.len()) {
                    free[i] = buf[k];
                }
                self.tick(patch, values, &free);
            }
            let mut wet = [0.0; 2];
            let mut dry = [0.0; 2];
            for i in 0..n_osc {
                let o = &patch.oscillators[i];
                let s = wave_sample(o.wave, self.phase[i], self.inc[i], self.width[i], &mut self.noise[i]);
                let p = self.phase[i] + self.inc[i];
                self.phase[i] = if p >= 1.0 { p - 1.0 } else { p };
                let g = self.gain[i];
                if o.filtered {
                    wet[0] += s * g[0];
                    wet[1] += s * g[1];
                } else {
                    dry[0] += s * g[0];
                    dry[1] += s * g[1];
                }
            }
            if patch.filter_enabled {
                if self.drive > 1.0 {
                    // A soft clip whose knee sits above full scale at no drive.
                    wet = [4.0 * (self.drive * wet[0] / 4.0).tanh(), 4.0 * (self.drive * wet[1] / 4.0).tanh()];
                }
                wet = self.filter.process(wet, patch.filter_mode, patch.sections);
            }
            let mut g = self.amp * self.level;
            if self.ramp != DONE {
                g *= (self.ramp as f64 / self.fade as f64).min(1.0);
                self.ramp += 1;
                if self.ramp >= self.fade {
                    self.ramp = DONE;
                }
            }
            if self.fade_left != 0 {
                g *= self.fade_left as f64 / self.fade as f64;
                self.fade_left -= 1;
            }
            out[k][0] += (wet[0] + dry[0]) * g;
            out[k][1] += (wet[1] + dry[1]) * g;
            self.amp += self.amp_step;
            self.frames += 1;
        }
    }
}

// ---------------------------------------------------------------------------
// The synth

/// A track's synth: its voices, the free LFOs and the notes it plays.
pub struct Synth {
    patch: Arc<Patch>,
    notes: Arc<Vec<NoteOn>>,
    /// The next note not yet started.
    next: usize,
    active: Vec<Voice>,
    ringing: Vec<Voice>,
    last_pitch: f64,
    lfo_phase: [f64; MAX_LFO],
    lfo_cycle: [u64; MAX_LFO],
    lfo_held: [f64; MAX_LFO],
    lfo_buf: [Vec<f64>; MAX_LFO],
    /// The timeline frame last rendered, for the LFOs while ringing.
    frame: i64,
    fade: usize,
    /// The patch taken over from, while this one's values glide from its.
    glide_from: Option<Arc<Patch>>,
    glide_left: usize,
    values: Values,
}

impl Synth {
    /// A synth for `notes`, stopped at frame 0. Blocks are at most
    /// `max_block` frames; a stolen voice, a stop and a take-over fade over
    /// `fade` frames.
    pub fn new(patch: Arc<Patch>, notes: Arc<Vec<NoteOn>>, max_block: usize, fade: usize) -> Synth {
        let mut s = Synth {
            patch,
            notes,
            next: 0,
            active: Vec::with_capacity(MAX_VOICES),
            ringing: Vec::with_capacity(MAX_VOICES),
            last_pitch: f64::NAN,
            lfo_phase: [0.0; MAX_LFO],
            lfo_cycle: [0; MAX_LFO],
            lfo_held: [0.0; MAX_LFO],
            lfo_buf: [(); MAX_LFO].map(|_| vec![0.0; max_block.max(1)]),
            frame: 0,
            fade: fade.max(1),
            glide_from: None,
            glide_left: 0,
            values: Values([0.0; LEN]),
        };
        s.seek(0, false);
        s
    }

    pub fn patch(&self) -> &Arc<Patch> {
        &self.patch
    }

    /// Moves to timeline frame `at` and picks up the notes sounding there:
    /// each starts with its envelopes and the free LFOs where time would
    /// have brought them and its filter empty, fading in when `ramp` is set.
    pub fn seek(&mut self, at: i64, ramp: bool) {
        self.active.clear();
        self.next = 0;
        self.last_pitch = f64::NAN;
        self.frame = at;
        let patch = self.patch.clone();
        for i in 0..patch.lfos.len() {
            let p = patch.lfo_phase_at(i, at);
            self.lfo_cycle[i] = p.floor() as u64;
            self.lfo_phase[i] = p.rem_euclid(1.0);
            self.lfo_held[i] = unit(hash(&[patch.seed, 5, i as u64, self.lfo_cycle[i]])) * 2.0 - 1.0;
        }
        let notes = self.notes.clone();
        let mut values = self.values;
        for (i, n) in notes.iter().enumerate() {
            if n.start >= at {
                break;
            }
            self.next = i + 1;
            patch.base(n.start, &mut values);
            let mut lfo = [0.0; MAX_LFO];
            for (j, l) in patch.lfos.iter().enumerate() {
                let p = patch.lfo_phase_at(j, n.start);
                let held = unit(hash(&[patch.seed, 5, j as u64, p.floor() as u64])) * 2.0 - 1.0;
                lfo[j] = lfo_sample(l.shape, p.rem_euclid(1.0), held);
            }
            let mut v = Voice::start(&patch, &notes, i, &values, &lfo, self.last_pitch, self.fade);
            self.last_pitch = v.pitch;
            v.frames = (at - n.start) as usize;
            if v.done() {
                continue;
            }
            v.ramp = if ramp { 0 } else { DONE };
            self.place(v);
        }
        self.values = values;
    }

    /// Puts a voice among the sounding ones, stealing the oldest releasing
    /// voice, else the oldest, when the patch's voices are all in use.
    fn place(&mut self, v: Voice) {
        if self.active.len() >= self.patch.voices {
            let pick = self.active.iter().enumerate().max_by_key(|(_, a)| (a.releasing(), -a.start)).map(|(i, _)| i);
            if let Some(i) = pick {
                let mut stolen = self.active.swap_remove(i);
                stolen.fade_left = self.fade;
                if self.ringing.len() >= MAX_VOICES {
                    self.ringing.remove(0);
                }
                self.ringing.push(stolen);
            }
        }
        if self.active.len() < MAX_VOICES {
            self.active.push(v);
        }
    }

    /// Lets every sounding voice ring out over the fade, as at a stop.
    pub fn release(&mut self) {
        for mut v in self.active.drain(..) {
            v.fade_left = self.fade;
            if self.ringing.len() >= MAX_VOICES {
                self.ringing.remove(0);
            }
            self.ringing.push(v);
        }
    }

    /// Whether nothing sounds: no voice, and none starts before frame `end`.
    pub fn idle(&self, end: i64) -> bool {
        self.active.is_empty() && self.ringing.is_empty() && self.notes.get(self.next).is_none_or(|n| n.start >= end)
    }

    /// Fills the free LFOs' values for `n` frames from `frame`, advancing
    /// each by its rate a frame.
    fn fill_lfos(&mut self, frame: i64, step: i64, n: usize) {
        let patch = self.patch.clone();
        for (i, l) in patch.lfos.iter().enumerate() {
            let at = LFO + i * LFO_FIELDS;
            let lane = patch.lanes.iter().find(|(index, _)| *index == at).map(|(_, e)| e.clone());
            let rate = patch.statics.0[at];
            for k in 0..n {
                let f = frame + k as i64 * step;
                let r = lane.as_ref().map_or(rate, |e| e.at(f));
                self.lfo_buf[i][k] = lfo_sample(l.shape, self.lfo_phase[i], self.lfo_held[i]);
                if step != 0 {
                    let p = self.lfo_phase[i] + r / patch.rate;
                    if p >= 1.0 {
                        self.lfo_cycle[i] += 1;
                        self.lfo_held[i] = unit(hash(&[patch.seed, 5, i as u64, self.lfo_cycle[i]])) * 2.0 - 1.0;
                    }
                    self.lfo_phase[i] = p.rem_euclid(1.0);
                }
            }
        }
    }

    /// Adds the synth's output for timeline frames from `start` to `out`.
    /// While `rolling` the notes starting in the block start, each at its own
    /// frame, and nothing sounds at or past `total`; otherwise only voices
    /// ringing out sound. Voices picked up by a seek fade in over `ramp` frames.
    pub fn render(&mut self, out: &mut [Frame], start: i64, total: i64, rolling: bool, ramp: usize) {
        let n = out.len();
        if n == 0 {
            return;
        }
        let step = i64::from(rolling);
        self.frame = start;
        self.fill_lfos(start, step, n);
        let patch = self.patch.clone();
        let old = self.glide_from.clone();
        let x = if self.glide_left == 0 { 0.0 } else { self.glide_left as f64 / self.fade as f64 };
        let base = |frame: i64, v: &mut Values| {
            patch.base(frame, v);
            if let (Some(old), true) = (&old, x > 0.0) {
                let mut was = *v;
                old.base(frame, &mut was);
                for i in 0..LEN {
                    v.0[i] = if is_log(i) && v.0[i] > 0.0 && was.0[i] > 0.0 {
                        (v.0[i].ln() * (1.0 - x) + was.0[i].ln() * x).exp()
                    } else {
                        v.0[i] * (1.0 - x) + was.0[i] * x
                    };
                }
            }
        };
        let notes = self.notes.clone();
        // The frames of the block that sound: none at or past the end.
        let hi = if rolling { (total - start).clamp(0, n as i64) as usize } else { n };
        if ramp == 0 {
            for v in self.active.iter_mut() {
                v.ramp = DONE;
            }
        }
        // Voices run up to each note's start, where it starts and may steal
        // one of them, so a block is cut where its notes begin.
        let mut k0 = 0;
        loop {
            let starting = rolling && self.next < notes.len() && notes[self.next].start < start + hi as i64;
            let k1 = if starting { (notes[self.next].start - start).max(0) as usize } else { hi };
            {
                let Synth { active, ringing, lfo_buf, values, .. } = self;
                if rolling {
                    for v in active.iter_mut() {
                        v.run(&patch, out, k0, k1, start, step, lfo_buf, &base, values);
                    }
                }
                for v in ringing.iter_mut() {
                    let to = k1.min(k0 + v.fade_left);
                    v.run(&patch, out, k0, to, start, step, lfo_buf, &base, values);
                }
            }
            self.active.retain(|v| !v.done());
            self.ringing.retain(|v| v.fade_left > 0 && !v.done());
            if !starting {
                break;
            }
            let at = notes[self.next].start;
            while self.next < notes.len() && notes[self.next].start == at {
                let i = self.next;
                self.next += 1;
                let mut values = self.values;
                base(at, &mut values);
                let mut lfo = [0.0; MAX_LFO];
                for (j, buf) in self.lfo_buf.iter().enumerate().take(patch.lfos.len()) {
                    lfo[j] = buf[k1];
                }
                let v = Voice::start(&patch, &notes, i, &values, &lfo, self.last_pitch, self.fade);
                self.last_pitch = v.pitch;
                self.place(v);
                self.values = values;
            }
            k0 = k1;
        }
        self.glide_left = self.glide_left.saturating_sub(n);
        if self.glide_left == 0 {
            self.glide_from = None;
        }
    }

    /// Adds only the voices ringing out, for the synth of a renderer being
    /// taken over from.
    pub fn ring(&mut self, out: &mut [Frame]) {
        let frame = self.frame;
        self.render(out, frame, i64::MAX, false, 0);
    }

    /// Continues from `old`, which plays the same notes: its voices carry
    /// on here, and this patch's values glide from the old patch's over the
    /// fade where they differ.
    pub fn take_over(&mut self, old: &Synth) {
        self.next = old.next;
        self.active.clear();
        self.active.extend_from_slice(&old.active);
        self.ringing.clear();
        self.ringing.extend_from_slice(&old.ringing);
        self.last_pitch = old.last_pitch;
        self.lfo_phase = old.lfo_phase;
        self.lfo_cycle = old.lfo_cycle;
        self.lfo_held = old.lfo_held;
        self.frame = old.frame;
        self.values = old.values;
        if self.patch.statics.0 != old.patch.statics.0 {
            self.glide_from = Some(old.patch.clone());
            self.glide_left = self.fade;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::BigRational;

    /// The synth of a patch written as a flow mapping.
    fn spec(yaml: &str) -> Spec {
        let song = aaw_model::yaml_load::load(&format!("session: {{}}\ntracks:\n- id: t\n  type: midi\n  instrument:\n    synth: {yaml}\n")).unwrap();
        let p = aaw_model::Project::validate(&song).unwrap_or_else(|e| panic!("{e}"));
        p.tracks[0].midi.as_ref().unwrap().synth().unwrap().clone()
    }

    fn note(pitch: i64, start: i64, end: i64) -> NoteOn {
        NoteOn {
            pitch,
            velocity: 100,
            start,
            end,
            at: BigRational::from_integer(0.into()),
            beats: BigRational::from_integer(1.into()),
        }
    }

    fn render(synth: &mut Synth, frames: usize, block: usize) -> Vec<Frame> {
        let mut out = vec![[0.0; 2]; frames];
        let mut at = 0;
        while at < frames {
            let n = block.min(frames - at);
            synth.render(&mut out[at..at + n], at as i64, frames as i64, true, 0);
            at += n;
        }
        out
    }

    fn patch(yaml: &str) -> Arc<Patch> {
        Arc::new(Patch::new(&spec(yaml), &BTreeMap::new(), 48000.0, 120.0, 7))
    }

    #[test]
    fn a_render_does_not_depend_on_blocks_and_repeats_exactly() {
        let p = patch("{oscillators: {a: {wave: saw}, b: {wave: pulse, pulse_width: 30, detune_cents: 7, phase: 25}, n: {wave: noise, level_db: -20, filter: false}}, filter: {cutoff_hz: 900, resonance_percent: 30, drive_db: 6}, envelopes: {amp: {attack_ms: 5, decay_ms: 50, sustain_percent: 60, release_ms: 40}, env2: {attack_ms: 0, decay_ms: 100, sustain_percent: 0}}, lfos: {lfo1: {rate_hz: 3}, lfo2: {shape: sample_hold, rate_hz: 20, retrigger: true}}, macros: {tone: 40}, modulation: [{source: env2, target: filter.cutoff_hz, amount: 2}, {source: lfo1, target: pitch, amount: 0.3}, {source: lfo2, target: oscillators.b.pan, amount: 0.5}, {source: macros.tone, target: filter.cutoff_hz, amount: 1}, {source: velocity, target: envelopes.amp.release_ms, amount: 1}]}");
        let notes = Arc::new(vec![note(48, 100, 6000), note(55, 2000, 7000), note(60, 2000, 20000), note(48, 9000, 30000)]);
        let whole = render(&mut Synth::new(p.clone(), notes.clone(), 4096, 240), 40000, 4096);
        let pieces = render(&mut Synth::new(p.clone(), notes.clone(), 4096, 240), 40000, 61);
        assert_eq!(whole, pieces);
        let again = render(&mut Synth::new(p, notes, 4096, 240), 40000, 4096);
        assert_eq!(whole, again);
        assert!(whole[100..6000].iter().any(|f| f[0].abs() > 0.05), "the note is silent");
        assert!(whole.iter().all(|f| f[0].is_finite() && f[0].abs() < 4.0));
    }

    #[test]
    fn a_sine_is_at_its_pitch_and_level_and_the_envelope_shapes_it() {
        let p = patch("{oscillators: {a: {wave: sine, level_db: -6}}, envelopes: {amp: {attack_ms: 10, decay_ms: 0, sustain_percent: 100, release_ms: 20}}}");
        let notes = Arc::new(vec![note(69, 0, 24000)]);
        let out = render(&mut Synth::new(p, notes, 4096, 240), 30000, 4096);
        // 440 Hz: zero crossings every 1/880 s, 54.5 frames apart.
        let crossings = out[4800..24000].windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        let seconds = (24000 - 4800) as f64 / 48000.0;
        assert!((crossings as f64 / seconds - 440.0).abs() < 2.0, "{crossings} crossings");
        let peak = out[4800..24000].iter().fold(0.0f64, |m, f| m.max(f[0].abs()));
        // -6 dB at velocity 100 of 127, on each side of an equal-power center.
        let expected = amplitude(-6.0) * (100.0 / 127.0) * (PI / 4.0).cos();
        assert!((peak - expected).abs() < 0.01, "{peak} against {expected}");
        // The attack is linear over 10 ms, and the release is done 20 ms after the note-off.
        let at = |f: usize| out[f][0].abs();
        assert!(at(240) < at(470));
        assert!(out[24000 + 960 + 2..].iter().all(|f| f[0] == 0.0), "silent after the release");
        assert!(out[24000 + 480][0].abs() < 0.3 * expected || out[24000 + 500][0].abs() < 0.3 * expected);
    }

    #[test]
    fn a_seek_chases_the_sounding_note_and_voices_are_stolen_past_the_count() {
        let p = patch("{voices: 2, oscillators: {a: {wave: sine, phase: 0}}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}");
        let notes = Arc::new(vec![note(60, 0, 48000), note(64, 1000, 48000), note(67, 2000, 48000)]);
        let mut s = Synth::new(p.clone(), notes.clone(), 4096, 240);
        s.seek(10000, false);
        assert_eq!(s.active.len(), 2, "two voices after the third note stole the first");
        assert!(s.active.iter().all(|v| v.pitch != 60.0));
        let mut out = vec![[0.0; 2]; 1000];
        s.render(&mut out, 10000, 48000, true, 0);
        assert!(out.iter().any(|f| f[0].abs() > 0.1));
        // A stop lets the voices ring out over the fade and no further.
        s.release();
        let mut tail = vec![[0.0; 2]; 1000];
        s.render(&mut tail, 11000, 48000, false, 0);
        assert!(tail[..200].iter().any(|f| f[0].abs() > 0.01));
        assert!(tail[300..].iter().all(|f| f[0] == 0.0));
        assert!(s.idle(i64::MAX));
    }

    #[test]
    fn glide_slides_from_the_last_note_and_a_lane_moves_the_cutoff() {
        let p = patch("{voices: 1, glide_ms: 100, oscillators: {a: {wave: sine, phase: 0}}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}");
        let notes = Arc::new(vec![note(57, 0, 10000), note(69, 10000, 48000)]);
        let mut s = Synth::new(p, notes, 4096, 240);
        let out = render(&mut s, 30000, 4096);
        let rate = |from: usize, to: usize| {
            let c = out[from..to].windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
            c as f64 / ((to - from) as f64 / 48000.0)
        };
        assert!((rate(2000, 9000) - 220.0).abs() < 3.0, "{}", rate(2000, 9000));
        assert!((rate(20000, 29000) - 440.0).abs() < 3.0, "{}", rate(20000, 29000));
        let mid = rate(11000, 13000);
        assert!(mid > 250.0 && mid < 420.0, "gliding: {mid}");
        // A lane on the cutoff, read at the song's frames.
        let lane = aaw_model::Lane {
            param: "instrument.filter.cutoff_hz".into(),
            points: vec![
                aaw_model::Point { at: aaw_model::Beat::int(0), value: 100.0, curve: aaw_model::Curve::Linear, shape: 0.0 },
                aaw_model::Point { at: aaw_model::Beat::int(1), value: 8000.0, curve: aaw_model::Curve::Linear, shape: 0.0 },
            ],
        };
        let env = Arc::new(Envelope::new(&lane, aaw_model::rules::Domain::Log, 120.0, 48000));
        let mut lanes = BTreeMap::new();
        lanes.insert("filter.cutoff_hz".to_string(), env);
        let sweep = Arc::new(Patch::new(&spec("{oscillators: {a: {wave: saw, phase: 0}}, filter: {cutoff_hz: 20000, slope_db_per_octave: 24}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}"), &lanes, 48000.0, 120.0, 7));
        let notes = Arc::new(vec![note(60, 0, 48000)]);
        let out = render(&mut Synth::new(sweep, notes, 4096, 240), 30000, 4096);
        let rms = |from: usize, to: usize| (out[from..to].iter().map(|f| f[0] * f[0]).sum::<f64>() / (to - from) as f64).sqrt();
        // Middle C under a 24 dB lowpass at 100 Hz, then open: the lane sweeps it.
        assert!(rms(1000, 5000) < 0.2 * rms(25000, 29000), "closed {} against open {}", rms(1000, 5000), rms(25000, 29000));
    }

    #[test]
    fn a_take_over_carries_the_voices_and_glides_the_patch() {
        let notes = Arc::new(vec![note(60, 0, 48000)]);
        let a = patch("{oscillators: {a: {wave: saw, phase: 0}}, filter: {cutoff_hz: 400}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}");
        let b = patch("{oscillators: {a: {wave: saw, phase: 0}}, filter: {cutoff_hz: 4000}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}");
        assert_eq!(a.signature, b.signature);
        let mut old = Synth::new(a, notes.clone(), 4096, 240);
        let _ = render(&mut old, 4096, 4096);
        let mut new = Synth::new(b, notes, 4096, 240);
        new.take_over(&old);
        assert_eq!(new.active.len(), 1);
        assert_eq!(new.active[0].frames, 4096);
        let mut out = vec![[0.0; 2]; 4096];
        new.render(&mut out, 4096, 48000, true, 0);
        assert!(out.windows(2).all(|w| (w[0][0] - w[1][0]).abs() < 0.5), "no jump");
        let c = patch("{oscillators: {a: {wave: square, phase: 0}}, filter: {cutoff_hz: 4000}, envelopes: {amp: {attack_ms: 0, release_ms: 0}}}");
        assert_ne!(new.patch.signature, c.signature);
    }

    #[test]
    fn allocation_is_at_build_time() {
        let p = patch("{oscillators: {a: {}, b: {wave: pulse}}, lfos: {l: {rate_beats: 1}}, modulation: [{source: l, target: pitch, amount: 1}]}");
        let notes = Arc::new((0..40).map(|i| note(40 + i % 24, i * 500, i * 500 + 3000)).collect::<Vec<_>>());
        let mut s = Synth::new(p, notes, 256, 240);
        let mut out = vec![[0.0; 2]; 256];
        assert_no_alloc::assert_no_alloc(|| {
            for b in 0..200 {
                s.render(&mut out, b * 256, 1 << 30, true, 0);
            }
            s.seek(30000, true);
            s.render(&mut out, 30000, 1 << 30, true, 240);
            s.release();
            s.render(&mut out, 30256, 1 << 30, false, 0);
        });
    }
}


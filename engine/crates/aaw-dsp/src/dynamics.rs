//! The compressor and the limiter, as `effects.Dynamics` and
//! `effects.LimiterDevice`.

use crate::envelope::Knob;
use crate::{Clock, Frame};

/// Limiter smoothing sums fixed-point dB, so sums do not depend on blocks.
const FIXED: f64 = (1u64 << 24) as f64;

/// The louder channel in dB; negative infinity for silence.
#[inline]
fn peak_db(f: Frame) -> f64 {
    20.0 * f[0].abs().max(f[1].abs()).log10()
}

/// Gain reduction over the output frames inside the session.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reduction {
    pub max: f64,
    pub sum: f64,
    pub over_1db: u64,
    pub frames: u64,
}

impl Reduction {
    #[inline]
    fn record(&mut self, db: f64, output_frame: i64, clock: &Clock) {
        if clock.step == 1 && output_frame >= 0 && output_frame < clock.total {
            self.max = self.max.max(db);
            self.sum += db;
            self.over_1db += u64::from(db > 1.0);
            self.frames += 1;
        }
    }

    pub fn mean(&self) -> f64 {
        self.sum / self.frames.max(1) as f64
    }

    pub fn fraction_over_1db(&self) -> f64 {
        self.over_1db as f64 / self.frames.max(1) as f64
    }
}

/// `y[n] = max(x[n], decay * y[n-1])`: reduction falls by a factor of e each
/// release time.
#[derive(Clone, Copy, Debug)]
struct ReleaseHold {
    decay: f64,
    held: f64,
}

impl ReleaseHold {
    fn new(release_frames: f64) -> ReleaseHold {
        ReleaseHold {
            decay: (-1.0 / release_frames.max(1e-9)).exp(),
            held: 0.0,
        }
    }

    #[inline]
    fn tick(&mut self, x: f64) -> f64 {
        self.held = x.max(self.held * self.decay);
        self.held
    }
}

/// The static settings of a compressor.
#[derive(Clone, Copy, Debug)]
pub struct CompressorSettings {
    pub ratio: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub knee_db: f64,
}

/// Stereo-linked peak compressor: soft-knee static curve, release hold, attack
/// pole. With a key, the key's level sets the reduction.
#[derive(Clone, Debug)]
pub struct Compressor {
    slope: f64,
    knee: f64,
    alpha: f64,
    hold: ReleaseHold,
    /// The attack pole's state.
    z: f64,
    pub threshold: Knob,
    pub makeup: Knob,
    thresholds: Vec<f64>,
    makeups: Vec<f64>,
    pub reduction: Reduction,
}

impl Compressor {
    pub fn new(s: CompressorSettings, threshold: Knob, makeup: Knob, rate: f64, max_block: usize) -> Compressor {
        let attack = s.attack_ms * rate / 1000.0;
        Compressor {
            slope: 1.0 - 1.0 / s.ratio,
            knee: s.knee_db,
            alpha: if attack > 0.0 { (-1.0 / attack).exp() } else { 0.0 },
            hold: ReleaseHold::new(s.release_ms * rate / 1000.0),
            z: 0.0,
            threshold,
            makeup,
            thresholds: vec![0.0; max_block],
            makeups: vec![0.0; max_block],
            reduction: Reduction::default(),
        }
    }

    #[inline]
    fn curve(&self, level: f64, threshold: f64) -> f64 {
        let over = level - threshold;
        let mut r = if over > 0.0 { over * self.slope } else { 0.0 };
        if self.knee > 0.0 && over.abs() <= self.knee / 2.0 {
            let x = over + self.knee / 2.0;
            r = self.slope * (x * x) / (2.0 * self.knee);
        }
        if level.is_finite() { r } else { 0.0 }
    }

    pub fn process(&mut self, x: &mut [Frame], key: Option<&[Frame]>, clock: Clock) {
        let n = x.len();
        self.threshold.fill(clock.frame, clock.step, &mut self.thresholds[..n]);
        self.makeup.fill(clock.frame, clock.step, &mut self.makeups[..n]);
        for i in 0..n {
            let level = peak_db(key.map_or(x[i], |k| k[i]));
            let mut reduction = self.hold.tick(self.curve(level, self.thresholds[i]));
            if self.alpha != 0.0 {
                reduction = (1.0 - self.alpha) * reduction + self.z;
                self.z = self.alpha * reduction;
            }
            self.reduction.record(reduction, clock.frame + i as i64 * clock.step, &clock);
            let gain = 10f64.powf((self.makeups[i] - reduction) / 20.0);
            x[i] = [x[i][0] * gain, x[i][1] * gain];
        }
    }

    /// Continues from `old`'s detector state; the levels glide to their own.
    pub fn take_over(&mut self, old: &Compressor) {
        self.hold.held = old.hold.held;
        self.z = old.z;
        self.threshold.take_over(&old.threshold);
        self.makeup.take_over(&old.makeup);
    }
}

/// Look-ahead brickwall limiter on sample peaks.
///
/// Required reduction is maximized over the look-ahead window, held with the
/// release, then averaged over the same window in fixed point. Every average
/// therefore covers the peak it protects, so no output sample exceeds the
/// ceiling. The look-ahead is the device's latency.
#[derive(Clone, Debug)]
pub struct Limiter {
    ceiling: f64,
    lookahead: usize,
    hold: ReleaseHold,
    /// The last `lookahead` input frames.
    audio: Vec<Frame>,
    /// Candidates for the window's maximum: required reduction and the input
    /// frame it came at, in falling order of reduction.
    window: Vec<(f64, u64)>,
    head: usize,
    count: usize,
    /// The last `lookahead + 1` held reductions in fixed point, and their sum.
    held: Vec<i64>,
    sum: i64,
    position: usize,
    frames: u64,
    pub reduction: Reduction,
}

impl Limiter {
    pub fn latency(lookahead_ms: f64, rate: f64) -> usize {
        ((lookahead_ms * rate / 1000.0).round_ties_even() as usize).max(1)
    }

    pub fn new(ceiling_db: f64, release_ms: f64, lookahead_ms: f64, rate: f64) -> Limiter {
        let n = Limiter::latency(lookahead_ms, rate);
        Limiter {
            ceiling: ceiling_db,
            lookahead: n,
            hold: ReleaseHold::new(release_ms * rate / 1000.0),
            audio: vec![[0.0; 2]; n],
            window: vec![(0.0, 0); n + 2],
            head: 0,
            count: 0,
            held: vec![0; n + 1],
            sum: 0,
            position: 0,
            frames: 0,
            reduction: Reduction::default(),
        }
    }

    /// The largest required reduction among the last `lookahead + 1` frames.
    #[inline]
    fn window_max(&mut self, required: f64) -> f64 {
        let size = self.window.len();
        while self.count > 0 && self.window[(self.head + self.count - 1) % size].0 <= required {
            self.count -= 1;
        }
        self.window[(self.head + self.count) % size] = (required, self.frames);
        self.count += 1;
        if self.window[self.head].1 + (self.lookahead as u64) < self.frames {
            self.head = (self.head + 1) % size;
            self.count -= 1;
        }
        self.window[self.head].0
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = self.lookahead;
        for (i, frame) in x.iter_mut().enumerate() {
            let required = (peak_db(*frame) - self.ceiling).max(0.0);
            let needed = self.window_max(required);
            let held = (self.hold.tick(needed) * FIXED).ceil() as i64;
            let slot = self.position % (n + 1);
            self.sum += held - self.held[slot];
            self.held[slot] = held;
            let reduction = self.sum as f64 / (n + 1) as f64 / FIXED;
            let delayed = std::mem::replace(&mut self.audio[self.position % n], *frame);
            self.position = (self.position + 1) % (n * (n + 1));
            self.frames += 1;
            self.reduction.record(reduction, clock.frame + i as i64 * clock.step - n as i64, &clock);
            let gain = 10f64.powf(-reduction / 20.0);
            *frame = [delayed[0] * gain, delayed[1] * gain];
        }
    }

    /// Continues from `old`'s state, when its look-ahead is the same.
    pub fn take_over(&mut self, old: &mut Limiter) {
        if old.lookahead != self.lookahead {
            return;
        }
        std::mem::swap(&mut self.audio, &mut old.audio);
        std::mem::swap(&mut self.window, &mut old.window);
        std::mem::swap(&mut self.held, &mut old.held);
        self.head = old.head;
        self.count = old.count;
        self.sum = old.sum;
        self.position = old.position;
        self.frames = old.frames;
        self.hold.held = old.hold.held;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Param;
    use std::f64::consts::PI;

    fn clock() -> Clock {
        Clock {
            frame: 0,
            step: 1,
            total: 1 << 40,
        }
    }

    fn sine(amplitude: f64, n: usize) -> Vec<Frame> {
        (0..n)
            .map(|i| {
                let x = amplitude * (2.0 * PI * 997.0 * i as f64 / 48000.0).sin();
                [x, x]
            })
            .collect()
    }

    fn compressor(threshold: f64, ratio: f64, knee: f64, makeup: f64) -> Compressor {
        let settings = CompressorSettings {
            ratio,
            attack_ms: 1.0,
            release_ms: 100.0,
            knee_db: knee,
        };
        Compressor::new(settings, Knob::new(Param::fixed(threshold), 240), Knob::new(Param::fixed(makeup), 240), 48000.0, 4096)
    }

    fn peak(x: &[Frame]) -> f64 {
        x.iter().fold(0.0, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    #[test]
    fn the_compressor_follows_its_curve() {
        // A -6 dBFS sine over a -18 dB threshold at 4:1: 12 dB over, 9 dB off.
        let mut x = sine(0.5011872336272722, 48000);
        let mut c = compressor(-18.0, 4.0, 0.0, 0.0);
        for chunk in x.chunks_mut(4096) {
            c.process(chunk, None, clock());
        }
        let db = 20.0 * peak(&x[24000..]).log10();
        assert!((db - (-6.0 - 9.0)).abs() < 0.1, "{db}");
        assert!((c.reduction.max - 9.0).abs() < 0.05);
        // Below the threshold and the knee it does nothing.
        let quiet = sine(0.01, 4096);
        let mut y = quiet.clone();
        compressor(-18.0, 4.0, 6.0, 0.0).process(&mut y, None, clock());
        assert_eq!(quiet, y);
    }

    #[test]
    fn a_key_ducks_and_makeup_adds() {
        let mut x = sine(0.1, 9600);
        let key = sine(0.9, 9600);
        compressor(-30.0, 8.0, 6.0, 0.0).process(&mut x[..4096], Some(&key[..4096]), clock());
        assert!(peak(&x[2000..4096]) < 0.02);
        let mut y = sine(0.01, 4096);
        compressor(-18.0, 4.0, 0.0, 6.0).process(&mut y, None, clock());
        assert!((peak(&y) / 0.01 - 10f64.powf(6.0 / 20.0)).abs() < 1e-3);
    }

    #[test]
    fn the_limiter_holds_its_ceiling_and_delays_by_its_lookahead() {
        let ceiling = 10f64.powf(-1.0 / 20.0);
        let mut loud = sine(1.6, 48000);
        let mut l = Limiter::new(-1.0, 60.0, 3.0, 48000.0);
        for chunk in loud.chunks_mut(500) {
            l.process(chunk, clock());
        }
        assert!(peak(&loud) <= ceiling * (1.0 + 1e-9), "{}", peak(&loud));
        assert!(peak(&loud) > ceiling * 0.98);
        // Below the ceiling the input passes delayed and unchanged.
        let quiet = sine(0.3, 2000);
        let mut out = quiet.clone();
        Limiter::new(-1.0, 60.0, 3.0, 48000.0).process(&mut out, clock());
        assert_eq!(Limiter::latency(3.0, 48000.0), 144);
        assert!(out[..144].iter().all(|f| *f == [0.0; 2]));
        assert_eq!(&out[144..], &quiet[..2000 - 144]);
    }

    #[test]
    fn dynamics_do_not_depend_on_blocks() {
        let x: Vec<Frame> = sine(1.3, 30000).iter().enumerate().map(|(i, f)| [f[0] * (1.0 + (i % 7000) as f64 / 7000.0) / 2.0, f[1]]).collect();
        let mut whole = x.clone();
        let (mut c, mut l) = (compressor(-20.0, 6.0, 8.0, 3.0), Limiter::new(-3.0, 40.0, 1.5, 48000.0));
        for chunk in whole.chunks_mut(4096) {
            c.process(chunk, None, clock());
            l.process(chunk, clock());
        }
        let mut pieces = x.clone();
        let (mut c, mut l) = (compressor(-20.0, 6.0, 8.0, 3.0), Limiter::new(-3.0, 40.0, 1.5, 48000.0));
        let (mut at, mut size) = (0, 1);
        while at < pieces.len() {
            let n = size.min(pieces.len() - at);
            c.process(&mut pieces[at..at + n], None, clock());
            l.process(&mut pieces[at..at + n], clock());
            at += n;
            size = size * 5 % 997 + 1;
        }
        assert_eq!(whole, pieces);
    }
}

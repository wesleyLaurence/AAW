//! The compressor and the limiter.

use crate::envelope::Knob;
use crate::truepeak::{Between, REACH};
use crate::{Clock, Frame};

/// Limiter smoothing sums fixed-point dB, so sums do not depend on blocks.
const FIXED: f64 = (1u64 << 24) as f64;

/// How far under its ceiling a limiter on a true peak aims. Where the gain
/// moves across the samples a point between them is read from, the point
/// comes out a little off the gain times what it was: on noise far over the
/// ceiling by up to 0.001 dB with a look-ahead of 3 ms and 0.004 with 1 ms.
pub const TRUE_PEAK_MARGIN_DB: f64 = 0.01;

/// The louder channel in dB; negative infinity for silence.
#[inline]
fn peak_db(f: Frame) -> f64 {
    20.0 * f[0].abs().max(f[1].abs()).log10()
}

/// How much a device took off over some frames.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Reduction {
    pub max: f64,
    pub sum: f64,
    pub over_1db: u64,
    pub frames: u64,
}

impl Reduction {
    #[inline]
    fn add(&mut self, db: f64) {
        self.max = self.max.max(db);
        self.sum += db;
        self.over_1db += u64::from(db > 1.0);
        self.frames += 1;
    }

    /// Takes in what another stretch of frames measured.
    fn join(&mut self, other: &Reduction) {
        self.max = self.max.max(other.max);
        self.sum += other.sum;
        self.over_1db += other.over_1db;
        self.frames += other.frames;
    }

    pub fn mean(&self) -> f64 {
        self.sum / self.frames.max(1) as f64
    }

    pub fn fraction_over_1db(&self) -> f64 {
        self.over_1db as f64 / self.frames.max(1) as f64
    }
}

/// A device's gain reduction over the output frames inside the session:
/// the whole of it, and each stretch between the edges a render asked for,
/// from which a section's is read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reductions {
    pub whole: Reduction,
    /// Timeline frames, rising, and the reduction between each and the next.
    edges: Vec<i64>,
    parts: Vec<Reduction>,
    /// The stretch the last frame fell in or after.
    at: usize,
}

impl Reductions {
    /// Measures each stretch between these timeline frames apart as well, in
    /// whatever order they come. A render asks before it starts; nothing
    /// played live does.
    pub fn split(&mut self, edges: &[i64]) {
        self.edges = edges.to_vec();
        self.edges.sort_unstable();
        self.edges.dedup();
        self.parts = vec![Reduction::default(); self.edges.len().saturating_sub(1)];
        self.at = 0;
    }

    #[inline]
    pub(crate) fn record(&mut self, db: f64, output_frame: i64, clock: &Clock) {
        if clock.step == 1 && output_frame >= 0 && output_frame < clock.total {
            self.whole.add(db);
            if !self.parts.is_empty() {
                if output_frame < self.edges[self.at] {
                    self.at = 0;
                }
                while self.at + 1 < self.parts.len() && output_frame >= self.edges[self.at + 1] {
                    self.at += 1;
                }
                if output_frame >= self.edges[self.at] && output_frame < self.edges[self.at + 1] {
                    self.parts[self.at].add(db);
                }
            }
        }
    }

    /// The reduction over the frames from `start` to `end`, both of them
    /// edges given to `split`.
    pub fn within(&self, start: i64, end: i64) -> Reduction {
        let mut out = Reduction::default();
        for (part, edges) in self.parts.iter().zip(self.edges.windows(2)) {
            if edges[0] >= start && edges[1] <= end {
                out.join(part);
            }
        }
        out
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
    pub reduction: Reductions,
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
            reduction: Reductions::default(),
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

/// Look-ahead brickwall limiter, on sample peaks or on the level between
/// the samples.
///
/// Required reduction is maximized over the look-ahead window, held with the
/// release, then averaged over the same window in fixed point. Every average
/// therefore covers the peak it protects, so no output sample exceeds the
/// ceiling. The look-ahead is the device's latency.
///
/// On a true peak, a frame's level is the highest of the sample and the
/// three points up to the next, as a render's report estimates them. Those
/// points read the samples from `REACH - 1` before to `REACH` after, so the
/// window is that much longer either way and the audio that much later:
/// each average over those samples then covers the peak, the gain is the
/// same across them when the peak stands alone, and the points come out at
/// the ceiling.
#[derive(Clone, Debug)]
pub struct Limiter {
    ceiling: f64,
    lookahead: usize,
    /// Frames the audio is late by, and frames before the last over which
    /// required reduction is maximized: the look-ahead, and what a true
    /// peak's points take.
    delay: usize,
    between: Option<Between>,
    hold: ReleaseHold,
    /// The last `delay` input frames.
    audio: Vec<Frame>,
    at: usize,
    /// Candidates for the window's maximum: required reduction and the input
    /// frame it came at, in falling order of reduction.
    window: Vec<(f64, u64)>,
    head: usize,
    count: usize,
    /// The last `lookahead + 1` held reductions in fixed point, and their sum.
    held: Vec<i64>,
    sum: i64,
    slot: usize,
    frames: u64,
    pub reduction: Reductions,
}

impl Limiter {
    pub fn latency(lookahead_ms: f64, rate: f64, true_peak: bool) -> usize {
        let lookahead = ((lookahead_ms * rate / 1000.0).round_ties_even() as usize).max(1);
        lookahead + if true_peak { 2 * REACH - 1 } else { 0 }
    }

    pub fn new(ceiling_db: f64, release_ms: f64, lookahead_ms: f64, true_peak: bool, rate: f64) -> Limiter {
        let n = Limiter::latency(lookahead_ms, rate, false);
        let delay = Limiter::latency(lookahead_ms, rate, true_peak);
        Limiter {
            ceiling: ceiling_db - if true_peak { TRUE_PEAK_MARGIN_DB } else { 0.0 },
            lookahead: n,
            delay,
            between: true_peak.then(Between::new),
            hold: ReleaseHold::new(release_ms * rate / 1000.0),
            audio: vec![[0.0; 2]; delay],
            at: 0,
            window: vec![(0.0, 0); delay + 2],
            head: 0,
            count: 0,
            held: vec![0; n + 1],
            sum: 0,
            slot: 0,
            frames: 0,
            reduction: Reductions::default(),
        }
    }

    /// The largest required reduction among the last `delay + 1` frames.
    #[inline]
    fn window_max(&mut self, required: f64) -> f64 {
        let size = self.window.len();
        while self.count > 0 && self.window[(self.head + self.count - 1) % size].0 <= required {
            self.count -= 1;
        }
        self.window[(self.head + self.count) % size] = (required, self.frames);
        self.count += 1;
        if self.window[self.head].1 + (self.delay as u64) < self.frames {
            self.head = (self.head + 1) % size;
            self.count -= 1;
        }
        self.window[self.head].0
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = self.lookahead;
        for (i, frame) in x.iter_mut().enumerate() {
            let level = match &mut self.between {
                Some(between) => 20.0 * between.tick(*frame).log10(),
                None => peak_db(*frame),
            };
            let required = (level - self.ceiling).max(0.0);
            let needed = self.window_max(required);
            let held = (self.hold.tick(needed) * FIXED).ceil() as i64;
            self.sum += held - self.held[self.slot];
            self.held[self.slot] = held;
            self.slot = (self.slot + 1) % (n + 1);
            let reduction = self.sum as f64 / (n + 1) as f64 / FIXED;
            let delayed = std::mem::replace(&mut self.audio[self.at], *frame);
            self.at = (self.at + 1) % self.delay;
            self.frames += 1;
            self.reduction.record(reduction, clock.frame + i as i64 * clock.step - self.delay as i64, &clock);
            let gain = 10f64.powf(-reduction / 20.0);
            *frame = [delayed[0] * gain, delayed[1] * gain];
        }
    }

    /// Continues from `old`'s state, when it is as late and reads the same
    /// peaks.
    pub fn take_over(&mut self, old: &mut Limiter) {
        if old.lookahead != self.lookahead || old.between.is_some() != self.between.is_some() {
            return;
        }
        std::mem::swap(&mut self.audio, &mut old.audio);
        std::mem::swap(&mut self.window, &mut old.window);
        std::mem::swap(&mut self.held, &mut old.held);
        if let (Some(new), Some(was)) = (&mut self.between, &old.between) {
            new.take_over(was);
        }
        self.at = old.at;
        self.head = old.head;
        self.count = old.count;
        self.sum = old.sum;
        self.slot = old.slot;
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

    fn amplitude(db: f64) -> f64 {
        10f64.powf(db / 20.0)
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
        assert!((c.reduction.whole.max - 9.0).abs() < 0.05);
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
        let mut l = Limiter::new(-1.0, 60.0, 3.0, false, 48000.0);
        for chunk in loud.chunks_mut(500) {
            l.process(chunk, clock());
        }
        assert!(peak(&loud) <= ceiling * (1.0 + 1e-9), "{}", peak(&loud));
        assert!(peak(&loud) > ceiling * 0.98);
        // Below the ceiling the input passes delayed and unchanged.
        let quiet = sine(0.3, 2000);
        let mut out = quiet.clone();
        Limiter::new(-1.0, 60.0, 3.0, false, 48000.0).process(&mut out, clock());
        assert_eq!(Limiter::latency(3.0, 48000.0, false), 144);
        assert!(out[..144].iter().all(|f| *f == [0.0; 2]));
        assert_eq!(&out[144..], &quiet[..2000 - 144]);
    }

    #[test]
    fn reduction_is_measured_between_the_edges_a_render_gives() {
        // A second under the ceiling, a second 6 dB over it and a second
        // under: three sections, the middle one also halved, and a fourth
        // that starts where the song ends.
        let n = 48000;
        let mut x = sine(0.25, n);
        x.extend(sine(1.0, n));
        x.extend(sine(0.25, n));
        let mut l = Limiter::new(-6.0, 20.0, 2.0, false, 48000.0);
        let total = x.len() as i64;
        let n = n as i64;
        l.reduction.split(&[2 * n, 0, n, n, 3 * n, n + n / 2, 2 * n, 4 * n]);
        let mut at = 0;
        for chunk in x.chunks_mut(1000) {
            l.process(chunk, Clock { frame: at, step: 1, total });
            at += chunk.len() as i64;
        }
        let r = &l.reduction;
        // The first section ends in the limiter's look-ahead, reaching for
        // the peak that follows; the rest of it is untouched.
        let first = r.within(0, n);
        assert!(first.frames == 48000 && first.max > 5.0 && first.fraction_over_1db() < 0.01, "{first:?}");
        let middle = r.within(n, 2 * n);
        assert!((middle.max - 6.0).abs() < 0.01 && middle.fraction_over_1db() > 0.99 && (middle.mean() - 6.0).abs() < 0.1, "{middle:?}");
        let half = r.within(n, n + n / 2);
        assert!(half.frames == 24000 && (half.max - 6.0).abs() < 0.01);
        // The last starts in the release and then rests.
        let last = r.within(2 * n, 3 * n);
        assert!(last.frames as i64 == n - 96 && last.fraction_over_1db() < 0.1 && last.mean() < 0.3, "{last:?}");
        assert_eq!(r.within(3 * n, 4 * n), Reduction::default());
        // The stretches together are the whole, which is measured as before.
        let all = r.within(0, 3 * n);
        assert_eq!((all.frames, all.over_1db, all.max), (r.whole.frames, r.whole.over_1db, r.whole.max));
        assert!((all.sum - r.whole.sum).abs() < 1e-6);
        // A transport that goes back finds its stretch again.
        let mut again = Reductions::default();
        again.split(&[0, 10, 20]);
        let clock = Clock { frame: 0, step: 1, total: 20 };
        for frame in [15, 16, 3, 4, 5, 19] {
            again.record(2.0, frame, &clock);
        }
        assert_eq!((again.within(0, 10).frames, again.within(10, 20).frames), (3, 3));
    }

    /// The highest level between the samples of `x` as it goes on, in dB:
    /// the report's estimate without the cut a file's end is.
    fn between_db(x: &[Frame]) -> f64 {
        let mut between = Between::new();
        20.0 * x.iter().map(|f| between.tick(*f)).fold(0.0f64, f64::max).log10()
    }

    fn noise(seed: u64, level: f64, n: usize) -> Vec<Frame> {
        let mut seed = seed;
        let mut next = move || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((seed >> 11) as f64 / (1u64 << 52) as f64 - 1.0) * level
        };
        (0..n).map(|_| [next(), next()]).collect()
    }

    fn limited(x: &[Frame], ceiling: f64, release: f64, lookahead: f64, true_peak: bool, rate: f64) -> Vec<Frame> {
        let mut y = x.to_vec();
        let mut l = Limiter::new(ceiling, release, lookahead, true_peak, rate);
        for chunk in y.chunks_mut(4096) {
            l.process(chunk, clock());
        }
        y
    }

    #[test]
    fn on_a_true_peak_the_limiter_holds_the_level_between_the_samples() {
        // A tone at a quarter of the rate whose samples lie at 0.707 of its
        // crests, swelling: the samples are held to the ceiling and the
        // crests pass it by 3 dB, or the crests are held.
        let tone: Vec<Frame> = (0..48000).map(|i| [2.0 * (PI * (i as f64 / 2.0 + 0.25)).sin() * (1.0 + (i as f64 / 3000.0).sin()) / 2.0; 2]).collect();
        let plain = limited(&tone, -1.0, 60.0, 3.0, false, 48000.0);
        assert!(peak(&plain) <= amplitude(-1.0) * (1.0 + 1e-9) && between_db(&plain) > 1.9, "{}", between_db(&plain));
        let held = limited(&tone, -1.0, 60.0, 3.0, true, 48000.0);
        let most = between_db(&held);
        assert!(most <= -1.0 && most > -1.0 - 2.0 * TRUE_PEAK_MARGIN_DB, "{most}");
        assert!((20.0 * peak(&held).log10() + 4.0).abs() < 0.05, "the samples are 3 dB under their crests");
        // It is later by what the points between the samples read.
        assert_eq!((Limiter::latency(3.0, 48000.0, false), Limiter::latency(3.0, 48000.0, true)), (144, 163));
        let quiet = sine(0.3, 2000);
        let out = limited(&quiet, -1.0, 60.0, 3.0, true, 48000.0);
        assert!(out[..163].iter().all(|f| *f == [0.0; 2]));
        assert_eq!(&out[163..], &quiet[..2000 - 163]);
        // Noise, whose peaks are between the samples as often as not and
        // which keeps the gain moving, from 3 to 36 dB over the ceiling.
        for over in [3.0, 12.0, 36.0] {
            let x = noise(99, amplitude(over - 1.0), 480000);
            let plain = between_db(&limited(&x, -1.0, 60.0, 3.0, false, 48000.0));
            assert!(plain > 0.5, "{plain}");
            for (lookahead, release, rate) in [(1.0, 60.0, 48000.0), (3.0, 60.0, 48000.0), (3.0, 5.0, 44100.0), (20.0, 200.0, 48000.0)] {
                let y = limited(&x, -1.0, release, lookahead, true, rate);
                assert!(between_db(&y) <= -1.0, "{over} dB over, {lookahead} ms: {}", between_db(&y));
                assert!(peak(&y) <= amplitude(-1.0));
            }
            // With the shortest look-ahead the gain moves fastest across a
            // point's samples, and the estimate passes by hundredths.
            let y = limited(&x, -1.0, 1.0, 0.5, true, 44100.0);
            assert!(between_db(&y) < -1.0 + 0.03, "{over} dB over: {}", between_db(&y));
        }
        // A lone peak between two samples comes out at the ceiling less the
        // margin: the gain is the same across the samples its estimate reads.
        let mut click = vec![[0.0; 2]; 4000];
        (click[2000], click[2001]) = ([1.5, 0.0], [1.5, 0.0]);
        let y = limited(&click, -1.0, 60.0, 3.0, true, 48000.0);
        assert!((between_db(&y) + 1.0 + TRUE_PEAK_MARGIN_DB).abs() < 1e-6, "{}", between_db(&y));
    }

    #[test]
    fn dynamics_do_not_depend_on_blocks() {
        let x: Vec<Frame> = sine(1.3, 30000).iter().enumerate().map(|(i, f)| [f[0] * (1.0 + (i % 7000) as f64 / 7000.0) / 2.0, f[1]]).collect();
        let devices = || (compressor(-20.0, 6.0, 8.0, 3.0), Limiter::new(-3.0, 40.0, 1.5, false, 48000.0), Limiter::new(-6.0, 40.0, 1.5, true, 48000.0));
        let mut whole = x.clone();
        let (mut c, mut l, mut t) = devices();
        for chunk in whole.chunks_mut(4096) {
            c.process(chunk, None, clock());
            l.process(chunk, clock());
            t.process(chunk, clock());
        }
        let mut pieces = x.clone();
        let (mut c, mut l, mut t) = devices();
        let (mut at, mut size) = (0, 1);
        while at < pieces.len() {
            let n = size.min(pieces.len() - at);
            c.process(&mut pieces[at..at + n], None, clock());
            l.process(&mut pieces[at..at + n], clock());
            t.process(&mut pieces[at..at + n], clock());
            at += n;
            size = size * 5 % 997 + 1;
        }
        assert_eq!(whole, pieces);
        // Limiters that take over from others go on as those would have.
        let mut halves = x.clone();
        let (mut c, mut l, mut t) = devices();
        for chunk in halves[..12288].chunks_mut(4096) {
            c.process(chunk, None, clock());
            l.process(chunk, clock());
            t.process(chunk, clock());
        }
        let (_, mut l2, mut t2) = devices();
        l2.take_over(&mut l);
        t2.take_over(&mut t);
        for chunk in halves[12288..].chunks_mut(4096) {
            c.process(chunk, None, clock());
            l2.process(chunk, clock());
            t2.process(chunk, clock());
        }
        assert_eq!(whole, halves);
    }
}

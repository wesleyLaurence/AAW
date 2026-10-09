//! The clipper: the signal driven by `drive_db` into a ceiling no sample
//! passes, each channel by itself and each sample by its own level, with no
//! look-ahead and no release. `knee_db` is how far under the ceiling the
//! curve starts to bend; at 0 it is a hard clip.
//!
//! Oversampled, the curve is applied between the samples too, and what it
//! takes off is brought back to the song's rate through a lowpass, so that
//! the harmonics over half the rate do not fold back under it. Only what is
//! taken off goes through the filters: the signal itself is delayed and
//! otherwise untouched, and a signal under the knee comes out bit for bit.
//! The lowpass rings past the ceiling, so a last clip at the song's rate
//! holds it.

use crate::dynamics::Reductions;
use crate::envelope::Knob;
use crate::resample::i0;
use crate::{Clock, Frame};
use std::f64::consts::PI;

/// Frames of the song the oversampling kernel reaches either side of its
/// middle. An oversampled clipper is this late twice, once up and once down.
/// A kernel four times as long folds back no less: the last clip decides.
pub const HALF: usize = 8;

/// The Kaiser window's beta: about 60 dB under the passband past the
/// transition, which is 6 kHz either side of half the rate at 48 kHz.
const BETA: f64 = 6.0;

#[inline]
fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The curve, for a level at or over zero: as it came up to `start`, and
/// from there bent towards `ceiling`, which it nears and does not pass.
#[inline]
pub fn curve(level: f64, ceiling: f64, start: f64) -> f64 {
    if level <= start {
        level
    } else if start >= ceiling {
        ceiling
    } else {
        let room = ceiling - start;
        (start + room * ((level - start) / room).tanh()).min(ceiling)
    }
}

#[inline]
fn clip(x: f64, ceiling: f64, start: f64) -> f64 {
    curve(x.abs(), ceiling, start).copysign(x)
}

/// What the curve takes off a frame's louder channel, in dB.
#[inline]
fn taken(frame: Frame, ceiling: f64, start: f64) -> f64 {
    let level = frame[0].abs().max(frame[1].abs());
    if level > start {
        20.0 * (level / curve(level, ceiling, start)).log10()
    } else {
        0.0
    }
}

/// A frame driven in, with the ceiling and the knee's start it came with.
#[derive(Clone, Copy, Debug)]
struct Held {
    frame: Frame,
    ceiling: f64,
    start: f64,
}

/// The curve at `factor` times the song's rate.
#[derive(Clone, Debug)]
struct Oversampler {
    factor: usize,
    /// The interpolation kernel's phases after the first, `2 * HALF` taps
    /// each, every phase summing to one. The first phase is the sample.
    up: Vec<f64>,
    /// The decimation kernel, `2 * HALF * factor + 1` taps summing to one.
    down: Vec<f64>,
    /// The last frames driven in: a ring.
    input: Vec<Held>,
    /// What the curve took off the last oversampled samples: a ring.
    delta: Vec<Frame>,
    /// Frames taken in.
    frames: usize,
    /// Oversampled samples since the curve last took anything off.
    idle: usize,
}

impl Oversampler {
    fn new(factor: usize) -> Oversampler {
        let reach = (HALF * factor) as f64;
        // A windowed sinc with its zeros at every sample of the song but the
        // middle one, so interpolation leaves the samples as they are.
        let tap = |offset: f64| {
            let t = offset / factor as f64;
            let sinc = if offset == 0.0 { 1.0 } else { (PI * t).sin() / (PI * t) };
            let r = offset / reach;
            sinc * i0(BETA * (1.0 - r * r).max(0.0).sqrt()) / i0(BETA)
        };
        let mut up = Vec::with_capacity((factor - 1) * 2 * HALF);
        for phase in 1..factor {
            let taps: Vec<f64> = (0..2 * HALF).map(|i| tap((i as f64 - HALF as f64 + 1.0) * factor as f64 - phase as f64)).collect();
            let sum: f64 = taps.iter().sum();
            up.extend(taps.iter().map(|t| t / sum));
        }
        let mut down: Vec<f64> = (0..=2 * HALF * factor)
            .map(|i| if i % factor == 0 && i != HALF * factor { 0.0 } else { tap(i as f64 - reach) })
            .collect();
        let sum: f64 = down.iter().sum();
        down.iter_mut().for_each(|t| *t /= sum);
        Oversampler {
            factor,
            up,
            down,
            input: vec![Held { frame: [0.0; 2], ceiling: 1.0, start: 1.0 }; (2 * HALF + 1).next_power_of_two()],
            delta: vec![[0.0; 2]; (2 * HALF * factor + factor).next_power_of_two()],
            frames: 0,
            idle: usize::MAX,
        }
    }

    /// Takes a frame in and gives the frame `2 * HALF` before it, clipped,
    /// with what the curve took off its louder channel.
    #[inline]
    fn tick(&mut self, held: Held) -> (Frame, f64) {
        let (factor, n) = (self.factor, self.frames);
        let (im, dm) = (self.input.len() - 1, self.delta.len() - 1);
        self.input[n & im] = held;
        self.frames = n.wrapping_add(1);
        // The curve between the samples, as they were `HALF` frames ago.
        let middle = self.input[n.wrapping_sub(HALF) & im];
        let base = n.wrapping_mul(factor);
        let first = n.wrapping_sub(2 * HALF - 1);
        for phase in 0..factor {
            let mut u = middle.frame;
            if phase > 0 {
                u = [0.0; 2];
                let taps = &self.up[(phase - 1) * 2 * HALF..phase * 2 * HALF];
                for (i, tap) in taps.iter().enumerate() {
                    let x = self.input[first.wrapping_add(i) & im].frame;
                    u[0] += x[0] * tap;
                    u[1] += x[1] * tap;
                }
            }
            let d = [clip(u[0], middle.ceiling, middle.start) - u[0], clip(u[1], middle.ceiling, middle.start) - u[1]];
            self.idle = if d == [0.0; 2] { self.idle.saturating_add(1) } else { 0 };
            self.delta[base.wrapping_add(phase) & dm] = d;
        }
        // What it took off, under half the song's rate, for the frame
        // another `HALF` back.
        let out = self.input[n.wrapping_sub(2 * HALF) & im];
        let mut y = out.frame;
        if self.idle < 2 * HALF * factor + factor {
            let oldest = base.wrapping_sub(2 * HALF * factor);
            let mut d = [0.0; 2];
            for (i, tap) in self.down.iter().enumerate() {
                let x = self.delta[oldest.wrapping_add(i) & dm];
                d[0] += x[0] * tap;
                d[1] += x[1] * tap;
            }
            // The lowpass rings past the ceiling; this holds it.
            y = [(y[0] + d[0]).clamp(-out.ceiling, out.ceiling), (y[1] + d[1]).clamp(-out.ceiling, out.ceiling)];
        }
        (y, taken(out.frame, out.ceiling, out.start))
    }
}

#[derive(Clone, Debug)]
pub struct Clipper {
    over: Option<Oversampler>,
    pub ceiling: Knob,
    pub drive: Knob,
    pub knee: Knob,
    values: [Vec<f64>; 3],
    pub reduction: Reductions,
}

impl Clipper {
    /// Frames by which a clipper at this oversampling delays its input.
    pub fn latency(oversample: i64) -> usize {
        if oversample > 1 { 2 * HALF } else { 0 }
    }

    pub fn new(oversample: i64, ceiling: Knob, drive: Knob, knee: Knob, max_block: usize) -> Clipper {
        Clipper {
            over: (oversample > 1).then(|| Oversampler::new(oversample as usize)),
            ceiling,
            drive,
            knee,
            values: [0; 3].map(|_| vec![0.0; max_block]),
            reduction: Reductions::default(),
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        let [ceilings, drives, knees] = &mut self.values;
        self.ceiling.fill(clock.frame, clock.step, &mut ceilings[..n]);
        self.drive.fill(clock.frame, clock.step, &mut drives[..n]);
        self.knee.fill(clock.frame, clock.step, &mut knees[..n]);
        let late = Clipper::latency(self.over.as_ref().map_or(1, |o| o.factor as i64)) as i64;
        for (i, frame) in x.iter_mut().enumerate() {
            let (gain, ceiling) = (amplitude(drives[i]), amplitude(ceilings[i]));
            let start = ceiling * amplitude(-knees[i]);
            let driven = [frame[0] * gain, frame[1] * gain];
            let (out, taken) = match &mut self.over {
                Some(over) => over.tick(Held { frame: driven, ceiling, start }),
                None => ([clip(driven[0], ceiling, start), clip(driven[1], ceiling, start)], taken(driven, ceiling, start)),
            };
            self.reduction.record(taken, clock.frame + i as i64 * clock.step - late, &clock);
            *frame = out;
        }
    }

    /// Continues from `old`: its filters ring on when it oversampled as this
    /// one does, and the knobs glide to their own values.
    pub fn take_over(&mut self, old: &mut Clipper) {
        if let (Some(new), Some(was)) = (&mut self.over, &mut old.over) {
            if new.factor == was.factor {
                std::mem::swap(&mut new.input, &mut was.input);
                std::mem::swap(&mut new.delta, &mut was.delta);
                new.frames = was.frames;
                new.idle = was.idle;
            }
        }
        self.ceiling.take_over(&old.ceiling);
        self.drive.take_over(&old.drive);
        self.knee.take_over(&old.knee);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Param;

    fn clock() -> Clock {
        Clock {
            frame: 0,
            step: 1,
            total: 1 << 40,
        }
    }

    fn clipper(oversample: i64, ceiling: f64, drive: f64, knee: f64) -> Clipper {
        let knob = |v: f64| Knob::new(Param::fixed(v), 240);
        Clipper::new(oversample, knob(ceiling), knob(drive), knob(knee), 4096)
    }

    fn tone(hz: f64, level: f64, frames: usize) -> Vec<Frame> {
        (0..frames).map(|i| [level * (2.0 * PI * hz * i as f64 / 48000.0).sin(); 2]).collect()
    }

    fn through(mut c: Clipper, x: &[Frame]) -> (Vec<Frame>, Clipper) {
        let mut y = x.to_vec();
        for chunk in y.chunks_mut(4096) {
            c.process(chunk, clock());
        }
        (y, c)
    }

    fn peak(x: &[Frame]) -> f64 {
        x.iter().fold(0.0, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    /// The power of `x`'s left channel at `hz`, over a whole number of its
    /// cycles.
    fn power_at(x: &[Frame], hz: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, f) in x.iter().enumerate() {
            let a = 2.0 * PI * hz * i as f64 / 48000.0;
            re += f[0] * a.cos();
            im += f[0] * a.sin();
        }
        2.0 * (re * re + im * im) / (x.len() as f64).powi(2)
    }

    /// How far under the whole the part of a clipped tone is that is not
    /// the tone or a harmonic of it under half the rate, in dB: what folded
    /// back. A second of the tone, after a second to settle.
    fn folded_db(oversample: i64, hz: f64, over_db: f64) -> f64 {
        let x = tone(hz, amplitude(over_db), 96000);
        let (y, _) = through(clipper(oversample, 0.0, 0.0, 0.0), &x);
        let y = &y[48000..];
        let total = y.iter().map(|f| f[0] * f[0]).sum::<f64>() / y.len() as f64;
        let harmonics: f64 = (1..).map(|k| k as f64 * hz).take_while(|f| *f < 24000.0).map(|f| power_at(y, f)).sum();
        10.0 * ((total - harmonics).max(1e-30) / total).log10()
    }

    #[test]
    fn a_hard_clip_holds_its_ceiling_and_leaves_what_is_under_it() {
        let ceiling = amplitude(-1.0);
        let loud = tone(997.0, 1.6, 48000);
        let (y, c) = through(clipper(1, -1.0, 0.0, 0.0), &loud);
        assert_eq!(peak(&y), ceiling);
        // A sample under the ceiling is the sample, and one over it the ceiling.
        for (a, b) in loud.iter().zip(&y) {
            assert_eq!(b[0], if a[0].abs() <= ceiling { a[0] } else { ceiling.copysign(a[0]) });
        }
        // What it took off the loudest sample, and how often it took 1 dB.
        let over = 20.0 * (1.6 / ceiling).log10();
        assert!((c.reduction.whole.max - over).abs() < 0.01, "{}", c.reduction.whole.max);
        assert!(c.reduction.whole.fraction_over_1db() > 0.5 && c.reduction.whole.fraction_over_1db() < 0.7);
        // Under the ceiling nothing is changed or delayed, and nothing is taken.
        let quiet = tone(997.0, 0.5, 4096);
        let (same, c) = through(clipper(1, -1.0, 0.0, 0.0), &quiet);
        assert_eq!(same, quiet);
        assert_eq!(c.reduction.whole.max, 0.0);
        // The drive is a gain into the ceiling.
        let (driven, _) = through(clipper(1, -1.0, 12.0, 0.0), &tone(997.0, 0.4, 4096));
        let (plain, _) = through(clipper(1, -1.0, 0.0, 0.0), &tone(997.0, 0.4 * amplitude(12.0), 4096));
        assert!(driven.iter().zip(&plain).all(|(a, b)| (a[0] - b[0]).abs() < 1e-12));
        assert_eq!(peak(&driven), ceiling);
    }

    #[test]
    fn a_knee_bends_the_curve_under_the_ceiling() {
        let ceiling = amplitude(-3.0);
        let start = ceiling * amplitude(-6.0);
        // As it came up to the knee, and from there under the hard clip,
        // rising, without a step, towards a ceiling it does not pass.
        assert_eq!(curve(start * 0.9, ceiling, start), start * 0.9);
        assert!((curve(start * 1.0001, ceiling, start) - start * 1.0001).abs() < 1e-9);
        let mut last = start;
        for i in 1..2000 {
            let level = start + i as f64 * 0.01;
            let y = curve(level, ceiling, start);
            assert!(y >= last && (y > last || level > 2.0), "{level}");
            assert!(y <= ceiling && y <= level && y <= curve(level, ceiling, ceiling));
            last = y;
        }
        assert!(curve(ceiling, ceiling, start) < ceiling * 0.9);
        assert!(curve(8.0, ceiling, start) > ceiling * 0.999);
        // Through the device, each channel by itself.
        let x: Vec<Frame> = tone(997.0, 1.0, 4096).iter().map(|f| [f[0], f[1] * 0.2]).collect();
        let (y, c) = through(clipper(1, -3.0, 0.0, 6.0), &x);
        for (a, b) in x.iter().zip(&y) {
            assert_eq!(b[0], curve(a[0].abs(), ceiling, start).copysign(a[0]));
            assert_eq!(b[1], a[1]);
        }
        let most = 20.0 * (peak(&x) / curve(peak(&x), ceiling, start)).log10();
        assert!((c.reduction.whole.max - most).abs() < 1e-9);
    }

    #[test]
    fn oversampled_it_is_late_holds_the_ceiling_and_folds_less_back() {
        assert_eq!((Clipper::latency(1), Clipper::latency(2), Clipper::latency(4)), (0, 16, 16));
        for factor in [2, 4] {
            // Under the ceiling the signal comes out as it went in, late.
            let quiet = tone(997.0, 0.5, 4096);
            let (y, c) = through(clipper(factor, -1.0, 0.0, 0.0), &quiet);
            assert!(y[..16].iter().all(|f| *f == [0.0; 2]));
            assert_eq!(&y[16..], &quiet[..4096 - 16]);
            assert_eq!(c.reduction.whole.max, 0.0);
            // Over it no sample passes, and the peaks are near it.
            let (y, c) = through(clipper(factor, -1.0, 0.0, 0.0), &tone(997.0, 1.6, 48000));
            assert!(peak(&y) <= amplitude(-1.0) && peak(&y) > amplitude(-1.0) * 0.99, "{}", peak(&y));
            assert!((c.reduction.whole.max - 20.0 * (1.6 / amplitude(-1.0)).log10()).abs() < 0.01);
            let (y, _) = through(clipper(factor, -6.0, 3.0, 9.0), &tone(4001.0, 2.0, 48000));
            assert!(peak(&y) <= amplitude(-6.0));
        }
        // What folds back under half the rate, in dB under the whole: a tone
        // of 1 kHz 6 dB over the ceiling, and one of 6 kHz 3 dB over it.
        let low = [1, 2, 4].map(|f| folded_db(f, 1001.0, 6.0));
        let high = [1, 2, 4].map(|f| folded_db(f, 6001.0, 3.0));
        assert!(low[0] > -48.0 && low[1] < -56.0 && low[2] < -57.0, "{low:?}");
        assert!(high[0] > -30.0 && high[1] < -42.0 && high[2] < -48.0, "{high:?}");
        // A tone at a quarter of the rate whose samples lie under the
        // ceiling and whose crests, between them, lie over it: left alone at
        // the song's rate, clipped at four times it.
        let between: Vec<Frame> = (0..4096).map(|i| [1.2 * (PI * (i as f64 / 2.0 + 0.25)).sin(); 2]).collect();
        assert!(peak(&between) < amplitude(-1.0));
        assert_eq!(through(clipper(1, -1.0, 0.0, 0.0), &between).0, between);
        let (y, _) = through(clipper(4, -1.0, 0.0, 0.0), &between);
        assert!(y[1000..].iter().zip(&between[1000 - 16..]).all(|(a, b)| a[0].abs() < b[0].abs() - 0.02));
    }

    #[test]
    fn the_clipper_does_not_depend_on_blocks_and_takes_over() {
        let x: Vec<Frame> = tone(1500.0, 1.4, 30000).iter().enumerate().map(|(i, f)| [f[0] * (1.0 + (i % 7000) as f64 / 7000.0) / 2.0, f[1]]).collect();
        for factor in [1, 4] {
            let (whole, _) = through(clipper(factor, -3.0, 2.0, 4.0), &x);
            let mut pieces = x.clone();
            let mut c = clipper(factor, -3.0, 2.0, 4.0);
            let (mut at, mut size) = (0, 1);
            while at < pieces.len() {
                let n = size.min(pieces.len() - at);
                c.process(&mut pieces[at..at + n], clock());
                at += n;
                size = size * 5 % 997 + 1;
            }
            assert_eq!(whole, pieces);
            // One that takes over from another goes on as it would have.
            let mut halves = x.clone();
            let mut first = clipper(factor, -3.0, 2.0, 4.0);
            for chunk in halves[..12288].chunks_mut(4096) {
                first.process(chunk, clock());
            }
            let mut second = clipper(factor, -3.0, 2.0, 4.0);
            second.take_over(&mut first);
            for chunk in halves[12288..].chunks_mut(4096) {
                second.process(chunk, clock());
            }
            assert_eq!(whole, halves);
        }
    }
}

//! The level between the samples: the estimate of a true peak that a
//! render's report and `daw listen` give, four times oversampled through the
//! filter of `resample_poly(x, 4, 1)`, here a frame at a time.

use crate::resample::firwin_kaiser;
use crate::Frame;

/// Frames past a sample that the three points after it read; they read one
/// fewer before it.
pub const REACH: usize = 10;

/// Taps of each point between two samples.
const TAPS: usize = 2 * REACH;

const RING: usize = 32;

/// The three points between each sample and the next, and the sample as the
/// filter leaves it.
#[derive(Clone, Debug)]
pub struct Between {
    /// A phase's taps, over the samples from `REACH - 1` before to `REACH`
    /// after the sample the points follow.
    phases: [[f64; TAPS]; 3],
    /// What the filter makes of a sample itself: a gain a hair off one.
    center: f64,
    recent: [Frame; RING],
    frames: usize,
}

impl Default for Between {
    fn default() -> Between {
        Between::new()
    }
}

impl Between {
    pub fn new() -> Between {
        // `resample_poly`'s filter for four times the rate: 81 taps, whose
        // output `4m + p` is the sum of `h[40 + p - 4d] * x[m + d]`.
        let h: Vec<f64> = firwin_kaiser(8 * REACH + 1, 0.25, 5.0).iter().map(|t| t * 4.0).collect();
        let mut phases = [[0.0; TAPS]; 3];
        for (p, taps) in phases.iter_mut().enumerate() {
            for (i, tap) in taps.iter_mut().enumerate() {
                *tap = h[4 * REACH + p + 1 + 4 * (REACH - 1) - 4 * i];
            }
        }
        Between {
            phases,
            center: h[4 * REACH],
            recent: [[0.0; 2]; RING],
            frames: 0,
        }
    }

    /// Takes a frame in and gives the highest level, of either channel, from
    /// the sample `REACH` frames before it up to the next: the sample, what
    /// the filter makes of it, and the three points after it.
    #[inline]
    pub fn tick(&mut self, frame: Frame) -> f64 {
        let peaks = self.peaks(frame);
        peaks[0].max(peaks[1])
    }

    /// As `tick`, each channel by itself.
    #[inline]
    pub fn peaks(&mut self, frame: Frame) -> [f64; 2] {
        let n = self.frames;
        self.recent[n & (RING - 1)] = frame;
        self.frames = n.wrapping_add(1);
        let first = n.wrapping_sub(TAPS - 1);
        let at = self.recent[n.wrapping_sub(REACH) & (RING - 1)];
        let mut level = [at[0].abs().max((at[0] * self.center).abs()), at[1].abs().max((at[1] * self.center).abs())];
        for taps in &self.phases {
            let mut y = [0.0; 2];
            for (i, tap) in taps.iter().enumerate() {
                let x = self.recent[first.wrapping_add(i) & (RING - 1)];
                y[0] += x[0] * tap;
                y[1] += x[1] * tap;
            }
            level = [level[0].max(y[0].abs()), level[1].max(y[1].abs())];
        }
        level
    }

    /// Continues from where `old` was.
    pub fn take_over(&mut self, old: &Between) {
        self.recent = old.recent;
        self.frames = old.frames;
    }
}

/// The highest of the levels between the samples of `x` followed by
/// silence: what the render's report gives as its estimated true peak.
pub fn true_peak(x: &[Frame]) -> f64 {
    let mut between = Between::new();
    let mut peak: f64 = 0.0;
    for (i, frame) in x.iter().copied().chain(std::iter::repeat_n([0.0; 2], REACH)).enumerate() {
        let level = between.tick(frame);
        if i >= REACH {
            peak = peak.max(level);
        }
    }
    peak
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resample::Resampler;
    use std::f64::consts::PI;

    #[test]
    fn it_is_the_reports_estimate_a_frame_at_a_time() {
        // Noise, a tone at a quarter of the rate between its crests, and a
        // step: each against the resampler the render's report measures with.
        let mut seed = 7u64;
        let mut noise = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        };
        let mut x: Vec<Frame> = (0..3000).map(|_| [noise(), noise() * 0.5]).collect();
        x.extend((0..500).map(|i| [0.9 * (PI * (i as f64 / 2.0 + 0.25)).sin(), 0.0]));
        x.extend((0..300).map(|i| if i < 150 { [0.0, 0.8] } else { [0.0, -0.8] }));
        let resampler = Resampler::new(4, 1);
        let column = |c: usize| resampler.apply(&x.iter().map(|f| f[c]).collect::<Vec<_>>());
        let (left, right) = (column(0), column(1));
        let mut between = Between::new();
        let mut levels = Vec::new();
        for frame in x.iter().copied().chain(std::iter::repeat_n([0.0; 2], REACH)) {
            levels.push(between.peaks(frame));
        }
        // The level `REACH` frames after frame `m` went in is that of the
        // four points from `4m`.
        for m in 0..x.len() {
            for (c, over) in [&left, &right].iter().enumerate() {
                let want = over[4 * m..4 * m + 4].iter().fold(x[m][c].abs(), |a, v| a.max(v.abs()));
                let got = levels[m + REACH][c];
                assert!((got - want).abs() < 1e-12, "frame {m} channel {c}: {got} against {want}");
            }
        }
        let whole = left.iter().chain(&right).fold(0.0f64, |a, v| a.max(v.abs()));
        assert!((true_peak(&x) - whole).abs() < 1e-12);
        // Between its samples at 0.707 the tone reaches its 0.9.
        let tone: Vec<Frame> = (0..2000).map(|i| [0.9 * (PI * (i as f64 / 2.0 + 0.25)).sin(); 2]).collect();
        assert!((true_peak(&tone[..]) / 0.9 - 1.0).abs() < 0.02, "{}", true_peak(&tone));
    }
}

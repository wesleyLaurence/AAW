//! Saturation: the signal driven into a curve, trimmed and mixed under the
//! dry signal. `soft` is `tanh`, `hard` a clip at full scale and `tube` an
//! asymmetric curve, whose even harmonics leave a DC offset that a one-pole
//! highpass at 10 Hz takes out.

use crate::envelope::Knob;
use crate::{Clock, Frame};
use aaw_model::SaturationMode;
use std::f64::consts::PI;

#[derive(Clone, Debug)]
pub struct Saturation {
    mode: SaturationMode,
    /// The DC blocker's last input and output, per channel.
    dc_in: [f64; 2],
    dc_out: [f64; 2],
    /// The blocker's pole.
    r: f64,
    pub drive: Knob,
    pub output: Knob,
    pub mix: Knob,
    values: [Vec<f64>; 3],
}

#[inline]
fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The curve: the shaped value of a sample driven to `x`.
#[inline]
pub fn shape(mode: SaturationMode, x: f64) -> f64 {
    match mode {
        SaturationMode::Soft => x.tanh(),
        SaturationMode::Hard => x.clamp(-1.0, 1.0),
        // A valve's bend: the positive half compresses sooner than the
        // negative, which adds even harmonics and an offset.
        SaturationMode::Tube => (x + 0.3 * x * x).tanh(),
    }
}

impl Saturation {
    pub fn new(mode: SaturationMode, drive: Knob, output: Knob, mix: Knob, rate: f64, max_block: usize) -> Saturation {
        Saturation {
            mode,
            dc_in: [0.0; 2],
            dc_out: [0.0; 2],
            r: 1.0 - 2.0 * PI * 10.0 / rate,
            drive,
            output,
            mix,
            values: [0; 3].map(|_| vec![0.0; max_block]),
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        let [drives, outputs, mixes] = &mut self.values;
        self.drive.fill(clock.frame, clock.step, &mut drives[..n]);
        self.output.fill(clock.frame, clock.step, &mut outputs[..n]);
        self.mix.fill(clock.frame, clock.step, &mut mixes[..n]);
        for i in 0..n {
            let (gain, trim, mix) = (amplitude(drives[i]), amplitude(outputs[i]), mixes[i] / 100.0);
            let frame = x[i];
            let mut wet = [shape(self.mode, frame[0] * gain), shape(self.mode, frame[1] * gain)];
            if self.mode == SaturationMode::Tube {
                for c in 0..2 {
                    let y = wet[c] - self.dc_in[c] + self.r * self.dc_out[c];
                    self.dc_in[c] = wet[c];
                    self.dc_out[c] = y;
                    wet[c] = y;
                }
            }
            x[i] = [frame[0] * (1.0 - mix) + wet[0] * trim * mix, frame[1] * (1.0 - mix) + wet[1] * trim * mix];
        }
    }

    /// Continues from `old`: the blocker's state carries on and the knobs
    /// glide to their own values.
    pub fn take_over(&mut self, old: &Saturation) {
        self.dc_in = old.dc_in;
        self.dc_out = old.dc_out;
        self.drive.take_over(&old.drive);
        self.output.take_over(&old.output);
        self.mix.take_over(&old.mix);
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

    fn saturation(mode: SaturationMode, drive: f64, output: f64, mix: f64) -> Saturation {
        let knob = |v: f64| Knob::new(Param::fixed(v), 240);
        Saturation::new(mode, knob(drive), knob(output), knob(mix), 48000.0, 4096)
    }

    /// The amplitude of harmonic `k` of a 100 Hz tone in `x`, over a second.
    fn harmonic(x: &[Frame], k: usize) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, f) in x.iter().enumerate() {
            let a = 2.0 * PI * 100.0 * k as f64 * i as f64 / 48000.0;
            re += f[0] * a.cos();
            im += f[0] * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / x.len() as f64
    }

    fn tone() -> Vec<Frame> {
        (0..48000).map(|i| [0.5 * (2.0 * PI * 100.0 * i as f64 / 48000.0).sin(); 2]).collect()
    }

    /// `tone()` through a saturation, in blocks.
    fn shaped(mut s: Saturation) -> Vec<Frame> {
        let mut x = tone();
        for chunk in x.chunks_mut(4096) {
            s.process(chunk, clock());
        }
        x
    }

    #[test]
    fn soft_adds_odd_harmonics_hard_clips_and_tube_adds_even_ones_without_dc() {
        let soft = shaped(saturation(SaturationMode::Soft, 12.0, 0.0, 100.0));
        assert!(harmonic(&soft, 3) > 0.05 && harmonic(&soft, 2) < 1e-6, "{} {}", harmonic(&soft, 3), harmonic(&soft, 2));
        assert!(soft.iter().all(|f| f[0].abs() <= 1.0));
        let hard = shaped(saturation(SaturationMode::Hard, 12.0, 0.0, 100.0));
        assert!(hard.iter().any(|f| f[0] == 1.0) && hard.iter().all(|f| f[0].abs() <= 1.0));
        let tube = shaped(saturation(SaturationMode::Tube, 12.0, 0.0, 100.0));
        assert!(harmonic(&tube, 2) > 0.02, "{}", harmonic(&tube, 2));
        let mean = tube[24000..].iter().map(|f| f[0]).sum::<f64>() / 24000.0;
        assert!(mean.abs() < 0.01, "DC {mean}");
        // No drive and no mix leave the signal alone; the output trims it.
        let same = shaped(saturation(SaturationMode::Soft, 12.0, 0.0, 0.0));
        assert_eq!(same, tone());
        let quiet = shaped(saturation(SaturationMode::Hard, 0.0, -6.0, 100.0));
        assert!((quiet[120][0] - tone()[120][0] * amplitude(-6.0)).abs() < 1e-12);
    }
}

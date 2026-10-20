//! The overdrive: the signal driven by `drive_db` into a curve harder than
//! the saturation's soft one, a lowpass at `tone_hz` after it, the result
//! trimmed by `output_db` and mixed under the dry signal.
//!
//! The curve is the cubic soft clip, `1.5x - 0.5x³` up to full scale and
//! flat from there: it reaches full scale where `tanh` is three quarters of
//! the way, and its bend ends in a corner, so it is harder, with more of the
//! upper harmonics, and still rounds the top where the saturation's `hard`
//! mode cuts it. The curve is symmetric, so it leaves no offset. The tone is
//! a second-order lowpass after the curve, the filter automation moves, so
//! `tone_hz` is a lane target; it takes the fizz the curve makes off the
//! top. The curve's top is full scale, so a driven signal comes out near
//! `output_db`: at it with the tone open, and up to 2 dB over it with the
//! tone low, which leaves a square's fundamental, louder than the square.

use crate::envelope::{Knob, Param};
use crate::svf::{Shape, Svf};
use crate::{Clock, Frame};

#[derive(Clone, Debug)]
pub struct Overdrive {
    tone: Svf,
    pub drive: Knob,
    pub output: Knob,
    pub mix: Knob,
    values: [Vec<f64>; 3],
    /// The driven signal on its way through the tone filter.
    wet: Vec<Frame>,
}

#[inline]
fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The curve: the shaped value of a sample driven to `x`.
#[inline]
pub fn shape(x: f64) -> f64 {
    if x >= 1.0 {
        1.0
    } else if x <= -1.0 {
        -1.0
    } else {
        1.5 * x - 0.5 * x * x * x
    }
}

impl Overdrive {
    /// `tone` is the lowpass's cutoff in Hz, as given or as a lane moves it.
    pub fn new(drive: Knob, tone: Param, output: Knob, mix: Knob, rate: f64, max_block: usize) -> Overdrive {
        Overdrive {
            tone: Svf::new(Shape::Filter { highpass: false, order: 2 }, vec![tone], rate),
            drive,
            output,
            mix,
            values: [0; 3].map(|_| vec![0.0; max_block]),
            wet: vec![[0.0; 2]; max_block],
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        let [drives, outputs, mixes] = &mut self.values;
        self.drive.fill(clock.frame, clock.step, &mut drives[..n]);
        self.output.fill(clock.frame, clock.step, &mut outputs[..n]);
        self.mix.fill(clock.frame, clock.step, &mut mixes[..n]);
        let wet = &mut self.wet[..n];
        for i in 0..n {
            let gain = amplitude(drives[i]);
            wet[i] = [shape(x[i][0] * gain), shape(x[i][1] * gain)];
        }
        self.tone.process(wet, clock);
        for i in 0..n {
            let (trim, mix) = (amplitude(outputs[i]), mixes[i] / 100.0);
            let frame = x[i];
            x[i] = [frame[0] * (1.0 - mix) + wet[i][0] * trim * mix, frame[1] * (1.0 - mix) + wet[i][1] * trim * mix];
        }
    }

    /// Continues from `old`: the tone filter's state carries on and the
    /// knobs glide to their own values.
    pub fn take_over(&mut self, old: &Overdrive) {
        self.tone.take_over(&old.tone);
        self.drive.take_over(&old.drive);
        self.output.take_over(&old.output);
        self.mix.take_over(&old.mix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn clock() -> Clock {
        Clock {
            frame: 0,
            step: 1,
            total: 1 << 40,
        }
    }

    fn overdrive(drive: f64, tone: f64, output: f64, mix: f64) -> Overdrive {
        let knob = |v: f64| Knob::new(Param::fixed(v), 240);
        Overdrive::new(knob(drive), Param::fixed(tone), knob(output), knob(mix), 48000.0, 4096)
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

    fn tone(level: f64, frames: usize) -> Vec<Frame> {
        (0..frames).map(|i| [level * (2.0 * PI * 100.0 * i as f64 / 48000.0).sin(); 2]).collect()
    }

    fn peak(x: &[Frame]) -> f64 {
        x.iter().fold(0.0, |m, f| m.max(f[0].abs()).max(f[1].abs()))
    }

    /// A second of a 100 Hz tone at `level` through an overdrive, in blocks.
    fn through(mut d: Overdrive, level: f64) -> Vec<Frame> {
        let mut x = tone(level, 48000);
        for chunk in x.chunks_mut(4096) {
            d.process(chunk, clock());
        }
        x
    }

    #[test]
    fn the_curve_is_harder_than_tanh_and_reaches_full_scale() {
        assert_eq!((shape(0.0), shape(1.0), shape(-1.0), shape(3.0), shape(-3.0)), (0.0, 1.0, -1.0, 1.0, -1.0));
        let mut last = 0.0;
        for i in 1..=1000 {
            let x = i as f64 / 1000.0;
            let y = shape(x);
            // Rising, odd, under the line and over tanh all the way up.
            assert!(y > last && y <= 1.5 * x && y > x.tanh() && shape(-x) == -y, "{x}");
            last = y;
        }
        assert!((shape(0.5) - 0.6875).abs() < 1e-12);
    }

    #[test]
    fn drive_adds_odd_harmonics_the_tone_takes_the_upper_ones_and_the_trim_sets_the_top() {
        // Driven hard with the tone open, the top is at full scale and the
        // harmonics are odd only.
        let open = through(overdrive(24.0, 20000.0, 0.0, 100.0), 0.5);
        assert!(harmonic(&open, 3) > 0.1 && harmonic(&open, 15) > 0.005, "{} {}", harmonic(&open, 3), harmonic(&open, 15));
        assert!(harmonic(&open, 2) < 1e-6 && harmonic(&open, 4) < 1e-6);
        assert!(peak(&open) > 0.98 && peak(&open) < 1.1, "{}", peak(&open));
        // The tone at 500 Hz leaves the third harmonic and takes the fifteenth,
        // 1.6 octaves over it, by about 19 dB.
        let dark = through(overdrive(24.0, 500.0, 0.0, 100.0), 0.5);
        assert!(harmonic(&dark, 3) > harmonic(&open, 3) * 0.5, "{} {}", harmonic(&dark, 3), harmonic(&open, 3));
        let ratio = harmonic(&dark, 15) / harmonic(&open, 15);
        assert!(ratio > 0.08 && ratio < 0.14, "{ratio}");
        // The output trims the result; a mix of 0 leaves the signal alone.
        let quiet = through(overdrive(24.0, 20000.0, -12.0, 100.0), 0.5);
        assert!((peak(&quiet) - peak(&open) * amplitude(-12.0)).abs() < 0.01, "{}", peak(&quiet));
        assert_eq!(through(overdrive(24.0, 500.0, 6.0, 0.0), 0.5), tone(0.5, 48000));
        // Under the drive a quiet signal is bent a little and not clipped.
        let soft = through(overdrive(0.0, 20000.0, 0.0, 100.0), 0.2);
        assert!(peak(&soft) > 0.28 && peak(&soft) < 0.3 && harmonic(&soft, 3) > 0.0008, "{} {}", peak(&soft), harmonic(&soft, 3));
    }

    #[test]
    fn the_overdrive_does_not_depend_on_blocks_and_takes_over() {
        let x: Vec<Frame> = tone(0.7, 30000).iter().enumerate().map(|(i, f)| [f[0] * (1.0 + (i % 7000) as f64 / 7000.0) / 2.0, f[1]]).collect();
        let mut whole = x.clone();
        let mut one = overdrive(18.0, 3000.0, -3.0, 80.0);
        for chunk in whole.chunks_mut(4096) {
            one.process(chunk, clock());
        }
        let mut pieces = x.clone();
        let mut d = overdrive(18.0, 3000.0, -3.0, 80.0);
        let (mut at, mut size) = (0, 1);
        while at < pieces.len() {
            let n = size.min(pieces.len() - at);
            d.process(&mut pieces[at..at + n], clock());
            at += n;
            size = size * 5 % 997 + 1;
        }
        assert_eq!(whole, pieces);
        // One that takes over from another goes on as it would have.
        let mut halves = x.clone();
        let mut first = overdrive(18.0, 3000.0, -3.0, 80.0);
        for chunk in halves[..12288].chunks_mut(4096) {
            first.process(chunk, clock());
        }
        let mut second = overdrive(18.0, 3000.0, -3.0, 80.0);
        second.take_over(&first);
        for chunk in halves[12288..].chunks_mut(4096) {
            second.process(chunk, clock());
        }
        assert_eq!(whole, halves);
    }
}

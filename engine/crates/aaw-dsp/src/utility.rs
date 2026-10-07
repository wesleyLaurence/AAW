//! The utility: the small channel moves that need no other device, in this
//! order. `invert` flips the polarity of a channel or both; `mono` sums the
//! channels to their average; `mono_below_hz` sums only what lies below it,
//! through a Linkwitz-Riley crossover of 24 dB per octave: each channel's
//! highs plus the lows of the middle, which meet flat, so a bass stays in the
//! middle while the rest keeps its width; `width_percent` scales the side
//! signal; `gain_db` and `pan`, a balance as a track's. Each is left out of
//! the arithmetic at its default, so a utility at rest passes the signal
//! through bit-identical.

use crate::biquad::{butter, Cascade, Section};
use crate::envelope::Knob;
use crate::{Clock, Frame};
use aaw_model::Invert;

#[derive(Clone, Debug)]
pub struct Utility {
    invert: Invert,
    mono: bool,
    /// The crossover, when a frequency is given: the highpass on both
    /// channels and the lowpass on the middle.
    crossover: Option<(Cascade, Cascade)>,
    pub gain: Knob,
    pub pan: Knob,
    pub width: Knob,
    values: [Vec<f64>; 3],
}

#[inline]
fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// A balance: the louder side stays at full level and the other is turned
/// down, as a stereo track's pan does.
#[inline]
pub fn balance(pan: f64) -> [f64; 2] {
    [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)]
}

/// A fourth-order Linkwitz-Riley filter: a second-order Butterworth twice,
/// whose lowpass and highpass sum to an allpass.
fn linkwitz_riley(cutoff_hz: f64, highpass: bool, rate: f64) -> Vec<Section> {
    let section = butter(2, cutoff_hz, highpass, rate);
    [section.clone(), section].concat()
}

impl Utility {
    pub fn new(invert: Invert, mono: bool, mono_below_hz: Option<f64>, gain: Knob, pan: Knob, width: Knob, rate: f64, max_block: usize) -> Utility {
        Utility {
            invert,
            mono,
            // Another frequency is another signature, so the filters never
            // cross over from ones with other coefficients.
            crossover: mono_below_hz.map(|hz| (Cascade::new(linkwitz_riley(hz, true, rate)), Cascade::new(linkwitz_riley(hz, false, rate)))),
            gain,
            pan,
            width,
            values: [0; 3].map(|_| vec![0.0; max_block]),
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        let [gains, pans, widths] = &mut self.values;
        self.gain.fill(clock.frame, clock.step, &mut gains[..n]);
        self.pan.fill(clock.frame, clock.step, &mut pans[..n]);
        self.width.fill(clock.frame, clock.step, &mut widths[..n]);
        for i in 0..n {
            let mut f = x[i];
            match self.invert {
                Invert::None => {}
                Invert::Left => f[0] = -f[0],
                Invert::Right => f[1] = -f[1],
                Invert::Both => f = [-f[0], -f[1]],
            }
            if self.mono {
                let m = (f[0] + f[1]) / 2.0;
                f = [m, m];
            }
            if let Some((highpass, lowpass)) = &mut self.crossover {
                let low = lowpass.tick([(f[0] + f[1]) / 2.0, 0.0])[0];
                let high = highpass.tick(f);
                f = [high[0] + low, high[1] + low];
            }
            let w = widths[i] / 100.0;
            if w != 1.0 {
                let (m, s) = ((f[0] + f[1]) / 2.0, (f[0] - f[1]) / 2.0 * w);
                f = [m + s, m - s];
            }
            if gains[i] != 0.0 {
                let g = amplitude(gains[i]);
                f = [f[0] * g, f[1] * g];
            }
            if pans[i] != 0.0 {
                let [l, r] = balance(pans[i]);
                f = [f[0] * l, f[1] * r];
            }
            x[i] = f;
        }
    }

    /// Continues from `old`: the crossover keeps its state and the knobs
    /// glide to their own values.
    pub fn take_over(&mut self, old: &Utility) {
        if let (Some(new), Some(was)) = (&mut self.crossover, &old.crossover) {
            new.0.take_over(&was.0);
            new.1.take_over(&was.1);
        }
        self.gain.take_over(&old.gain);
        self.pan.take_over(&old.pan);
        self.width.take_over(&old.width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Param;
    use std::f64::consts::PI;

    const RATE: f64 = 48000.0;

    fn clock() -> Clock {
        Clock {
            frame: 0,
            step: 1,
            total: 1 << 40,
        }
    }

    fn utility(invert: Invert, mono: bool, below: Option<f64>, gain: f64, pan: f64, width: f64) -> Utility {
        let knob = |v: f64| Knob::new(Param::fixed(v), 240);
        Utility::new(invert, mono, below, knob(gain), knob(pan), knob(width), RATE, 4096)
    }

    /// A second of two tones: 60 Hz in the middle, 2 kHz on the left alone.
    fn signal() -> Vec<Frame> {
        (0..48000)
            .map(|i| {
                let t = i as f64 / RATE;
                let bass = 0.4 * (2.0 * PI * 60.0 * t).sin();
                let high = 0.3 * (2.0 * PI * 2000.0 * t).sin();
                [bass + high, bass]
            })
            .collect()
    }

    fn through(mut u: Utility, mut x: Vec<Frame>) -> Vec<Frame> {
        for chunk in x.chunks_mut(4096) {
            u.process(chunk, clock());
        }
        x
    }

    /// The RMS of a tone at `hz` in channel `c` over the second half.
    fn level(x: &[Frame], c: usize, hz: f64) -> f64 {
        let (mut re, mut im) = (0.0, 0.0);
        for (i, f) in x.iter().enumerate().skip(24000) {
            let a = 2.0 * PI * hz * i as f64 / RATE;
            re += f[c] * a.cos();
            im += f[c] * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / 24000.0
    }

    #[test]
    fn at_rest_it_passes_the_signal_through_bit_identical() {
        let x = signal();
        assert_eq!(through(utility(Invert::None, false, None, 0.0, 0.0, 100.0), x.clone()), x);
    }

    #[test]
    fn gain_pan_and_invert_are_exact() {
        let x = signal();
        let quiet = through(utility(Invert::None, false, None, -6.0, 0.0, 100.0), x.clone());
        assert!((quiet[100][0] - x[100][0] * amplitude(-6.0)).abs() < 1e-12);
        let left = through(utility(Invert::None, false, None, 0.0, -1.0, 100.0), x.clone());
        assert_eq!(left[100], [x[100][0], 0.0]);
        let half = through(utility(Invert::None, false, None, 0.0, 0.5, 100.0), x.clone());
        assert_eq!(half[100], [x[100][0] * 0.5, x[100][1]]);
        let flipped = through(utility(Invert::Left, false, None, 0.0, 0.0, 100.0), x.clone());
        assert_eq!(flipped[100], [-x[100][0], x[100][1]]);
        let both = through(utility(Invert::Both, false, None, 0.0, 0.0, 100.0), x.clone());
        assert_eq!(both[100], [-x[100][0], -x[100][1]]);
    }

    #[test]
    fn width_scales_the_side_and_mono_removes_it() {
        let x = signal();
        // The 2 kHz tone is on the left alone: half mid, half side.
        let narrow = through(utility(Invert::None, false, None, 0.0, 0.0, 0.0), x.clone());
        assert!((level(&narrow, 0, 2000.0) - 0.15).abs() < 1e-3 && (level(&narrow, 1, 2000.0) - 0.15).abs() < 1e-3);
        assert!((level(&narrow, 0, 60.0) - 0.4).abs() < 1e-3, "the middle is left alone");
        let wide = through(utility(Invert::None, false, None, 0.0, 0.0, 200.0), x.clone());
        assert!((level(&wide, 0, 2000.0) - 0.45).abs() < 1e-3 && (level(&wide, 1, 2000.0) - 0.15).abs() < 1e-3, "{} {}", level(&wide, 0, 2000.0), level(&wide, 1, 2000.0));
        let mono = through(utility(Invert::None, true, None, 0.0, 0.0, 400.0), x.clone());
        assert_eq!(narrow, mono, "mono leaves width nothing to widen");
    }

    #[test]
    fn mono_below_sums_the_bass_and_keeps_the_width_above() {
        // The bass spread: 60 Hz on the left alone, 2 kHz on the right alone.
        let x: Vec<Frame> = (0..48000)
            .map(|i| {
                let t = i as f64 / RATE;
                [0.4 * (2.0 * PI * 60.0 * t).sin(), 0.3 * (2.0 * PI * 2000.0 * t).sin()]
            })
            .collect();
        // A centred tone at the crossover itself comes through at its level:
        // the two halves meet flat.
        let centred: Vec<Frame> = (0..48000).map(|i| [0.5 * (2.0 * PI * 200.0 * i as f64 / RATE).sin(); 2]).collect();
        let flat = through(utility(Invert::None, false, Some(200.0), 0.0, 0.0, 100.0), centred);
        assert!((level(&flat, 0, 200.0) - 0.5).abs() < 1e-3 && (level(&flat, 1, 200.0) - 0.5).abs() < 1e-3, "{}", level(&flat, 0, 200.0));
        let y = through(utility(Invert::None, false, Some(200.0), 0.0, 0.0, 100.0), x.clone());
        assert!((level(&y, 0, 60.0) - 0.2).abs() < 0.003 && (level(&y, 1, 60.0) - 0.2).abs() < 0.003, "{} {}", level(&y, 0, 60.0), level(&y, 1, 60.0));
        assert!((level(&y, 1, 2000.0) - 0.3).abs() < 1e-3 && level(&y, 0, 2000.0) < 1e-3, "{} {}", level(&y, 0, 2000.0), level(&y, 1, 2000.0));
    }

    #[test]
    fn blocks_do_not_change_the_output() {
        let x = signal();
        let whole = through(utility(Invert::Right, false, Some(120.0), -3.0, 0.2, 150.0), x.clone());
        let mut u = utility(Invert::Right, false, Some(120.0), -3.0, 0.2, 150.0);
        let mut small = x;
        for chunk in small.chunks_mut(97) {
            u.process(chunk, clock());
        }
        assert_eq!(whole, small);
    }
}

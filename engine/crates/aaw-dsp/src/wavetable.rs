//! Wavetables: one cycle of a wave, read bandlimited at any pitch.
//!
//! A table is kept as a stack of levels, each the same cycle with its
//! harmonics cut off lower: level 0 holds up to 1024 harmonics, level 1 up to
//! 512, and so on down to the fundamental alone. An oscillator reads the
//! level whose harmonics all fall below half the sample rate at its pitch, so
//! a table never aliases, and interpolates linearly between the samples.
//!
//! The built-in tables are spectra, or a cycle drawn in time and taken apart
//! by a Fourier transform; a sample of the project is read the same way: its
//! whole file is one cycle. Building a table is a compile-time cost, done
//! off the audio thread and kept between compiles.

use realfft::num_complex::Complex64;
use realfft::RealFftPlanner;
use std::f64::consts::{PI, TAU};
use std::sync::{Arc, OnceLock};

/// Samples a cycle has at every level.
pub const LENGTH: usize = 2048;
/// The most harmonics a level holds, half the length.
const HARMONICS: usize = LENGTH / 2;
/// The longest sample file read as one cycle.
pub const MAX_CYCLE_FRAMES: usize = 65536;

/// One cycle at every bandlimit.
#[derive(Debug)]
pub struct Wavetable {
    /// What the table is, for a patch's signature: a built-in name or a
    /// sample's identity.
    pub key: String,
    /// Each level's cycle, with its first sample repeated at the end so a
    /// read never wraps.
    levels: Vec<Vec<f64>>,
}

impl Wavetable {
    /// A table from a spectrum: `spectrum[k]` is the complex amplitude of
    /// harmonic `k + 1`, in the convention that a sine of amplitude `a` is
    /// `-i a / 2`. Levels are normalized together, so the fullest peaks at
    /// 1 and the rest sit where they fall.
    fn from_spectrum(key: &str, spectrum: &[Complex64]) -> Wavetable {
        let mut planner = RealFftPlanner::<f64>::new();
        let inverse = planner.plan_fft_inverse(LENGTH);
        let mut levels = Vec::new();
        let mut harmonics = HARMONICS;
        while harmonics >= 1 {
            let mut bins = vec![Complex64::new(0.0, 0.0); LENGTH / 2 + 1];
            for (k, a) in spectrum.iter().enumerate().take(harmonics.min(spectrum.len())) {
                // A real inverse FFT of length N makes a sine of amplitude a
                // from a bin of -i a N / 2.
                bins[k + 1] = *a * (LENGTH as f64);
            }
            let mut cycle = vec![0.0; LENGTH];
            inverse.process(&mut bins, &mut cycle).expect("sizes match");
            cycle.push(cycle[0]);
            levels.push(cycle);
            if harmonics == 1 {
                break;
            }
            harmonics /= 2;
        }
        let peak = levels[0].iter().fold(0.0f64, |m, x| m.max(x.abs()));
        if peak > 0.0 {
            for level in &mut levels {
                for x in level.iter_mut() {
                    *x /= peak;
                }
            }
        }
        Wavetable {
            key: key.to_string(),
            levels,
        }
    }

    /// A table of a cycle given in time, of any length over one sample.
    fn from_cycle_in(key: &str, cycle: &[f64]) -> Wavetable {
        let n = cycle.len();
        let mut planner = RealFftPlanner::<f64>::new();
        let forward = planner.plan_fft_forward(n);
        let mut input = cycle.to_vec();
        let mut bins = vec![Complex64::new(0.0, 0.0); n / 2 + 1];
        forward.process(&mut input, &mut bins).expect("sizes match");
        // Bin k of a forward FFT of length n holds n/2 times the complex
        // amplitude of harmonic k; the DC bin is left out.
        let spectrum: Vec<Complex64> = bins.iter().skip(1).take(HARMONICS).map(|b| *b / (n as f64)).collect();
        Wavetable::from_spectrum(key, &spectrum)
    }

    /// A table of a sample's file, mono-summed, its DC removed: the whole
    /// file is one cycle. Refused past `MAX_CYCLE_FRAMES` frames.
    pub fn from_cycle(key: &str, data: &[f64], channels: usize) -> Result<Wavetable, String> {
        let channels = channels.max(1);
        let frames = data.len() / channels;
        if frames < 2 {
            return Err("a wavetable's file needs at least two frames".into());
        }
        if frames > MAX_CYCLE_FRAMES {
            return Err(format!("a wavetable's file is one cycle, at most {MAX_CYCLE_FRAMES} frames; this one has {frames}"));
        }
        let mut cycle: Vec<f64> = data.chunks(channels).map(|f| f.iter().sum::<f64>() / channels as f64).collect();
        let mean = cycle.iter().sum::<f64>() / frames as f64;
        for x in &mut cycle {
            *x -= mean;
        }
        if cycle.iter().all(|x| x.abs() < 1e-9) {
            return Err("a wavetable's file is silent".into());
        }
        Ok(Wavetable::from_cycle_in(key, &cycle))
    }

    /// A built-in table by name, made once and shared.
    pub fn builtin(name: &str) -> Option<Arc<Wavetable>> {
        static TABLES: OnceLock<Vec<(&'static str, OnceLock<Arc<Wavetable>>)>> = OnceLock::new();
        let tables = TABLES.get_or_init(|| aaw_model::WAVETABLES.iter().map(|n| (*n, OnceLock::new())).collect());
        let (name, slot) = tables.iter().find(|(n, _)| *n == name)?;
        Some(slot.get_or_init(|| Arc::new(make_builtin(name))).clone())
    }

    /// The level whose harmonics all sit below half the sample rate for a
    /// phase increment of `inc` cycles a frame.
    #[inline]
    pub fn level_for(&self, inc: f64) -> usize {
        let allowed = if inc > 0.0 { (0.5 / inc).floor() } else { HARMONICS as f64 };
        let mut level = 0;
        let mut harmonics = HARMONICS as f64;
        while level + 1 < self.levels.len() && harmonics > allowed {
            level += 1;
            harmonics /= 2.0;
        }
        level
    }

    /// The wave at phase `t` (0 to 1) read from `level`, interpolated.
    #[inline]
    pub fn sample(&self, level: usize, t: f64) -> f64 {
        let cycle = &self.levels[level.min(self.levels.len() - 1)];
        let x = t.rem_euclid(1.0) * LENGTH as f64;
        let i = x as usize;
        let frac = x - i as f64;
        let i = i.min(LENGTH - 1);
        cycle[i] + (cycle[i + 1] - cycle[i]) * frac
    }

    /// The fullest cycle, for a drawing: `n` points over one cycle.
    pub fn cycle(&self, n: usize) -> Vec<f64> {
        (0..n).map(|i| self.sample(0, i as f64 / n as f64)).collect()
    }
}

/// A sine of amplitude `a` as a spectrum entry.
fn sine(a: f64) -> Complex64 {
    Complex64::new(0.0, -a / 2.0)
}

/// The built-in tables.
fn make_builtin(name: &str) -> Wavetable {
    match name {
        "organ" => {
            // Drawbars: 16', 8', 5 1/3', 4', 2 2/3', 2'.
            let mut spectrum = vec![Complex64::new(0.0, 0.0); 8];
            for (k, a) in [(1, 0.8), (2, 0.6), (3, 0.5), (4, 0.4), (6, 0.25), (8, 0.2)] {
                spectrum[k - 1] = sine(a);
            }
            Wavetable::from_spectrum(name, &spectrum)
        }
        "bright" => {
            let spectrum: Vec<Complex64> = (1..=64).map(|k| sine(1.0 / (k as f64).sqrt())).collect();
            Wavetable::from_spectrum(name, &spectrum)
        }
        "hollow" => {
            let spectrum: Vec<Complex64> = (1..=63).map(|k| if k % 2 == 1 { sine(1.0 / (k as f64).powf(1.5)) } else { Complex64::new(0.0, 0.0) }).collect();
            Wavetable::from_spectrum(name, &spectrum)
        }
        "vowel" => {
            // An "ah": formants around the fifth, ninth and twentieth harmonics.
            let formants = [(5.0, 1.5, 1.0), (9.0, 2.0, 0.6), (20.0, 3.0, 0.3)];
            let spectrum: Vec<Complex64> = (1..=32)
                .map(|k| {
                    let k = k as f64;
                    let a: f64 = formants.iter().map(|(c, w, h)| h * (-((k - c) / w).powi(2)).exp()).sum();
                    sine(a + if k == 1.0 { 0.5 } else { 0.0 })
                })
                .collect();
            Wavetable::from_spectrum(name, &spectrum)
        }
        "fold" => {
            let cycle: Vec<f64> = (0..LENGTH).map(|i| (2.5 * (TAU * i as f64 / LENGTH as f64).sin() * PI / 2.0).sin()).collect();
            Wavetable::from_cycle_in(name, &cycle)
        }
        _ => {
            // "steps": a sine in eight steps.
            let cycle: Vec<f64> = (0..LENGTH).map(|i| ((TAU * i as f64 / LENGTH as f64).sin() * 4.0).round() / 4.0).collect();
            Wavetable::from_cycle_in("steps", &cycle)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The amplitude of harmonic `k` in `cycle`.
    fn harmonic(cycle: &[f64], k: usize) -> f64 {
        let n = cycle.len() as f64;
        let (mut re, mut im) = (0.0, 0.0);
        for (i, x) in cycle.iter().enumerate() {
            let a = TAU * k as f64 * i as f64 / n;
            re += x * a.cos();
            im += x * a.sin();
        }
        2.0 * (re * re + im * im).sqrt() / n
    }

    #[test]
    fn a_spectrum_comes_back_as_its_harmonics_and_a_level_leaves_the_high_ones_out() {
        let t = Wavetable::builtin("organ").unwrap();
        let full: Vec<f64> = t.cycle(LENGTH);
        assert!((full.iter().fold(0.0f64, |m, x| m.max(x.abs())) - 1.0).abs() < 1e-9, "normalized");
        // The drawbars' ratios hold after normalization.
        assert!((harmonic(&full, 2) / harmonic(&full, 1) - 0.75).abs() < 1e-6);
        assert!((harmonic(&full, 8) / harmonic(&full, 1) - 0.25).abs() < 1e-6);
        assert!(harmonic(&full, 5) < 1e-9 && harmonic(&full, 7) < 1e-9);
        // At a pitch where only four harmonics fit, the eighth is gone.
        let level = t.level_for(0.5 / 5.0);
        let narrow: Vec<f64> = (0..LENGTH).map(|i| t.sample(level, i as f64 / LENGTH as f64)).collect();
        assert!(harmonic(&narrow, 4) > 0.1 && harmonic(&narrow, 6) < 1e-9 && harmonic(&narrow, 8) < 1e-9);
        assert_eq!(t.level_for(0.0001), 0);
        assert_eq!(t.level_for(0.4), t.levels.len() - 1);
    }

    #[test]
    fn a_cycle_of_any_length_becomes_a_table_and_a_sample_is_cleaned() {
        // A saw of 300 samples: its harmonics fall as 1/k.
        let saw: Vec<f64> = (0..300).map(|i| 2.0 * i as f64 / 300.0 - 1.0).collect();
        let t = Wavetable::from_cycle("saw", &saw, 1).unwrap();
        let full = t.cycle(LENGTH);
        assert!((harmonic(&full, 2) / harmonic(&full, 1) - 0.5).abs() < 0.02);
        assert!((harmonic(&full, 4) / harmonic(&full, 1) - 0.25).abs() < 0.02);
        // Stereo with an offset: summed and centered.
        let offset: Vec<f64> = (0..300).flat_map(|i| [0.5 + (TAU * i as f64 / 300.0).sin(), 0.5]).collect();
        let t = Wavetable::from_cycle("s", &offset, 2).unwrap();
        let full = t.cycle(LENGTH);
        assert!(full.iter().sum::<f64>().abs() < 1e-6, "no DC");
        assert!((harmonic(&full, 1) - 1.0).abs() < 1e-6 && harmonic(&full, 2) < 1e-6);
        assert!(Wavetable::from_cycle("x", &[0.0; 100], 1).is_err());
        assert!(Wavetable::from_cycle("x", &vec![0.1; MAX_CYCLE_FRAMES + 1], 1).unwrap_err().contains("one cycle"));
        for name in aaw_model::WAVETABLES {
            let t = Wavetable::builtin(name).unwrap();
            assert!(t.cycle(64).iter().all(|x| x.is_finite() && x.abs() <= 1.0 + 1e-9), "{name}");
        }
        assert!(Wavetable::builtin("nope").is_none());
    }
}

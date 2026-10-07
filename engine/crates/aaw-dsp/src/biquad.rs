//! Second-order sections: Butterworth filters designed as `scipy.signal.butter`
//! designs them, RBJ equalizer bands, and the cascade `scipy.signal.sosfilt` runs.

use crate::Frame;
use aaw_model::{BandShape, EqBand};
use realfft::num_complex::Complex64;
use std::f64::consts::{FRAC_1_SQRT_2, PI};

/// One section, normalized to a0 = 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Section {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// numpy's complex division.
fn divide(a: Complex64, b: Complex64) -> Complex64 {
    if b.re.abs() >= b.im.abs() {
        let rat = b.im / b.re;
        let scl = 1.0 / (b.re + b.im * rat);
        Complex64::new((a.re + a.im * rat) * scl, (a.im - a.re * rat) * scl)
    } else {
        let rat = b.re / b.im;
        let scl = 1.0 / (b.im + b.re * rat);
        Complex64::new((a.re * rat + a.im) * scl, (a.im * rat - a.re) * scl)
    }
}

fn product(values: impl Iterator<Item = Complex64>) -> Complex64 {
    values.fold(Complex64::new(1.0, 0.0), |acc, v| acc * v)
}

/// `butter(order, cutoff_hz, "highpass" | "lowpass", fs=rate, output="sos")` for
/// an even order: the analog prototype, the frequency transform, the bilinear
/// transform and `zpk2sos`, whose sections end with the poles nearest the unit
/// circle.
pub fn butter(order: usize, cutoff_hz: f64, highpass: bool, rate: f64) -> Vec<Section> {
    assert!(order >= 2 && order % 2 == 0, "even orders only");
    let n = order as i64;
    // buttap: poles on the unit circle's left half.
    let prototype: Vec<Complex64> = (0..order)
        .map(|i| {
            let m = (-n + 1 + 2 * i as i64) as f64;
            let theta = PI * m / (2.0 * order as f64);
            -Complex64::new(theta.cos(), theta.sin())
        })
        .collect();
    let wn = cutoff_hz / (rate / 2.0);
    let fs = 2.0;
    let warped = 2.0 * fs * (PI * wn / fs).tan();
    let fs2 = Complex64::new(2.0 * fs, 0.0);
    let one = Complex64::new(1.0, 0.0);
    let (analog, mut gain): (Vec<Complex64>, f64) = if highpass {
        // lp2hp_zpk: the zeros land on the origin.
        let poles: Vec<Complex64> = prototype.iter().map(|p| divide(Complex64::new(warped, 0.0), *p)).collect();
        (poles, divide(one, product(prototype.iter().map(|p| -*p))).re)
    } else {
        (prototype.iter().map(|p| *p * warped).collect(), warped.powf(order as f64))
    };
    // bilinear_zpk
    let poles: Vec<Complex64> = analog.iter().map(|p| divide(fs2 + *p, fs2 - *p)).collect();
    let denominator = product(analog.iter().map(|p| fs2 - *p));
    let zero = if highpass {
        gain *= divide(product((0..order).map(|_| fs2)), denominator).re;
        1.0
    } else {
        gain *= divide(one, denominator).re;
        -1.0
    };
    // zpk2sos: one pole of each conjugate pair, in order of real part, paired
    // with two of the identical real zeros; the worst pole goes last.
    let mut pairs: Vec<Complex64> = poles.iter().filter(|p| p.im > 0.0).copied().collect();
    pairs.sort_by(|a, b| a.re.total_cmp(&b.re).then(a.im.abs().total_cmp(&b.im.abs())));
    let mut sections = vec![
        Section {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        };
        order / 2
    ];
    for slot in (0..order / 2).rev() {
        let mut worst = 0;
        for (i, p) in pairs.iter().enumerate() {
            if (1.0 - p.norm()).abs() < (1.0 - pairs[worst].norm()).abs() {
                worst = i;
            }
        }
        let p = pairs.remove(worst);
        sections[slot] = Section {
            b0: 1.0,
            b1: -zero - zero,
            b2: zero * zero,
            a1: -p.re - p.re,
            a2: p.re * p.re + p.im * p.im,
        };
    }
    sections[0].b0 *= gain;
    sections[0].b1 *= gain;
    sections[0].b2 *= gain;
    sections
}

/// The Q of each second-order section of a Butterworth filter of `order`,
/// least resonant first, as `butter` lays them out.
pub fn butterworth_q(order: usize) -> impl Iterator<Item = f64> {
    (0..order / 2).map(move |i| 1.0 / (2.0 * (PI * (2 * i + 1) as f64 / (2 * order) as f64).sin()))
}

/// The Q of each section of a pass band: the Butterworth Qs of its slope's
/// order, the last scaled by the band's `q` over 1/√2, so that 0.71 is flat
/// at every slope and a higher `q` lifts the corner as it does at 12 dB.
pub fn pass_q(band: &EqBand) -> impl Iterator<Item = f64> {
    let n = band.sections();
    butterworth_q(2 * n).enumerate().map(move |(i, q)| if i + 1 == n { q * band.q / FRAC_1_SQRT_2 } else { q })
}

/// The RBJ audio EQ cookbook biquad for a band, or for one section of a
/// pass band at `q`.
fn section(band: &EqBand, q: f64, rate: f64) -> Section {
    let a = 10f64.powf(band.gain_db / 40.0);
    let w = 2.0 * PI * band.freq_hz / rate;
    let (cos, alpha) = (w.cos(), w.sin() / (2.0 * q));
    let (b, d) = match band.shape {
        BandShape::Bell => (
            [1.0 + alpha * a, -2.0 * cos, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * cos, 1.0 - alpha / a],
        ),
        BandShape::Highpass => ([(1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]),
        BandShape::Lowpass => ([(1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0], [1.0 + alpha, -2.0 * cos, 1.0 - alpha]),
        shelf => {
            let root = 2.0 * a.sqrt() * alpha;
            let sign = if shelf == BandShape::LowShelf { 1.0 } else { -1.0 };
            (
                [
                    a * ((a + 1.0) - sign * (a - 1.0) * cos + root),
                    sign * 2.0 * a * ((a - 1.0) - sign * (a + 1.0) * cos),
                    a * ((a + 1.0) - sign * (a - 1.0) * cos - root),
                ],
                [
                    (a + 1.0) + sign * (a - 1.0) * cos + root,
                    -sign * 2.0 * ((a - 1.0) + sign * (a + 1.0) * cos),
                    (a + 1.0) + sign * (a - 1.0) * cos - root,
                ],
            )
        }
    };
    Section {
        b0: b[0] / d[0],
        b1: b[1] / d[0],
        b2: b[2] / d[0],
        a1: d[1] / d[0],
        a2: d[2] / d[0],
    }
}

/// The RBJ audio EQ cookbook biquad for a bell or a shelf, or the one
/// section of a pass band at 12 dB an octave.
pub fn band(band: &EqBand, rate: f64) -> Section {
    section(band, band.q, rate)
}

/// A band's sections: one for a bell or a shelf, and for a pass band one
/// for each 12 dB of its slope, with `pass_q`'s resonances.
pub fn sections(band: &EqBand, rate: f64) -> Vec<Section> {
    if band.shape.is_pass() {
        pass_q(band).map(|q| section(band, q, rate)).collect()
    } else {
        vec![section(band, band.q, rate)]
    }
}

/// A 2×2 matrix, row-major.
type Matrix = [f64; 4];

fn multiply(m: &Matrix, v: [f64; 2]) -> [f64; 2] {
    [m[0] * v[0] + m[1] * v[1], m[2] * v[0] + m[3] * v[1]]
}

fn inverse(m: &Matrix) -> Option<Matrix> {
    let det = m[0] * m[3] - m[1] * m[2];
    let scale = m.iter().fold(0.0f64, |s, x| s.max(x.abs()));
    if det.abs() <= 1e-12 * scale * scale || !det.is_finite() {
        return None;
    }
    Some([m[3] / det, -m[1] / det, -m[2] / det, m[0] / det])
}

impl Section {
    /// The transposed state as a function of the direct form's history
    /// (w[n-1], w[n-2]).
    fn from_history(&self) -> Matrix {
        let Section { b0, b1, b2, a1, a2 } = *self;
        [b1 - a1 * b0, b2 - a2 * b0, b2 - a2 * b0, b2 * a1 - a2 * b1]
    }

    /// The state of the trapezoidal state-variable filter with these poles as a
    /// function of the direct form's history. None for poles such a filter
    /// does not have.
    fn to_integrators(&self) -> Option<Matrix> {
        let (a1, a2) = (self.a1, self.a2);
        let (low, high) = (1.0 + a1 + a2, 1.0 - a1 + a2);
        if !(low > 0.0 && high > 0.0) {
            return None;
        }
        let g = (low / high).sqrt();
        let c1 = high / 4.0;
        let (c2, c3) = (g * c1, g * g * c1);
        let (a00, a01, a10, a11) = (2.0 * c1 - 1.0, -2.0 * c2, 2.0 * c2, 1.0 - 2.0 * c3);
        let (b_0, b_1) = (2.0 * c2, 2.0 * c3);
        Some([b_0, a00 * b_0 + a01 * b_1 + a1 * b_0, b_1, a10 * b_0 + a11 * b_1 + a1 * b_1])
    }

    /// The larger of the gains at zero and at the Nyquist frequency: the level
    /// a section passes its band at.
    fn passband_gain(&self) -> f64 {
        let Section { b0, b1, b2, a1, a2 } = *self;
        ((b0 + b1 + b2) / (1.0 + a1 + a2)).abs().max(((b0 - b1 + b2) / (1.0 - a1 + a2)).abs())
    }

    /// A state of `old` carried into this section: the integrator states of
    /// the equivalent state-variable filter stay as they are, as an analog
    /// filter's capacitors would, scaled by how the level reaching the section
    /// changed. A direct-form state carried as it is into very different
    /// coefficients bursts.
    fn carry(&self, old: &Section, z: [f64; 2], scale: f64) -> [f64; 2] {
        let steps = (|| {
            let history = multiply(&inverse(&old.from_history())?, z);
            let integrators = multiply(&old.to_integrators()?, history);
            let history = multiply(&inverse(&self.to_integrators()?)?, [integrators[0] * scale, integrators[1] * scale]);
            Some(multiply(&self.from_history(), history))
        })();
        match steps {
            Some(z) if z[0].is_finite() && z[1].is_finite() => z,
            _ => [z[0] * scale, z[1] * scale],
        }
    }
}

/// A cascade of sections with stereo state, run as `sosfilt` runs it.
///
/// A cascade made `with_glide` can take over from one with other coefficients
/// without a step: each section keeps what its filter was holding, and for the
/// length of the glide the old cascade runs beside the new one while the
/// output crosses from one to the other.
#[derive(Clone, Debug)]
pub struct Cascade {
    sections: Vec<Section>,
    /// Per section, the two transposed direct form II states of each channel:
    /// `state[section][which][channel]`.
    state: Vec<[[f64; 2]; 2]>,
    /// The cascade taken over from, while the output crosses over.
    previous: Vec<Section>,
    previous_state: Vec<[[f64; 2]; 2]>,
    glide: usize,
    left: usize,
}

/// One frame through the sections. The channels are independent and run side
/// by side, each as `sosfilt` runs it.
#[inline]
fn run(sections: &[Section], state: &mut [[[f64; 2]; 2]], x: Frame) -> Frame {
    let mut x_c = x;
    for (s, z) in sections.iter().zip(state.iter_mut()) {
        let x_n = x_c;
        x_c = [s.b0 * x_n[0] + z[0][0], s.b0 * x_n[1] + z[0][1]];
        z[0] = [s.b1 * x_n[0] - s.a1 * x_c[0] + z[1][0], s.b1 * x_n[1] - s.a1 * x_c[1] + z[1][1]];
        z[1] = [s.b2 * x_n[0] - s.a2 * x_c[0], s.b2 * x_n[1] - s.a2 * x_c[1]];
    }
    x_c
}

impl Cascade {
    pub fn new(sections: Vec<Section>) -> Cascade {
        Cascade {
            state: vec![[[0.0; 2]; 2]; sections.len()],
            sections,
            previous: Vec::new(),
            previous_state: Vec::new(),
            glide: 0,
            left: 0,
        }
    }

    /// A cascade that crosses over from another in `glide` frames.
    pub fn with_glide(sections: Vec<Section>, glide: usize) -> Cascade {
        Cascade {
            previous: sections.clone(),
            previous_state: vec![[[0.0; 2]; 2]; sections.len()],
            glide,
            ..Cascade::new(sections)
        }
    }

    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    #[inline]
    pub fn tick(&mut self, x: Frame) -> Frame {
        let y = run(&self.sections, &mut self.state, x);
        if self.left == 0 {
            return y;
        }
        let was = run(&self.previous, &mut self.previous_state, x);
        let w = self.left as f64 / self.glide as f64;
        self.left -= 1;
        [was[0] * w + y[0] * (1.0 - w), was[1] * w + y[1] * (1.0 - w)]
    }

    pub fn process(&mut self, x: &mut [Frame]) {
        for f in x.iter_mut() {
            *f = self.tick(*f);
        }
    }

    /// Continues from `old`'s state. With other coefficients of the same
    /// shape, each section keeps what its filter was holding. Does not
    /// allocate.
    pub fn take_over(&mut self, old: &Cascade) {
        if old.sections.len() != self.sections.len() {
            return;
        }
        // A design may put its whole gain in one section, so the level reaching
        // each of the others changes with it.
        let mut scale = 1.0;
        for (i, (new, was)) in self.sections.iter().zip(&old.sections).enumerate() {
            for c in 0..2 {
                let z = [old.state[i][0][c], old.state[i][1][c]];
                let z = if new == was && scale == 1.0 { z } else { new.carry(was, z, scale) };
                (self.state[i][0][c], self.state[i][1][c]) = (z[0], z[1]);
            }
            let change = new.passband_gain() / was.passband_gain();
            if change.is_finite() && change > 0.0 {
                scale *= change;
            }
        }
        if self.previous.len() != self.sections.len() {
            return;
        }
        if self.sections != old.sections {
            self.previous.copy_from_slice(&old.sections);
            self.previous_state.copy_from_slice(&old.state);
            self.left = self.glide;
        } else if old.left > 0 && old.previous.len() == self.previous.len() {
            // Still crossing over from the one before.
            self.previous.copy_from_slice(&old.previous);
            self.previous_state.copy_from_slice(&old.previous_state);
            self.left = old.left.min(self.glide);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &Section, b: [f64; 5], tolerance: f64) {
        let got = [a.b0, a.b1, a.b2, a.a1, a.a2];
        for (x, y) in got.iter().zip(b) {
            assert!((x - y).abs() <= tolerance * y.abs().max(1.0), "{got:?} is not {b:?}");
        }
    }

    // Values from scipy.signal.butter(..., output="sos").
    #[test]
    fn butter_matches_scipy() {
        let s = butter(2, 1000.0, false, 48000.0);
        close(&s[0], [0.003916126660547369, 0.007832253321094738, 0.003916126660547369, -1.815341082704568, 0.8310055893467575], 1e-14);
        let s = butter(4, 180.0, true, 44100.0);
        close(&s[0], [0.9670464611382816, -1.9340929222765633, 0.9670464611382816, -1.9530722998109404, 0.9537147432867223], 1e-14);
        close(&s[1], [1.0, -2.0, 1.0, -1.9799132668898496, 0.9805645394316036], 1e-14);
        let s = butter(8, 5000.0, false, 48000.0);
        close(&s[0], [3.164723042861606e-05, 6.329446085723212e-05, 3.164723042861606e-05, -0.9935146201701127, 0.25229777164031314], 1e-14);
        close(&s[1], [1.0, 2.0, 1.0, -1.0534735331733704, 0.3278743274549607], 1e-14);
        close(&s[2], [1.0, 2.0, 1.0, -1.1856935771991712, 0.49453404552870517], 1e-14);
        close(&s[3], [1.0, 2.0, 1.0, -1.4182682334127001, 0.7876879838837794], 1e-14);
        let s = butter(6, 30.0, true, 48000.0);
        close(&s[0], [0.9924423339917975, -1.984884667983595, 0.9924423339917975, -1.9924269604045233, 0.9924423233672205], 1e-13);
        close(&s[1], [1.0, -2.0, 1.0, -1.9944464105419268, 0.9944617890759538], 1e-13);
        close(&s[2], [1.0, -2.0, 1.0, -1.997953903566973, 0.9979693091461493], 1e-13);
    }

    fn eq_band(shape: BandShape, freq_hz: f64, gain_db: f64, q: f64) -> EqBand {
        EqBand {
            shape,
            freq_hz,
            gain_db,
            q,
            slope_db_per_octave: 12,
        }
    }

    // Values from effects.band_sos.
    #[test]
    fn bands_match_the_cookbook() {
        let bell = eq_band(BandShape::Bell, 400.0, -3.0, 1.2);
        close(&band(&bell, 48000.0), [0.992621975907652, -1.9468032816944603, 0.9568529934791812, -1.9468032816944603, 0.9494749693868333], 1e-15);
        let shelf = eq_band(BandShape::HighShelf, 8000.0, 2.0, 0.71);
        close(&band(&shelf, 48000.0), [1.1628491712894642, -0.7917469324803136, 0.29815254270866065, -0.5600835370584608, 0.2293383185762718], 1e-15);
        let low = eq_band(BandShape::LowShelf, 120.0, 4.5, 0.9);
        close(&band(&low, 44100.0), [1.0024848277007015, -1.9831499757511235, 0.9810407543756617, -1.9832259116793358, 0.9834496461481514], 1e-15);
    }

    /// A pass band at q = 1/√2 is the Butterworth filter of its slope, at
    /// every slope, and a higher q lifts the corner by 20·log10(q/(1/√2)).
    #[test]
    fn pass_bands_are_butterworth_when_flat_and_resonate_above() {
        for (shape, highpass) in [(BandShape::Highpass, true), (BandShape::Lowpass, false)] {
            for slope in [12, 24, 36, 48] {
                let mut flat = eq_band(shape, 300.0, 0.0, FRAC_1_SQRT_2);
                flat.slope_db_per_octave = slope;
                let x = sine(100.0, 9600).iter().zip(sine(2400.0, 9600)).map(|(a, b)| [a[0] + b[0], a[1] + b[1]]).collect::<Vec<_>>();
                let mut expected = x.clone();
                Cascade::new(butter(slope as usize / 6, 300.0, highpass, 48000.0)).process(&mut expected);
                let mut got = x.clone();
                Cascade::new(sections(&flat, 48000.0)).process(&mut got);
                let worst = expected.iter().zip(&got).fold(0.0f64, |m, (a, b)| m.max((a[0] - b[0]).abs()));
                assert!(worst < 1e-9, "{shape:?} {slope}: {worst}");
                assert_eq!(sections(&flat, 48000.0).len(), slope as usize / 12);
                // The gain of a pass band is ignored: the sections are the same.
                let mut gained = flat.clone();
                gained.gain_db = 12.0;
                assert_eq!(sections(&gained, 48000.0), sections(&flat, 48000.0));
                // A tone at the corner comes through at the product of the
                // sections' Qs, which is where each section's gain sits: q
                // itself at 12 dB, since the one Butterworth Q is 1/√2.
                let mut resonant = flat.clone();
                resonant.q = 2.0;
                let mut corner = sine(300.0, 48000);
                Cascade::new(sections(&resonant, 48000.0)).process(&mut corner);
                let expected = 20.0 * (2.0 / FRAC_1_SQRT_2 * butterworth_q(slope as usize / 6).product::<f64>()).log10();
                let db = level(&corner[24000..]);
                assert!((db - expected).abs() < 0.05, "{shape:?} {slope}: {db} dB at the corner, not {expected}");
            }
        }
    }

    fn sine(hz: f64, n: usize) -> Vec<Frame> {
        (0..n)
            .map(|i| {
                let x = (2.0 * PI * hz * i as f64 / 48000.0).sin();
                [x, x * 0.5]
            })
            .collect()
    }

    fn peak(x: &[Frame]) -> f64 {
        x.iter().fold(0.0, |m, f| m.max(f[0].abs()))
    }

    /// The level of a sine over whole cycles, in dB relative to full scale.
    fn level(x: &[Frame]) -> f64 {
        10.0 * (2.0 * x.iter().map(|f| f[0] * f[0]).sum::<f64>() / x.len() as f64).log10()
    }

    #[test]
    fn a_lowpass_passes_lows_and_cuts_highs() {
        for order in [2, 4, 6, 8] {
            let mut low = sine(100.0, 9600);
            Cascade::new(butter(order, 1000.0, false, 48000.0)).process(&mut low);
            assert!((peak(&low[4800..]) - 1.0).abs() < 1e-3);
            let mut high = sine(4000.0, 9600);
            Cascade::new(butter(order, 1000.0, false, 48000.0)).process(&mut high);
            // A Butterworth response on the bilinear transform's warped axis.
            let ratio = (PI * 4000.0 / 48000.0).tan() / (PI * 1000.0 / 48000.0).tan();
            let expected = -10.0 * (1.0 + ratio.powi(2 * order as i32)).log10();
            let db = level(&high[4800..]);
            assert!((db - expected).abs() < 0.1, "order {order}: {db} dB, not {expected}");
            let mut cut = sine(100.0, 9600);
            Cascade::new(butter(order, 1000.0, true, 48000.0)).process(&mut cut);
            let ratio = (PI * 1000.0 / 48000.0).tan() / (PI * 100.0 / 48000.0).tan();
            let expected = -10.0 * (1.0 + ratio.powi(2 * order as i32)).log10();
            let db = level(&cut[4800..]);
            assert!((db - expected).abs() < 0.1, "highpass order {order}: {db} dB, not {expected}");
        }
    }

    #[test]
    fn carried_state_does_not_burst() {
        // A 48 dB lowpass jumping from 20 kHz to 10 Hz on noise.
        let mut x: Vec<Frame> = Vec::new();
        let mut s = 1u64;
        for _ in 0..9600 {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let v = (s >> 11) as f64 / (1u64 << 53) as f64 - 0.5;
            x.push([v, -v]);
        }
        let mut open = Cascade::new(butter(8, 20000.0, false, 48000.0));
        let mut head = x[..4800].to_vec();
        open.process(&mut head);
        let mut closed = Cascade::new(butter(8, 10.0, false, 48000.0));
        closed.take_over(&open);
        let mut tail = x[4800..].to_vec();
        closed.process(&mut tail);
        assert!(peak(&tail) < 1.0, "burst of {}", peak(&tail));
        // With a glide the output crosses from the old filter to the new.
        let mut gliding = Cascade::with_glide(butter(8, 10.0, false, 48000.0), 240);
        gliding.take_over(&open);
        let mut crossed = x[4800..].to_vec();
        gliding.process(&mut crossed);
        let mut on = x[4800..].to_vec();
        open.clone().process(&mut on);
        assert!((crossed[0][0] - on[0][0]).abs() < 1e-12, "starts as the old filter");
        assert_eq!(&crossed[240..], &tail[240..], "ends as the new one");
        // The same coefficients carry the state as it is.
        let mut same = Cascade::new(butter(8, 20000.0, false, 48000.0));
        same.take_over(&open);
        assert_eq!(same.state, open.state);
    }
}

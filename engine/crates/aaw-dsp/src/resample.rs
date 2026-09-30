//! Bandlimited resampling as `scipy.signal.resample_poly(x, up, down)` computes it:
//! a Kaiser-windowed (beta 5) sinc low-pass from `firwin`, applied by scipy's
//! polyphase `upfirdn` loop with zero padding, in the same operation order.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, ToPrimitive, Zero};
use std::f64::consts::PI;

/// Cephes `i0` coefficients for |x| <= 8, as scipy.special and numpy use.
const I0_A: [f64; 30] = [
    -4.4153416464793395e-18,
    3.3307945188222384e-17,
    -2.431279846547955e-16,
    1.715391285555133e-15,
    -1.1685332877993451e-14,
    7.676185498604936e-14,
    -4.856446783111929e-13,
    2.95505266312964e-12,
    -1.726826291441556e-11,
    9.675809035373237e-11,
    -5.189795601635263e-10,
    2.6598237246823866e-09,
    -1.300025009986248e-08,
    6.046995022541919e-08,
    -2.670793853940612e-07,
    1.1173875391201037e-06,
    -4.4167383584587505e-06,
    1.6448448070728896e-05,
    -5.754195010082104e-05,
    0.00018850288509584165,
    -0.0005763755745385824,
    0.0016394756169413357,
    -0.004324309995050576,
    0.010546460394594998,
    -0.02373741480589947,
    0.04930528423967071,
    -0.09490109704804764,
    0.17162090152220877,
    -0.3046826723431984,
    0.6767952744094761,
];

/// Cephes `i0` coefficients for |x| > 8.
const I0_B: [f64; 25] = [
    -7.233180487874754e-18,
    -4.830504485944182e-18,
    4.46562142029676e-17,
    3.461222867697461e-17,
    -2.8276239805165836e-16,
    -3.425485619677219e-16,
    1.7725601330565263e-15,
    3.8116806693526224e-15,
    -9.554846698828307e-15,
    -4.150569347287222e-14,
    1.54008621752141e-14,
    3.8527783827421426e-13,
    7.180124451383666e-13,
    -1.7941785315068062e-12,
    -1.3215811840447713e-11,
    -3.1499165279632416e-11,
    1.1889147107846439e-11,
    4.94060238822497e-10,
    3.3962320257083865e-09,
    2.266668990498178e-08,
    2.0489185894690638e-07,
    2.8913705208347567e-06,
    6.889758346916825e-05,
    0.0033691164782556943,
    0.8044904110141088,
];

/// Cephes `chbevl`: a Chebyshev series by Clenshaw's recurrence.
fn chbevl(x: f64, coefficients: &[f64]) -> f64 {
    let mut b0 = coefficients[0];
    let mut b1 = 0.0;
    let mut b2 = 0.0;
    for c in &coefficients[1..] {
        b2 = b1;
        b1 = b0;
        b0 = x * b1 - b2 + c;
    }
    0.5 * (b0 - b2)
}

/// The modified Bessel function of the first kind, order zero (`scipy.special.i0`).
pub fn i0(x: f64) -> f64 {
    let x = x.abs();
    if x <= 8.0 {
        x.exp() * chbevl(x / 2.0 - 2.0, &I0_A)
    } else {
        x.exp() * chbevl(32.0 / x - 2.0, &I0_B) / x.sqrt()
    }
}

/// numpy's pairwise summation, which `np.sum` of a float64 array uses.
pub fn pairwise_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut res = 0.0;
        for x in a {
            res += x;
        }
        res
    } else if n <= 128 {
        let mut r = [0.0; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - n % 8 {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += a[i];
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum(&a[..n2]) + pairwise_sum(&a[n2..])
    }
}

/// `firwin(numtaps, cutoff, window=("kaiser", 5.0))`: a scaled low-pass.
pub fn firwin_kaiser(numtaps: usize, cutoff: f64, beta: f64) -> Vec<f64> {
    let alpha = 0.5 * (numtaps as f64 - 1.0);
    let m: Vec<f64> = (0..numtaps).map(|i| i as f64 - alpha).collect();
    let sinc = |x: f64| {
        let y = PI * if x == 0.0 { 1.0e-20 } else { x };
        y.sin() / y
    };
    let mut h: Vec<f64> = m
        .iter()
        .map(|&mi| {
            let high = cutoff * sinc(cutoff * mi);
            let low = 0.0 * sinc(0.0 * mi);
            (0.0 + high) - low
        })
        .collect();
    // kaiser(M, beta, sym=True)
    let half = (numtaps as f64 - 1.0) / 2.0;
    let denominator = i0(beta);
    for (i, hi) in h.iter_mut().enumerate() {
        let r = (i as f64 - half) / half;
        *hi *= i0(beta * (1.0 - r * r).sqrt()) / denominator;
    }
    // Scaled to unit gain at DC: cos(pi * m * 0) is 1 for every tap.
    let s = pairwise_sum(&h.iter().map(|x| x * 1.0).collect::<Vec<_>>());
    for x in &mut h {
        *x /= s;
    }
    h
}

/// `upfirdn`'s output length.
fn output_len(len_h: usize, len_x: usize, up: usize, down: usize) -> usize {
    ((len_x - 1) * up + len_h - 1) / down + 1
}

/// A resampler for one ratio, reusable across signals.
pub struct Resampler {
    up: usize,
    down: usize,
    /// Filter coefficients transposed into phases and flipped, as `_pad_h` stores them.
    h_trans_flip: Vec<f64>,
    h_len: usize,
    n_pre_pad: usize,
    half_len: usize,
}

impl Resampler {
    /// `up` and `down` must be coprime and not both 1.
    pub fn new(up: usize, down: usize) -> Resampler {
        let max_rate = up.max(down);
        let f_c = 1.0 / max_rate as f64;
        let half_len = 10 * max_rate;
        let mut h = firwin_kaiser(2 * half_len + 1, f_c, 5.0);
        for x in &mut h {
            *x *= up as f64;
        }
        let n_pre_pad = down - half_len % down;
        let padded: Vec<f64> = std::iter::repeat_n(0.0, n_pre_pad).chain(h).collect();
        Resampler {
            up,
            down,
            h_trans_flip: Vec::new(),
            h_len: padded.len(),
            n_pre_pad,
            half_len,
        }
        .with_filter(padded)
    }

    fn with_filter(mut self, h: Vec<f64>) -> Resampler {
        self.h_trans_flip = pad_h(&h, self.up);
        self.h_len = h.len();
        self
    }

    /// Output frames for `n_in` input frames.
    pub fn output_frames(&self, n_in: usize) -> usize {
        let n = n_in * self.up;
        n / self.down + usize::from(n % self.down != 0)
    }

    /// Resamples one channel.
    pub fn apply(&self, x: &[f64]) -> Vec<f64> {
        let n_in = x.len();
        let n_out = self.output_frames(n_in);
        let n_pre_remove = (self.half_len + self.n_pre_pad) / self.down;
        // Post-padding the filter only lengthens the zero tail of the output.
        let mut n_post_pad = 0;
        while output_len(self.h_len + n_post_pad, n_in, self.up, self.down) < n_out + n_pre_remove {
            n_post_pad += 1;
        }
        let h = if n_post_pad == 0 {
            std::borrow::Cow::Borrowed(&self.h_trans_flip)
        } else {
            let mut full = unpad_h(&self.h_trans_flip, self.up, self.h_len);
            full.extend(std::iter::repeat_n(0.0, n_post_pad));
            std::borrow::Cow::Owned(pad_h(&full, self.up))
        };
        let len_out = output_len(self.h_len + n_post_pad, n_in, self.up, self.down);
        let y = upfirdn(&h, x, self.up, self.down, len_out);
        y[n_pre_remove..n_pre_remove + n_out].to_vec()
    }
}

/// `_pad_h`: coefficients zero-padded to a multiple of `up`, one flipped row per phase.
fn pad_h(h: &[f64], up: usize) -> Vec<f64> {
    let padded = h.len() + (up - h.len() % up) % up;
    let per_phase = padded / up;
    let mut out = vec![0.0; padded];
    for phase in 0..up {
        for j in 0..per_phase {
            let k = (per_phase - 1 - j) * up + phase;
            out[phase * per_phase + j] = if k < h.len() { h[k] } else { 0.0 };
        }
    }
    out
}

fn unpad_h(trans: &[f64], up: usize, len: usize) -> Vec<f64> {
    let per_phase = trans.len() / up;
    let mut h = vec![0.0; len];
    for phase in 0..up {
        for j in 0..per_phase {
            let k = (per_phase - 1 - j) * up + phase;
            if k < len {
                h[k] = trans[phase * per_phase + j];
            }
        }
    }
    h
}

/// scipy's `_apply_impl` for zero padding.
fn upfirdn(h_trans_flip: &[f64], x: &[f64], up: usize, down: usize, len_out: usize) -> Vec<f64> {
    let mut out = vec![0.0; len_out];
    let len_x = x.len() as isize;
    let h_per_phase = (h_trans_flip.len() / up) as isize;
    let padded_len = len_x + h_per_phase - 1;
    let (up, down) = (up as isize, down as isize);
    let mut x_idx: isize = 0;
    let mut y_idx = 0usize;
    let mut t: isize = 0;
    if len_out == 0 {
        return out;
    }
    while x_idx < len_x {
        let mut h_idx = t * h_per_phase;
        let mut x_conv_idx = x_idx - h_per_phase + 1;
        if x_conv_idx < 0 {
            h_idx -= x_conv_idx;
            x_conv_idx = 0;
        }
        for k in x_conv_idx..=x_idx {
            out[y_idx] = out[y_idx] + x[k as usize] * h_trans_flip[h_idx as usize];
            h_idx += 1;
        }
        y_idx += 1;
        if y_idx >= len_out {
            return out;
        }
        t += down;
        x_idx += t / up;
        t %= up;
    }
    while x_idx < padded_len {
        let mut h_idx = t * h_per_phase;
        for k in (x_idx - h_per_phase + 1)..=x_idx {
            let xval = if k >= 0 && k < len_x { x[k as usize] } else { 0.0 };
            out[y_idx] += xval * h_trans_flip[h_idx as usize];
            h_idx += 1;
        }
        y_idx += 1;
        if y_idx >= len_out {
            return out;
        }
        t += down;
        x_idx += t / up;
        t %= up;
    }
    out
}

/// `resample_poly(x, up, down, axis=0)` for interleaved frames.
pub fn resample_poly(interleaved: &[f64], channels: usize, up: usize, down: usize) -> Vec<f64> {
    let g = up.gcd(&down);
    let (up, down) = (up / g, down / g);
    if up == 1 && down == 1 {
        return interleaved.to_vec();
    }
    let r = Resampler::new(up, down);
    let frames = interleaved.len() / channels;
    let columns: Vec<Vec<f64>> = (0..channels)
        .map(|c| {
            let column: Vec<f64> = (0..frames).map(|i| interleaved[i * channels + c]).collect();
            r.apply(&column)
        })
        .collect();
    let n_out = r.output_frames(frames);
    let mut out = vec![0.0; n_out * channels];
    for (c, column) in columns.iter().enumerate() {
        for (i, v) in column.iter().enumerate() {
            out[i * channels + c] = *v;
        }
    }
    out
}

/// `Fraction(x).limit_denominator(max_denominator)` for a float.
pub fn limit_denominator(x: f64, max_denominator: u64) -> (BigInt, BigInt) {
    let exact = BigRational::from_float(x).expect("finite ratio");
    let (numer, denom) = (exact.numer().clone(), exact.denom().clone());
    let max = BigInt::from(max_denominator);
    if denom <= max {
        return (numer, denom);
    }
    let (mut p0, mut q0, mut p1, mut q1) = (BigInt::zero(), BigInt::one(), BigInt::one(), BigInt::zero());
    let (mut n, mut d) = (numer.clone(), denom.clone());
    loop {
        let a = n.div_floor(&d);
        let q2 = &q0 + &a * &q1;
        if q2 > max {
            break;
        }
        let p2 = &p0 + &a * &p1;
        (p0, q0, p1, q1) = (p1, q1, p2, q2);
        let r = &n - &a * &d;
        (n, d) = (d, r);
    }
    let k = (&max - &q0).div_floor(&q1);
    let two = BigInt::from(2);
    if &two * &d * (&q0 + &k * &q1) <= denom {
        (p1, q1)
    } else {
        (&p0 + &k * &p1, &q0 + &k * &q1)
    }
}

/// The resampling ratio for a playback speed: `Fraction(rate / source / speed)
/// .limit_denominator(8192)` as (up, down), or None when it is exactly 1.
pub fn repitch_ratio(session_rate: u32, source_rate: u32, speed: f64) -> Option<(usize, usize)> {
    let (p, q) = limit_denominator(session_rate as f64 / source_rate as f64 / speed, 8192);
    if p == q {
        return None;
    }
    assert!(p.is_positive(), "resampling ratio must be positive");
    Some((p.to_usize().expect("ratio"), q.to_usize().expect("ratio")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i0_matches_known_values() {
        assert!((i0(0.0) - 1.0).abs() < 1e-15);
        assert_eq!(i0(5.0), 27.239871823604442);
        assert_eq!(i0(2.5), 3.289839144050123);
    }

    #[test]
    fn limit_denominator_matches_python() {
        let r = |x: f64, m| {
            let (p, q) = limit_denominator(x, m);
            (p.to_i64().unwrap(), q.to_i64().unwrap())
        };
        assert_eq!(r(48000.0 / 44100.0, 8192), (160, 147));
        assert_eq!(r(std::f64::consts::PI, 1000), (355, 113));
        assert_eq!(r(0.5, 8192), (1, 2));
        // Values from Python's Fraction(x).limit_denominator(m).
        assert_eq!(r(0.9438743126816934, 8192), (7450, 7893));
        assert_eq!(r(0.3337099635425086, 8192), (886, 2655));
        assert_eq!(r(0.715859375, 8192), (4744, 6627));
    }

    #[test]
    fn resampling_keeps_length_and_passband() {
        let n = 4800;
        let x: Vec<f64> = (0..n).map(|i| (2.0 * PI * 440.0 * i as f64 / 48000.0).sin()).collect();
        let y = resample_poly(&x, 1, 147, 160);
        assert_eq!(y.len(), (n * 147 + 159) / 160);
        // A 440 Hz tone keeps its amplitude away from the edges.
        let peak = y[200..y.len() - 200].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        assert!((peak - 1.0).abs() < 1e-3, "{peak}");
    }
}

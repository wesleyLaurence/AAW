//! Measurements of rendered audio for a reply the agent reads: the loudness
//! of ITU-R BS.1770 and a spectral centroid, beside the peak `metrics` gives.

use crate::render::Frame;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

/// A biquad from its coefficients, run on one channel.
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    fn new(b: [f64; 3], a: [f64; 2]) -> Biquad {
        Biquad { b, a, z: [0.0; 2] }
    }

    #[inline]
    fn next(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The K-weighting of BS.1770 at a sample rate: a high shelf of +4 dB
/// above 1.7 kHz and a highpass at 38 Hz, designed as the standard designs
/// them, so at 48 kHz the coefficients are the standard's table.
fn k_weighting(rate: f64) -> (Biquad, Biquad) {
    let (f0, gain_db, q) = (1681.974450955533, 3.999843853973347, 0.7071752369554196);
    let k = (PI * f0 / rate).tan();
    let vh = 10f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.4996667741545416);
    let a0 = 1.0 + k / q + k * k;
    let shelf = Biquad::new(
        [(vh + vb * k / q + k * k) / a0, 2.0 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0],
        [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    );
    let (f0, q) = (38.13547087602444, 0.5003270373238773);
    let k = (PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    let high = Biquad::new([1.0, -2.0, 1.0], [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0]);
    (shelf, high)
}

/// The integrated loudness of a stereo render in LUFS, gated as BS.1770
/// gates it; None for audio too short or too quiet to measure.
pub fn loudness_lufs(x: &[Frame], rate: u32) -> Option<f64> {
    let rate_f = rate as f64;
    let block = (0.4 * rate_f).round() as usize;
    let hop = block / 4;
    if x.len() < block {
        return None;
    }
    // Each channel weighted, then its mean square over each 400 ms block.
    let mut weighted: [Vec<f64>; 2] = [Vec::with_capacity(x.len()), Vec::with_capacity(x.len())];
    for (c, out) in weighted.iter_mut().enumerate() {
        let (mut shelf, mut high) = k_weighting(rate_f);
        out.extend(x.iter().map(|f| high.next(shelf.next(f[c]))));
    }
    let mut blocks: Vec<f64> = Vec::new();
    let mut at = 0;
    while at + block <= x.len() {
        let power: f64 = weighted
            .iter()
            .map(|w| w[at..at + block].iter().map(|v| v * v).sum::<f64>() / block as f64)
            .sum();
        blocks.push(-0.691 + 10.0 * power.max(1e-20).log10());
        at += hop;
    }
    let mean = |lk: &[f64]| {
        let power: f64 = lk.iter().map(|l| 10f64.powf((l + 0.691) / 10.0)).sum::<f64>() / lk.len() as f64;
        -0.691 + 10.0 * power.log10()
    };
    let above: Vec<f64> = blocks.iter().copied().filter(|l| *l > -70.0).collect();
    if above.is_empty() {
        return None;
    }
    let relative = mean(&above) - 10.0;
    let gated: Vec<f64> = above.into_iter().filter(|l| *l > relative).collect();
    if gated.is_empty() {
        return None;
    }
    Some(mean(&gated))
}

/// The spectral centroid of a stereo render in Hz: the power-weighted mean
/// frequency over Hann windows of 4096 frames, each weighted by its power.
/// None for audio too short or silent.
pub fn spectral_centroid_hz(x: &[Frame], rate: u32) -> Option<f64> {
    let n = 4096;
    if x.len() < n {
        return None;
    }
    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(n);
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let window: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos()).collect();
    let bin = rate as f64 / n as f64;
    let (mut weighted, mut power) = (0.0, 0.0);
    let mut at = 0;
    while at + n <= x.len() {
        for (i, v) in input.iter_mut().enumerate() {
            let f = x[at + i];
            *v = (f[0] + f[1]) * 0.5 * window[i];
        }
        fft.process(&mut input, &mut output).ok()?;
        for (k, c) in output.iter().enumerate().skip(1) {
            let p = c.norm_sqr();
            weighted += p * k as f64 * bin;
            power += p;
        }
        at += n / 2;
    }
    (power > 0.0).then(|| weighted / power)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sine on the left channel alone.
    fn sine(hz: f64, seconds: f64, level: f64) -> Vec<Frame> {
        (0..(48000.0 * seconds) as usize)
            .map(|i| {
                let v = (2.0 * PI * hz * i as f64 / 48000.0).sin() * level;
                [v, 0.0]
            })
            .collect()
    }

    #[test]
    fn a_full_scale_1khz_sine_is_near_minus_three_lufs_and_centered_on_1khz() {
        // BS.1770 puts a 997 Hz sine at 0 dBFS on one channel at -3.01 LUFS.
        let x = sine(997.0, 3.0, 1.0);
        let lufs = loudness_lufs(&x, 48000).unwrap();
        assert!((lufs + 3.01).abs() < 0.1, "{lufs}");
        let quieter = loudness_lufs(&sine(997.0, 3.0, 0.1), 48000).unwrap();
        assert!((lufs - quieter - 20.0).abs() < 0.1, "{lufs} against {quieter}");
        let centroid = spectral_centroid_hz(&x, 48000).unwrap();
        assert!((centroid - 997.0).abs() < 15.0, "{centroid}");
        assert!(loudness_lufs(&vec![[0.0; 2]; 48000], 48000).is_none());
        assert!(loudness_lufs(&sine(100.0, 0.1, 1.0), 48000).is_none());
    }
}

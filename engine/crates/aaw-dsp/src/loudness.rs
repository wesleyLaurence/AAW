//! Loudness from a stream of frames, as BS.1770 and EBU Tech 3342 have it:
//! the gated loudness of the whole, the loudness range, and the loudest
//! 400 ms and 3 s. The analyzer's meter runs it on what plays, and
//! `daw listen` on a saved render through `aaw_py`, so the window and the
//! report are one implementation.
//!
//! The frames' K-weighted power is kept as the mean of each 100 ms, and
//! every loudness is read from those blocks: momentary over four of them
//! and short-term over thirty, a block apart.

use crate::Frame;
use std::f64::consts::PI;

/// A biquad from its coefficients, run on one channel.
#[derive(Clone, Debug)]
pub struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}

impl Biquad {
    pub fn new(b: [f64; 3], a: [f64; 2]) -> Biquad {
        Biquad { b, a, z: [0.0; 2] }
    }

    #[inline]
    pub fn next(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

/// The K-weighting of BS.1770 at a sample rate: a high shelf of +4 dB
/// above 1.7 kHz and a highpass at 38 Hz, designed as the standard designs
/// them, so at 48 kHz the coefficients are the standard's table.
pub fn k_weighting(rate: f64) -> (Biquad, Biquad) {
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

/// Loudness in LUFS of a K-weighted mean square summed over the channels.
#[inline]
pub fn lufs(power: f64) -> f64 {
    -0.691 + 10.0 * power.max(1e-20).log10()
}

/// The mean of loudness values, in the power domain.
fn mean_lufs(values: impl Iterator<Item = f64>) -> Option<f64> {
    let (mut sum, mut n) = (0.0, 0usize);
    for l in values {
        sum += 10f64.powf((l + 0.691) / 10.0);
        n += 1;
    }
    (n > 0).then(|| lufs(sum / n as f64))
}

/// The absolute gate: a loudness under it is not one.
const GATE_LUFS: f64 = -70.0;
/// Blocks of 100 ms in a momentary window and in a short-term one.
const MOMENTARY: usize = 4;
const SHORT_TERM: usize = 30;

/// The loudness of the frames fed since the last `reset`.
#[derive(Clone, Debug)]
pub struct Loudness {
    weighting: [(Biquad, Biquad); 2],
    /// The K-weighted power of the 100 ms block being filled, and the
    /// frames in it.
    block: (f64, usize),
    block_frames: usize,
    /// The mean power of each 100 ms block.
    blocks: Vec<f64>,
    /// The highest mean power of four blocks in a row and of thirty, once
    /// that many have been fed.
    loudest: [Option<f64>; 2],
}

/// What `measure` reads from a stretch of audio; None where there is too
/// little of it, or it is under the gate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Summary {
    pub integrated_lufs: Option<f64>,
    pub range_lu: Option<f64>,
    pub max_short_term_lufs: Option<f64>,
    pub max_momentary_lufs: Option<f64>,
}

impl Loudness {
    pub fn new(rate: u32) -> Loudness {
        let rate = rate as f64;
        Loudness {
            weighting: [k_weighting(rate), k_weighting(rate)],
            block: (0.0, 0),
            block_frames: (0.1 * rate).round() as usize,
            blocks: Vec::new(),
            loudest: [None; 2],
        }
    }

    /// Feeds a frame. Returns its K-weighted power, summed over the channels
    /// as BS.1770 sums them, and whether it filled a 100 ms block, which is
    /// when the loudnesses move.
    #[inline]
    pub fn feed(&mut self, f: &Frame) -> (f64, bool) {
        let mut power = 0.0;
        for c in 0..2 {
            let (shelf, high) = &mut self.weighting[c];
            let w = high.next(shelf.next(f[c]));
            power += w * w;
        }
        self.block.0 += power;
        self.block.1 += 1;
        if self.block.1 < self.block_frames {
            return (power, false);
        }
        self.blocks.push(self.block.0 / self.block_frames as f64);
        self.block = (0.0, 0);
        for (loudest, len) in self.loudest.iter_mut().zip([MOMENTARY, SHORT_TERM]) {
            if self.blocks.len() >= len {
                let mean = self.blocks[self.blocks.len() - len..].iter().sum::<f64>() / len as f64;
                *loudest = Some(loudest.map_or(mean, |was| was.max(mean)));
            }
        }
        (power, true)
    }

    /// Starts again, as when the song is played from a stop. The weighting
    /// keeps its state: the audio goes on.
    pub fn reset(&mut self) {
        self.blocks.clear();
        self.block = (0.0, 0);
        self.loudest = [None; 2];
    }

    /// The loudness of the last 3 s as it runs: before 3 s have been fed,
    /// what came before them counts as silence.
    pub fn short_term(&self) -> f64 {
        let last = &self.blocks[self.blocks.len().saturating_sub(SHORT_TERM)..];
        lufs(last.iter().sum::<f64>() / SHORT_TERM as f64)
    }

    /// The gated loudness: the momentary blocks over the absolute gate and
    /// within 10 LU under their mean. None until 400 ms over the gate.
    pub fn integrated(&self) -> Option<f64> {
        let above: Vec<f64> = self
            .blocks
            .windows(MOMENTARY)
            .map(|w| lufs(w.iter().sum::<f64>() / MOMENTARY as f64))
            .filter(|l| *l > GATE_LUFS)
            .collect();
        let relative = mean_lufs(above.iter().copied())? - 10.0;
        mean_lufs(above.into_iter().filter(|l| *l > relative))
    }

    /// The loudness range in LU: of the short-term values over the absolute
    /// gate and within 20 LU under their mean, the 10th to the 95th
    /// percentile, as EBU Tech 3342 has it. None until two of them.
    pub fn range(&self) -> Option<f64> {
        let above: Vec<f64> = self
            .blocks
            .windows(SHORT_TERM)
            .map(|w| lufs(w.iter().sum::<f64>() / SHORT_TERM as f64))
            .filter(|l| *l > GATE_LUFS)
            .collect();
        let mean = mean_lufs(above.iter().copied())?;
        let mut gated: Vec<f64> = above.into_iter().filter(|l| *l > mean - 20.0).collect();
        if gated.len() < 2 {
            return None;
        }
        gated.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| gated[((gated.len() - 1) as f64 * q).round() as usize];
        Some(at(0.95) - at(0.10))
    }

    /// The loudest 400 ms, of the windows a block apart; None before 400 ms
    /// or under the gate.
    pub fn max_momentary(&self) -> Option<f64> {
        self.loudest[0].map(lufs).filter(|l| *l > GATE_LUFS)
    }

    /// The loudest 3 s, as `max_momentary`.
    pub fn max_short_term(&self) -> Option<f64> {
        self.loudest[1].map(lufs).filter(|l| *l > GATE_LUFS)
    }

    pub fn summary(&self) -> Summary {
        Summary {
            integrated_lufs: self.integrated(),
            range_lu: self.range(),
            max_short_term_lufs: self.max_short_term(),
            max_momentary_lufs: self.max_momentary(),
        }
    }
}

/// The loudness of a stretch of audio.
pub fn measure(frames: impl IntoIterator<Item = Frame>, rate: u32) -> Summary {
    let mut loudness = Loudness::new(rate);
    for f in frames {
        loudness.feed(&f);
    }
    loudness.summary()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: f64, amplitude: f64, rate: f64) -> impl Iterator<Item = Frame> {
        (0..(rate * seconds) as usize).map(move |i| [(2.0 * PI * 1000.0 * i as f64 / rate).sin() * amplitude; 2])
    }

    #[test]
    fn a_tone_at_two_levels_has_their_difference_as_its_range() {
        // Twenty seconds 10 dB over the twenty that follow: the short-term
        // values sit at the two levels, but for the 3 s between them.
        let rate = 48000;
        let s = measure(tone(20.0, 0.1, 48000.0).chain(tone(20.0, 0.1 * 10f64.powf(-0.5), 48000.0)), rate);
        let range = s.range_lu.expect("forty seconds");
        assert!((range - 10.0).abs() < 0.1, "{range}");
        // A 1 kHz sine on both channels reads its amplitude in dB: -3 for a
        // sine, +3 for the two channels.
        let loud = s.max_short_term_lufs.unwrap();
        assert!((loud + 20.0).abs() < 0.1, "{loud}");
        assert!((s.max_momentary_lufs.unwrap() - loud).abs() < 0.01);
        // The gated loudness is the loud half's, less what the quiet half
        // adds to the mean: both are within 10 LU of it.
        let whole = s.integrated_lufs.unwrap();
        assert!((whole - (-20.0 + 10.0 * (1.1f64 / 2.0).log10())).abs() < 0.1, "{whole}");
        // A steady tone has no range.
        let steady = measure(tone(10.0, 0.1, 48000.0), rate);
        assert!(steady.range_lu.unwrap() < 0.05);
    }

    #[test]
    fn the_loudest_windows_are_found_and_short_audio_has_none() {
        let rate = 48000;
        // A second 12 dB up in ten: the loudest 400 ms holds all of it, and
        // the loudest 3 s a third of it.
        let quiet = 0.05;
        let x: Vec<Frame> = tone(4.0, quiet, 48000.0).chain(tone(1.0, quiet * 4.0, 48000.0)).chain(tone(5.0, quiet, 48000.0)).collect();
        let s = measure(x.iter().copied(), rate);
        let base = 20.0 * quiet.log10();
        assert!((s.max_momentary_lufs.unwrap() - (base + 12.04)).abs() < 0.15, "{:?}", s);
        let third = 10.0 * ((16.0f64 + 2.0) / 3.0).log10();
        assert!((s.max_short_term_lufs.unwrap() - (base + third)).abs() < 0.15, "{:?}", s);
        // Under 3 s there is no short-term loudness and no range; under
        // 400 ms nothing; and silence is under the gate.
        let short = measure(tone(2.0, 0.1, 48000.0), rate);
        assert!(short.max_short_term_lufs.is_none() && short.range_lu.is_none());
        assert!(short.max_momentary_lufs.is_some() && short.integrated_lufs.is_some());
        let shorter = measure(tone(0.3, 0.1, 48000.0), rate);
        assert_eq!(shorter, Summary { integrated_lufs: None, range_lu: None, max_short_term_lufs: None, max_momentary_lufs: None });
        let silent = measure(vec![[0.0; 2]; 48000 * 5], rate);
        assert_eq!(silent, shorter);
        // A reset starts all of it again.
        let mut l = Loudness::new(rate);
        for f in &x {
            l.feed(f);
        }
        l.reset();
        assert_eq!(l.summary(), shorter);
    }

    #[test]
    fn k_weighting_at_48k_is_the_standard_s_table() {
        let (shelf, high) = k_weighting(48000.0);
        assert!((shelf.b[0] - 1.53512485958697).abs() < 1e-9);
        assert!((shelf.a[0] + 1.69065929318241).abs() < 1e-9);
        assert!((high.a[0] + 1.99004745483398).abs() < 1e-9);
    }
}

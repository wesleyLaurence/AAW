//! What an analyzer shows: the stereo ring the audio thread writes every
//! frame it passes to, and the meter that reads the ring off the audio
//! thread and works out levels, loudness, a spectrum, the stereo field,
//! and the columns of a spectrogram and a scrolling waveform.
//!
//! The ring is `RING_FRAMES` frames, both channels packed into one atomic
//! store a frame, with a count of frames written, so that a reader takes
//! what has come since it last read: every frame once, in order, unless it
//! stayed away for longer than the ring holds. The audio thread never waits
//! and never allocates.

use crate::spectrum::{Analyzer, BINS, TAP_FRAMES};
use crate::Frame;
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Frames the ring keeps: 1.37 s at 48 kHz, 1.49 s at 44.1 kHz.
pub const RING_FRAMES: usize = 65536;

/// The last `RING_FRAMES` frames a device put out, both channels.
#[derive(Debug)]
pub struct Ring {
    samples: Box<[AtomicU64]>,
    written: AtomicU64,
}

impl Default for Ring {
    fn default() -> Ring {
        Ring {
            samples: (0..RING_FRAMES).map(|_| AtomicU64::new(0)).collect(),
            written: AtomicU64::new(0),
        }
    }
}

#[inline]
fn pack(f: &Frame) -> u64 {
    ((f[0] as f32).to_bits() as u64) << 32 | (f[1] as f32).to_bits() as u64
}

#[inline]
fn unpack(bits: u64) -> Frame {
    [f32::from_bits((bits >> 32) as u32) as f64, f32::from_bits(bits as u32) as f64]
}

impl Ring {
    /// Writes a block, on the audio thread.
    #[inline]
    pub fn write(&self, x: &[Frame]) {
        let start = self.written.load(Relaxed) as usize;
        for (i, f) in x.iter().enumerate() {
            self.samples[(start + i) & (RING_FRAMES - 1)].store(pack(f), Relaxed);
        }
        self.written.fetch_add(x.len() as u64, Relaxed);
    }

    /// Frames written so far.
    pub fn written(&self) -> u64 {
        self.written.load(Relaxed)
    }

    /// The frames written since `from`, oldest first, appended to `out`, and
    /// the count to read from next time. A reader that stayed away longer
    /// than the ring holds gets the last `RING_FRAMES` frames and the count
    /// of the ones it lost.
    pub fn read_since(&self, from: u64, out: &mut Vec<Frame>) -> (u64, u64) {
        let end = self.written.load(Relaxed);
        let available = end.saturating_sub(from);
        let lost = available.saturating_sub(RING_FRAMES as u64);
        let start = end - available.min(RING_FRAMES as u64);
        for at in start..end {
            out.push(unpack(self.samples[at as usize & (RING_FRAMES - 1)].load(Relaxed)));
        }
        (end, lost)
    }
}

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
fn lufs(power: f64) -> f64 {
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

/// Taps of the interpolating filter a true peak is estimated with: four
/// phases of twelve, a windowed sinc at a quarter of the oversampled rate.
const TRUE_PEAK_TAPS: usize = 48;

fn true_peak_filter() -> [f64; TRUE_PEAK_TAPS] {
    let center = (TRUE_PEAK_TAPS as f64 - 1.0) / 2.0;
    let mut taps = [0.0; TRUE_PEAK_TAPS];
    for (n, tap) in taps.iter_mut().enumerate() {
        let t = (n as f64 - center) / 4.0;
        let sinc = if t.abs() < 1e-12 { 1.0 } else { (PI * t).sin() / (PI * t) };
        let window = 0.5 - 0.5 * (2.0 * PI * (n as f64 + 0.5) / TRUE_PEAK_TAPS as f64).cos();
        *tap = sinc * window;
    }
    taps
}

/// A sample's level in dB relative to full scale; silence reads −200.
pub fn db(x: f64) -> f32 {
    (20.0 * x.max(1e-10).log10()) as f32
}

/// The sum of a window's last `len` values, kept as frames arrive and
/// summed afresh each time the window wraps, so it does not drift.
#[derive(Clone, Debug)]
struct Window {
    values: Vec<f64>,
    at: usize,
    sum: f64,
}

impl Window {
    fn new(len: usize) -> Window {
        Window {
            values: vec![0.0; len.max(1)],
            at: 0,
            sum: 0.0,
        }
    }

    #[inline]
    fn push(&mut self, x: f64) {
        self.sum += x - self.values[self.at];
        self.values[self.at] = x;
        self.at += 1;
        if self.at == self.values.len() {
            self.at = 0;
            self.sum = self.values.iter().sum();
        }
    }

    /// The mean of a window of squares, which cannot be below zero.
    fn mean(&self) -> f64 {
        self.mean_signed().max(0.0)
    }

    fn mean_signed(&self) -> f64 {
        self.sum / self.values.len() as f64
    }
}

/// What a meter has measured, for a picture of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Reading {
    /// Each channel's highest sample since the last reading, in dB.
    pub peak_db: [f32; 2],
    /// Each channel's highest sample since the meter was reset.
    pub peak_hold_db: [f32; 2],
    /// Each channel's RMS over the last 400 ms.
    pub rms_db: [f32; 2],
    /// Each channel's highest estimated true peak since the last reading,
    /// four times oversampled, in dBTP.
    pub true_peak_db: [f32; 2],
    /// Each channel's highest true peak since the meter was reset.
    pub true_peak_hold_db: [f32; 2],
    /// The loudness of the last 400 ms.
    pub momentary_lufs: f32,
    /// The loudness of the last 3 s.
    pub short_term_lufs: f32,
    /// The gated loudness since the meter was reset; None until 400 ms
    /// above the gate have played.
    pub integrated_lufs: Option<f32>,
    /// The loudness range since the meter was reset, in LU; None until
    /// enough has played.
    pub range_lu: Option<f32>,
    /// The short-term loudness every 100 ms, oldest first, the last minutes.
    pub history: Vec<f32>,
    /// The level of each bin of the last 4096 frames as mono, in dB.
    pub spectrum: Vec<f32>,
    /// The highest level of each bin since the meter was reset.
    pub spectrum_hold: Vec<f32>,
    /// The correlation of the channels over the last 400 ms, −1 to +1;
    /// 0 for silence.
    pub correlation: f32,
    /// The right channel's RMS over the left's, in dB; 0 for silence.
    pub balance_db: f32,
    /// The last `SCOPE_FRAMES` frames, left and right interleaved, oldest
    /// first, for a vectorscope.
    pub scope: Vec<f32>,
    /// Spectrogram columns made since the meter began, `SPECTROGRAM_PER_SECOND`
    /// a second, counting the blank ones for frames the watcher missed.
    pub spectrogram_columns: u64,
    /// The last `SPECTROGRAM_KEEP` columns at most, oldest first, each
    /// `SPECTROGRAM_ROWS` levels in dB from the lowest band up; a watcher
    /// takes the ones past its count.
    pub spectrogram: Vec<f32>,
    /// Waveform columns made since the meter began, `WAVEFORM_PER_SECOND` a
    /// second, the blank ones counted.
    pub waveform_columns: u64,
    /// The last `WAVEFORM_KEEP` columns at most, oldest first, each the left
    /// channel's lowest and highest sample in its hop then the right's.
    pub waveform: Vec<f32>,
}

/// Frames a reading holds for the vectorscope.
pub const SCOPE_FRAMES: usize = 1024;

/// Short-term loudness values a reading keeps: five minutes at ten a second.
pub const HISTORY: usize = 3000;

/// The spectrogram's rows: bands of equal width in pitch from
/// `SPECTROGRAM_LOW_HZ` to `SPECTROGRAM_HIGH_HZ`, the range the spectrum is
/// drawn over, each the loudest bin in it or the level between the bins
/// either side where the bins are further apart than the rows.
pub const SPECTROGRAM_ROWS: usize = 256;
pub const SPECTROGRAM_LOW_HZ: f64 = 20.0;
pub const SPECTROGRAM_HIGH_HZ: f64 = 20000.0;
/// Columns a second: a spectrum of the last 4096 frames every 20 ms, and
/// the waveform's lowest and highest sample every 10 ms.
pub const SPECTROGRAM_PER_SECOND: u32 = 50;
pub const WAVEFORM_PER_SECOND: u32 = 100;
/// Columns a reading holds of each: well over the frame a watcher takes
/// between readings, so one a few frames behind misses none.
pub const SPECTROGRAM_KEEP: usize = 32;
pub const WAVEFORM_KEEP: usize = 64;

/// Each spectrogram row's reach in bins of a `TAP_FRAMES` analysis at the
/// rate, as fractions, lowest band first.
pub fn spectrogram_rows(rate: f64) -> Vec<(f64, f64)> {
    let span = SPECTROGRAM_HIGH_HZ / SPECTROGRAM_LOW_HZ;
    let bin = |hz: f64| hz / rate * TAP_FRAMES as f64;
    (0..SPECTROGRAM_ROWS)
        .map(|r| {
            let f0 = SPECTROGRAM_LOW_HZ * span.powf(r as f64 / SPECTROGRAM_ROWS as f64);
            let f1 = SPECTROGRAM_LOW_HZ * span.powf((r + 1) as f64 / SPECTROGRAM_ROWS as f64);
            (bin(f0), bin(f1))
        })
        .collect()
}

/// A spectrum's level in each spectrogram row: the loudest bin of a row
/// wider than a bin, or the level between the two bins around the row's
/// middle; −200 above the analysis's top.
pub fn spectrogram_column(spectrum: &[f32], rows: &[(f64, f64)]) -> Vec<f32> {
    rows.iter()
        .map(|&(b0, b1)| {
            if b1 - b0 >= 1.0 {
                let (lo, hi) = (b0.ceil() as usize, (b1.floor() as usize).min(spectrum.len().saturating_sub(1)));
                if lo > hi {
                    return -200.0;
                }
                spectrum[lo..=hi].iter().copied().fold(-200.0, f32::max)
            } else {
                let b = (b0 + b1) / 2.0;
                let i = b.floor() as usize;
                if i + 1 >= spectrum.len() {
                    return -200.0;
                }
                let t = (b - i as f64) as f32;
                spectrum[i] * (1.0 - t) + spectrum[i + 1] * t
            }
        })
        .collect()
}

/// The measurements of a stream of frames, fed off the audio thread from a
/// `Ring`, and read for each picture drawn. Integrated loudness, the
/// loudness range and the held peaks run from the last `reset`.
pub struct Meter {
    rate: f64,
    peak: [f64; 2],
    peak_hold: [f64; 2],
    true_peak: [f64; 2],
    true_peak_hold: [f64; 2],
    /// The last twelve samples of each channel, for the interpolation.
    recent: [[f64; 12]; 2],
    taps: [f64; TRUE_PEAK_TAPS],
    squares: [Window; 2],
    product: Window,
    weighting: [(Biquad, Biquad); 2],
    weighted: Window,
    /// The K-weighted power of the 100 ms block being filled, and the
    /// frames in it.
    block: (f64, usize),
    block_frames: usize,
    /// The power of each 100 ms block since the reset.
    blocks: Vec<f64>,
    integrated: Option<f64>,
    range: Option<f64>,
    short_term: f64,
    history: VecDeque<f32>,
    analyzer: Analyzer,
    /// The last 4096 frames as mono, for the spectrum.
    mono: [f64; TAP_FRAMES],
    mono_at: usize,
    spectrum_hold: Vec<f32>,
    scope: VecDeque<Frame>,
    /// The spectrogram: frames between columns, frames into the current
    /// hop, each row's reach in bins, the last columns and the count.
    spectrogram_hop: usize,
    spectrogram_at: usize,
    rows: Vec<(f64, f64)>,
    spectrogram: VecDeque<Vec<f32>>,
    spectrogram_columns: u64,
    /// The waveform: frames between columns, frames into the current hop,
    /// the lowest and highest sample of each channel so far in it, the last
    /// columns and the count.
    waveform_hop: usize,
    waveform_at: usize,
    waveform_column: [f64; 4],
    waveform: VecDeque<[f32; 4]>,
    waveform_columns: u64,
}

/// A waveform column before any frame: silence.
const BLANK_WAVEFORM: [f64; 4] = [0.0; 4];

impl Meter {
    pub fn new(rate: u32) -> Meter {
        let rate_f = rate as f64;
        let window = (0.4 * rate_f).round() as usize;
        Meter {
            rate: rate_f,
            peak: [0.0; 2],
            peak_hold: [0.0; 2],
            true_peak: [0.0; 2],
            true_peak_hold: [0.0; 2],
            recent: [[0.0; 12]; 2],
            taps: true_peak_filter(),
            squares: [Window::new(window), Window::new(window)],
            product: Window::new(window),
            weighting: [k_weighting(rate_f), k_weighting(rate_f)],
            weighted: Window::new(window),
            block: (0.0, 0),
            block_frames: (0.1 * rate_f).round() as usize,
            blocks: Vec::new(),
            integrated: None,
            range: None,
            short_term: lufs(0.0),
            history: VecDeque::with_capacity(HISTORY),
            analyzer: Analyzer::default(),
            mono: [0.0; TAP_FRAMES],
            mono_at: 0,
            spectrum_hold: vec![-200.0; BINS],
            scope: VecDeque::with_capacity(SCOPE_FRAMES),
            spectrogram_hop: (rate_f / SPECTROGRAM_PER_SECOND as f64).round().max(1.0) as usize,
            spectrogram_at: 0,
            rows: spectrogram_rows(rate_f),
            spectrogram: VecDeque::with_capacity(SPECTROGRAM_KEEP + 1),
            spectrogram_columns: 0,
            waveform_hop: (rate_f / WAVEFORM_PER_SECOND as f64).round().max(1.0) as usize,
            waveform_at: 0,
            waveform_column: BLANK_WAVEFORM,
            waveform: VecDeque::with_capacity(WAVEFORM_KEEP + 1),
            waveform_columns: 0,
        }
    }

    /// Frames the watcher missed, before the frames that follow them: the
    /// time passes as blank columns, so the spectrogram and the waveform
    /// keep their pace across the gap.
    pub fn skipped(&mut self, frames: u64) {
        if frames == 0 {
            return;
        }
        // Only the columns a reading holds need making; the rest are counted.
        let at = self.spectrogram_at as u64 + frames;
        let columns = at / self.spectrogram_hop as u64;
        for _ in 0..columns.min(SPECTROGRAM_KEEP as u64) {
            self.push_spectrogram(vec![-200.0; SPECTROGRAM_ROWS]);
        }
        self.spectrogram_columns += columns.saturating_sub(SPECTROGRAM_KEEP as u64);
        self.spectrogram_at = (at % self.spectrogram_hop as u64) as usize;
        let at = self.waveform_at as u64 + frames;
        let columns = at / self.waveform_hop as u64;
        for _ in 0..columns.min(WAVEFORM_KEEP as u64) {
            self.push_waveform(BLANK_WAVEFORM);
        }
        self.waveform_columns += columns.saturating_sub(WAVEFORM_KEEP as u64);
        if columns > 0 {
            self.waveform_column = BLANK_WAVEFORM;
        }
        self.waveform_at = (at % self.waveform_hop as u64) as usize;
    }

    fn push_spectrogram(&mut self, column: Vec<f32>) {
        if self.spectrogram.len() == SPECTROGRAM_KEEP {
            self.spectrogram.pop_front();
        }
        self.spectrogram.push_back(column);
        self.spectrogram_columns += 1;
    }

    fn push_waveform(&mut self, column: [f64; 4]) {
        if self.waveform.len() == WAVEFORM_KEEP {
            self.waveform.pop_front();
        }
        self.waveform.push_back(column.map(|v| v as f32));
        self.waveform_columns += 1;
    }

    /// The spectrum of the last `TAP_FRAMES` frames as mono, oldest first.
    fn spectrum(&mut self) -> Vec<f32> {
        let mut frames = [0.0; TAP_FRAMES];
        for (i, f) in frames.iter_mut().enumerate() {
            *f = self.mono[(self.mono_at + i) & (TAP_FRAMES - 1)];
        }
        self.analyzer.analyze_frames(&frames)
    }

    /// Starts the integrated loudness, the loudness range, the history and
    /// the held peaks again, as when the song is played from a stop.
    pub fn reset(&mut self) {
        self.peak_hold = [0.0; 2];
        self.true_peak_hold = [0.0; 2];
        self.blocks.clear();
        self.block = (0.0, 0);
        self.integrated = None;
        self.range = None;
        self.history.clear();
        self.spectrum_hold.iter_mut().for_each(|l| *l = -200.0);
    }

    /// Feeds frames, in order.
    pub fn feed(&mut self, x: &[Frame]) {
        for f in x {
            let mut power = 0.0;
            for c in 0..2 {
                let v = f[c];
                let a = v.abs();
                if a > self.peak[c] {
                    self.peak[c] = a;
                }
                // The true peak: the four samples between this one and the
                // last, from the filter's phases over the recent samples.
                let recent = &mut self.recent[c];
                recent.copy_within(1.., 0);
                recent[11] = v;
                for phase in 0..4 {
                    let mut y = 0.0;
                    for (k, r) in recent.iter().enumerate() {
                        y += r * self.taps[phase + 4 * (11 - k)];
                    }
                    let a = y.abs();
                    if a > self.true_peak[c] {
                        self.true_peak[c] = a;
                    }
                }
                self.squares[c].push(v * v);
                let (shelf, high) = &mut self.weighting[c];
                let w = high.next(shelf.next(v));
                power += w * w;
            }
            // The K-weighted power of the frame, summed over the channels as
            // BS.1770 sums them.
            self.weighted.push(power);
            self.block.0 += power;
            self.product.push(f[0] * f[1]);
            self.mono[self.mono_at] = (f[0] + f[1]) * 0.5;
            self.mono_at = (self.mono_at + 1) & (TAP_FRAMES - 1);
            if self.scope.len() == SCOPE_FRAMES {
                self.scope.pop_front();
            }
            self.scope.push_back(*f);
            // The waveform's column: the lowest and highest of each channel.
            let w = &mut self.waveform_column;
            w[0] = w[0].min(f[0]);
            w[1] = w[1].max(f[0]);
            w[2] = w[2].min(f[1]);
            w[3] = w[3].max(f[1]);
            self.waveform_at += 1;
            if self.waveform_at == self.waveform_hop {
                self.waveform_at = 0;
                let column = std::mem::replace(&mut self.waveform_column, BLANK_WAVEFORM);
                self.push_waveform(column);
            }
            self.spectrogram_at += 1;
            if self.spectrogram_at == self.spectrogram_hop {
                self.spectrogram_at = 0;
                let spectrum = self.spectrum();
                let column = spectrogram_column(&spectrum, &self.rows);
                self.push_spectrogram(column);
            }
            self.block.1 += 1;
            if self.block.1 == self.block_frames {
                self.block_done();
            }
        }
        for c in 0..2 {
            self.peak_hold[c] = self.peak_hold[c].max(self.peak[c]);
            self.true_peak_hold[c] = self.true_peak_hold[c].max(self.true_peak[c]);
        }
    }

    /// A 100 ms block is full: its mean power joins the blocks, and the
    /// loudnesses that move at this pace are worked out.
    fn block_done(&mut self) {
        let power = self.block.0 / self.block_frames as f64;
        self.blocks.push(power);
        self.block = (0.0, 0);
        // Momentary blocks of 400 ms at this hop, gated as BS.1770 gates them.
        let momentary: Vec<f64> = self.blocks.windows(4).map(|w| lufs(w.iter().sum::<f64>() / 4.0)).collect();
        let above: Vec<f64> = momentary.iter().copied().filter(|l| *l > -70.0).collect();
        self.integrated = mean_lufs(above.iter().copied()).and_then(|mean| {
            let relative = mean - 10.0;
            mean_lufs(above.iter().copied().filter(|l| *l > relative))
        });
        // Short-term: the last 3 s, and the range of the short-term values
        // gated at −70 and −20 LU under their mean, the 10th to the 95th
        // percentile, as EBU Tech 3342 has it.
        let n = self.blocks.len();
        let last = &self.blocks[n.saturating_sub(30)..];
        self.short_term = lufs(last.iter().sum::<f64>() / last.len().max(30) as f64);
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(self.short_term as f32);
        let short: Vec<f64> = self.blocks.windows(30).map(|w| lufs(w.iter().sum::<f64>() / 30.0)).collect();
        let above: Vec<f64> = short.into_iter().filter(|l| *l > -70.0).collect();
        self.range = mean_lufs(above.iter().copied()).and_then(|mean| {
            let mut gated: Vec<f64> = above.into_iter().filter(|l| *l > mean - 20.0).collect();
            if gated.len() < 2 {
                return None;
            }
            gated.sort_by(|a, b| a.total_cmp(b));
            let at = |q: f64| gated[((gated.len() - 1) as f64 * q).round() as usize];
            Some(at(0.95) - at(0.10))
        });
    }

    /// What has been measured, and the peaks since the last reading start
    /// again.
    pub fn read(&mut self) -> Reading {
        let rms = [self.squares[0].mean().sqrt(), self.squares[1].mean().sqrt()];
        let correlation = if rms[0] > 1e-9 && rms[1] > 1e-9 {
            (self.product.mean_signed() / (rms[0] * rms[1])).clamp(-1.0, 1.0)
        } else {
            0.0
        };
        let balance_db = if rms[0] > 1e-9 && rms[1] > 1e-9 { 20.0 * (rms[1] / rms[0]).log10() } else { 0.0 };
        let spectrum = self.spectrum();
        for (hold, level) in self.spectrum_hold.iter_mut().zip(&spectrum) {
            if *level > *hold {
                *hold = *level;
            }
        }
        let reading = Reading {
            peak_db: [db(self.peak[0]), db(self.peak[1])],
            peak_hold_db: [db(self.peak_hold[0]), db(self.peak_hold[1])],
            rms_db: [db(rms[0]), db(rms[1])],
            true_peak_db: [db(self.true_peak[0]), db(self.true_peak[1])],
            true_peak_hold_db: [db(self.true_peak_hold[0]), db(self.true_peak_hold[1])],
            momentary_lufs: lufs(self.weighted.mean()) as f32,
            short_term_lufs: self.short_term as f32,
            integrated_lufs: self.integrated.map(|l| l as f32),
            range_lu: self.range.map(|r| r as f32),
            history: self.history.iter().copied().collect(),
            spectrum,
            spectrum_hold: self.spectrum_hold.clone(),
            correlation: correlation as f32,
            balance_db: balance_db as f32,
            scope: self.scope.iter().flat_map(|f| [f[0] as f32, f[1] as f32]).collect(),
            spectrogram_columns: self.spectrogram_columns,
            spectrogram: self.spectrogram.iter().flatten().copied().collect(),
            waveform_columns: self.waveform_columns,
            waveform: self.waveform.iter().flatten().copied().collect(),
        };
        self.peak = [0.0; 2];
        self.true_peak = [0.0; 2];
        reading
    }

    /// Frames a second.
    pub fn rate(&self) -> f64 {
        self.rate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f64, seconds: f64, level: [f64; 2], rate: f64) -> Vec<Frame> {
        (0..(rate * seconds) as usize)
            .map(|i| {
                let v = (2.0 * PI * hz * i as f64 / rate).sin();
                [v * level[0], v * level[1]]
            })
            .collect()
    }

    fn noise(n: usize, seed: u64) -> Vec<f64> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                ((s >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 2.0
            })
            .collect()
    }

    #[test]
    fn the_ring_hands_every_frame_over_once_in_order() {
        let ring = Ring::default();
        let mut out = Vec::new();
        assert_eq!(ring.read_since(0, &mut out), (0, 0));
        assert!(out.is_empty());
        let frames: Vec<Frame> = (0..150_000).map(|i| [i as f64, -(i as f64)]).collect();
        let mut next = 0;
        let mut got: Vec<Frame> = Vec::new();
        // Written in blocks of 480 and read every few blocks: nothing is
        // missed and nothing read twice.
        for (n, chunk) in frames[..50_000].chunks(480).enumerate() {
            ring.write(chunk);
            if n % 3 == 2 {
                let (at, lost) = ring.read_since(next, &mut got);
                assert_eq!(lost, 0);
                next = at;
            }
        }
        let (at, lost) = ring.read_since(next, &mut got);
        assert_eq!((at, lost), (50_000, 0));
        assert_eq!(got, frames[..50_000]);
        // A reader away for more than the ring holds gets the last frames
        // and is told what it lost.
        ring.write(&frames[50_000..]);
        got.clear();
        let (at, lost) = ring.read_since(next, &mut got);
        assert_eq!((at, lost), (150_000, 100_000 - RING_FRAMES as u64), "{RING_FRAMES} frames held of 100 000 missed");
        assert_eq!(got, frames[150_000 - RING_FRAMES..]);
    }

    #[test]
    fn a_full_scale_sine_reads_its_level_loudness_and_bin() {
        let rate = 48000.0;
        // BS.1770 puts a 997 Hz sine at 0 dBFS on one channel at −3.01 LUFS;
        // on both channels it is 3 dB louder.
        let mut meter = Meter::new(48000);
        meter.feed(&sine(997.0, 4.0, [1.0, 1.0], rate));
        let r = meter.read();
        assert!((r.peak_db[0]).abs() < 0.01 && (r.peak_db[1]).abs() < 0.01, "{:?}", r.peak_db);
        assert!((r.rms_db[0] + 3.01).abs() < 0.05, "{:?}", r.rms_db);
        assert!((r.momentary_lufs - 0.0).abs() < 0.1, "momentary {}", r.momentary_lufs);
        assert!((r.short_term_lufs - 0.0).abs() < 0.1, "short-term {}", r.short_term_lufs);
        let integrated = r.integrated_lufs.expect("four seconds are enough");
        assert!((integrated - 0.0).abs() < 0.1, "integrated {integrated}");
        assert!(r.range_lu.expect("a range") < 0.1, "a steady tone has no range: {:?}", r.range_lu);
        assert_eq!(r.history.len(), 40);
        assert!((r.correlation - 1.0).abs() < 1e-6, "{}", r.correlation);
        assert!(r.balance_db.abs() < 1e-6);
        // The spectrum peaks at the tone's bin, and its hold holds it.
        let bin = (997.0 / rate * TAP_FRAMES as f64).round() as usize;
        let loudest = r.spectrum.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        assert!((loudest as i64 - bin as i64).abs() <= 1, "bin {loudest} for {bin}");
        assert!(r.spectrum_hold[loudest] >= r.spectrum[loudest]);
        assert_eq!(r.scope.len(), SCOPE_FRAMES * 2);
        // A quieter tone at −20 dB reads 20 lower everywhere.
        let mut quiet = Meter::new(48000);
        quiet.feed(&sine(997.0, 4.0, [0.1, 0.1], rate));
        let q = quiet.read();
        assert!((q.momentary_lufs + 20.0).abs() < 0.1 && (q.peak_db[0] + 20.0).abs() < 0.05);
        assert!((q.integrated_lufs.unwrap() + 20.0).abs() < 0.1);
    }

    #[test]
    fn the_stereo_field_reads_mono_inverted_and_unrelated() {
        let rate = 48000.0;
        let mut inverted = Meter::new(48000);
        inverted.feed(&sine(200.0, 1.0, [0.5, -0.5], rate));
        let r = inverted.read();
        assert!((r.correlation + 1.0).abs() < 1e-6, "{}", r.correlation);
        let a = noise(48000, 1);
        let b = noise(48000, 2);
        let mut unrelated = Meter::new(48000);
        unrelated.feed(&a.iter().zip(&b).map(|(l, r)| [*l * 0.5, *r * 0.25]).collect::<Vec<_>>());
        let r = unrelated.read();
        assert!(r.correlation.abs() < 0.05, "{}", r.correlation);
        assert!((r.balance_db + 6.02).abs() < 0.3, "the right is 6 dB under: {}", r.balance_db);
        let mut silent = Meter::new(48000);
        silent.feed(&vec![[0.0; 2]; 48000]);
        let r = silent.read();
        assert_eq!((r.correlation, r.balance_db), (0.0, 0.0));
        assert!(r.integrated_lufs.is_none() && r.momentary_lufs <= -190.0);
    }

    #[test]
    fn the_true_peak_is_read_between_the_samples() {
        // A sine at a quarter of the rate, sampled at 45°: every sample is
        // at 0.707 of the peak, and the true peak is the peak.
        let rate = 48000.0;
        let frames: Vec<Frame> = (0..48000).map(|i| [(2.0 * PI * (i as f64 / 4.0) + PI / 4.0).sin() * 0.9; 2]).collect();
        let mut meter = Meter::new(48000);
        meter.feed(&frames);
        let r = meter.read();
        assert!((r.peak_db[0] - db(0.9 * 0.5f64.sqrt())).abs() < 0.01, "{:?}", r.peak_db);
        assert!((r.true_peak_db[0] - db(0.9)).abs() < 0.15, "true peak {:?}", r.true_peak_db);
        assert!(r.true_peak_hold_db[0] >= r.true_peak_db[0]);
        // A second reading holds the peaks but reads none since.
        let r2 = meter.read();
        assert!(r2.peak_db[0] <= -190.0 && (r2.true_peak_hold_db[0] - r.true_peak_hold_db[0]).abs() < 1e-6);
        meter.feed(&sine(1000.0, 0.5, [0.1, 0.1], rate));
        meter.reset();
        let r3 = meter.read();
        assert!(r3.peak_hold_db[0] <= -190.0 && r3.integrated_lufs.is_none() && r3.history.is_empty());
        assert_eq!(r3.spectrum_hold, r3.spectrum, "the hold starts again from this reading's spectrum");
    }

    #[test]
    fn the_spectrogram_and_the_waveform_come_as_columns_at_their_pace() {
        let rate = 48000.0;
        let mut meter = Meter::new(48000);
        meter.feed(&sine(997.0, 2.0, [0.8, 0.4], rate));
        let r = meter.read();
        // Fifty spectrogram columns and a hundred waveform columns a second;
        // a reading holds the last of each.
        assert_eq!((r.spectrogram_columns, r.waveform_columns), (100, 200));
        assert_eq!(r.spectrogram.len(), SPECTROGRAM_KEEP * SPECTROGRAM_ROWS);
        assert_eq!(r.waveform.len(), WAVEFORM_KEEP * 4);
        // The last column's loudest row is the tone's band.
        let last = &r.spectrogram[(SPECTROGRAM_KEEP - 1) * SPECTROGRAM_ROWS..];
        let loudest = last.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).unwrap().0;
        let rows = spectrogram_rows(rate);
        let bin = 997.0 / rate * TAP_FRAMES as f64;
        assert!(rows[loudest].0 <= bin && bin < rows[loudest].1, "row {loudest} spans bins {:?}, the tone is at {bin}", rows[loudest]);
        assert!((last[loudest] - db(0.6)).abs() < 1.0, "the mono level of 0.8 and 0.4: {}", last[loudest]);
        assert!(last[0] < -80.0 && last[SPECTROGRAM_ROWS - 1] < -80.0);
        // The rows span 20 Hz to 20 kHz in equal steps of pitch: a tenth of
        // the rows is a step of two octaves and a half, and no row is wider
        // than a bin below the point where they start to be.
        assert!((rows[0].0 - 20.0 / rate * TAP_FRAMES as f64).abs() < 1e-9);
        assert!((rows[SPECTROGRAM_ROWS - 1].1 - 20000.0 / rate * TAP_FRAMES as f64).abs() < 1e-9);
        assert!(rows[0].1 - rows[0].0 < 1.0 && rows[SPECTROGRAM_ROWS - 1].1 - rows[SPECTROGRAM_ROWS - 1].0 > 1.0);
        // The waveform's last column: each channel's lowest and highest
        // sample in its 10 ms, ten cycles of the tone.
        let w = &r.waveform[(WAVEFORM_KEEP - 1) * 4..];
        assert!((w[0] + 0.8).abs() < 0.01 && (w[1] - 0.8).abs() < 0.01, "{w:?}");
        assert!((w[2] + 0.4).abs() < 0.01 && (w[3] - 0.4).abs() < 0.01, "{w:?}");
        // Frames the watcher missed pass as blank columns, counted, and the
        // columns go on from there without a step.
        meter.skipped(48000);
        meter.feed(&sine(997.0, 0.015, [0.5, 0.5], rate));
        let r = meter.read();
        assert_eq!((r.spectrogram_columns, r.waveform_columns), (150, 301));
        let blank = &r.spectrogram[(SPECTROGRAM_KEEP - 2) * SPECTROGRAM_ROWS..(SPECTROGRAM_KEEP - 1) * SPECTROGRAM_ROWS];
        assert!(blank.iter().all(|l| *l == -200.0), "a blank column");
        assert_eq!(&r.waveform[(WAVEFORM_KEEP - 2) * 4..(WAVEFORM_KEEP - 1) * 4], &[0.0; 4], "a blank column");
        let w = &r.waveform[(WAVEFORM_KEEP - 1) * 4..];
        assert!((w[1] - 0.5).abs() < 0.01, "the column after the gap: {w:?}");
        // A much longer gap is counted, not made.
        meter.skipped(48000 * 60);
        let r = meter.read();
        assert_eq!((r.spectrogram_columns, r.waveform_columns), (150 + 3000, 301 + 6000));
        assert_eq!(r.spectrogram.len(), SPECTROGRAM_KEEP * SPECTROGRAM_ROWS);
        // Silence is a column of −200 and a waveform of nothing.
        let mut silent = Meter::new(44100);
        silent.feed(&vec![[0.0; 2]; 44100]);
        let r = silent.read();
        assert_eq!((r.spectrogram_columns, r.waveform_columns), (50, 100));
        assert!(r.spectrogram.iter().all(|l| *l <= -190.0) && r.waveform.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn k_weighting_at_48k_is_the_standard_s_table() {
        let (shelf, high) = k_weighting(48000.0);
        assert!((shelf.b[0] - 1.53512485958697).abs() < 1e-9);
        assert!((shelf.a[0] + 1.69065929318241).abs() < 1e-9);
        assert!((high.a[0] + 1.99004745483398).abs() < 1e-9);
    }
}

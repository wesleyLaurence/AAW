//! Convolution reverb with a seeded synthetic impulse response.
//!
//! The response follows `effects.reverb_ir`: Gaussian noise shaped in the
//! short-time Fourier domain so each frequency decays exponentially, with a
//! 3 ms raised-cosine onset, a width control, a 12 dB/octave low cut, energy
//! normalization per channel and a predelay. Its noise comes from this file's
//! own generator rather than numpy's, so a response is statistically
//! equivalent to the Python engine's and not sample-identical (D40).
//!
//! The noise generator: SplitMix64 expands the reverb's `seed` into the state
//! of xoshiro256++, whose outputs make uniform doubles from their top 53 bits.
//! Each pair of uniforms (u1 in (0, 1], u2 in [0, 1)) gives two normals by
//! Box–Muller, `sqrt(-2 ln u1) * (cos, sin)(2 pi u2)`. Channel 0 is drawn in
//! full, then channel 1.
//!
//! The convolution is non-uniformly partitioned: the first 64 taps run as a
//! direct filter, then blocks of 64, 256, 1024, 4096 and 16384 frames run as
//! overlap-save FFT convolutions, each covering the part of the response that
//! starts at least its own block length in. A block's result is then not
//! needed before the block is complete, so the reverb adds no latency. Blocks
//! of 256 frames and more cover the response from two block lengths in and
//! spread their work over the block that follows, in 64-frame slices counted
//! on the reverb's own input, so the load is even and the output does not
//! depend on the caller's blocks.

use crate::biquad::{butter, Cascade};
use crate::envelope::Knob;
use crate::{Clock, Frame};
use realfft::num_complex::Complex64;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::f64::consts::PI;
use std::sync::Arc;

/// `exp(-DECAY_60DB * t / rt60)` falls 60 dB at t = rt60.
const DECAY_60DB: f64 = 3.0 * std::f64::consts::LN_10;

/// What shapes an impulse response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shape {
    pub decay_seconds: f64,
    pub predelay_ms: f64,
    pub damping_hz: f64,
    pub lowcut_hz: f64,
    pub width_percent: f64,
    pub seed: u64,
    pub rate: u32,
}

impl Shape {
    /// The shape as bits, for use as a key: equal keys give equal responses.
    pub fn key(&self) -> [u64; 7] {
        [
            self.decay_seconds.to_bits(),
            self.predelay_ms.to_bits(),
            self.damping_hz.to_bits(),
            self.lowcut_hz.to_bits(),
            self.width_percent.to_bits(),
            self.seed,
            self.rate as u64,
        ]
    }
}

struct Noise {
    s: [u64; 4],
    spare: Option<f64>,
}

impl Noise {
    fn new(seed: u64) -> Noise {
        let mut x = seed;
        let mut next = || {
            x = x.wrapping_add(0x9E3779B97F4A7C15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
            z ^ (z >> 31)
        };
        Noise {
            s: [next(), next(), next(), next()],
            spare: None,
        }
    }

    /// xoshiro256++.
    fn bits(&mut self) -> u64 {
        let s = &mut self.s;
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    fn uniform(&mut self) -> f64 {
        (self.bits() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        if let Some(z) = self.spare.take() {
            return z;
        }
        let (u1, u2) = (1.0 - self.uniform(), self.uniform());
        let r = (-2.0 * u1.ln()).sqrt();
        self.spare = Some(r * (2.0 * PI * u2).sin());
        r * (2.0 * PI * u2).cos()
    }
}

/// Shapes noise so each frequency decays exponentially: rt60 is the decay time
/// up to the damping frequency and falls as 1/f above it. A short-time Fourier
/// transform with a periodic Hann window of 1024 frames every 256, zero-padded
/// at both ends, as `scipy.signal.stft` and `istft` make it.
fn shape_decay(noise: &[f64], shape: &Shape) -> Vec<f64> {
    const SEGMENT: usize = 1024;
    const HOP: usize = 256;
    let length = noise.len();
    let rate = shape.rate as f64;
    let mut padded = vec![0.0; SEGMENT / 2];
    padded.extend_from_slice(noise);
    padded.resize(padded.len() + SEGMENT / 2, 0.0);
    let extra = (HOP - (padded.len() - SEGMENT) % HOP) % HOP;
    padded.resize(padded.len() + extra, 0.0);
    let window: Vec<f64> = (0..SEGMENT).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / SEGMENT as f64).cos()).collect();
    let mut planner = RealFftPlanner::<f64>::new();
    let (forward, inverse) = (planner.plan_fft_forward(SEGMENT), planner.plan_fft_inverse(SEGMENT));
    let mut segment = vec![0.0; SEGMENT];
    let mut spectrum = forward.make_output_vec();
    let rates: Vec<f64> = (0..spectrum.len())
        .map(|k| {
            let freq = k as f64 * rate / SEGMENT as f64;
            let rt60 = shape.decay_seconds * (shape.damping_hz / freq.max(1.0)).min(1.0);
            -DECAY_60DB / rt60
        })
        .collect();
    let mut sum = vec![0.0; padded.len()];
    let mut norm = vec![0.0; padded.len()];
    for j in 0..(padded.len() - SEGMENT) / HOP + 1 {
        let at = j * HOP;
        for (i, s) in segment.iter_mut().enumerate() {
            *s = padded[at + i] * window[i];
        }
        forward.process(&mut segment, &mut spectrum).expect("segment length");
        let time = at as f64 / rate;
        for (bin, per_second) in spectrum.iter_mut().zip(&rates) {
            *bin *= (per_second * time).exp();
        }
        // The first and last bins of a real signal's spectrum are real.
        spectrum[0].im = 0.0;
        spectrum[SEGMENT / 2].im = 0.0;
        inverse.process(&mut spectrum, &mut segment).expect("segment length");
        for i in 0..SEGMENT {
            sum[at + i] += segment[i] / SEGMENT as f64 * window[i];
            norm[at + i] += window[i] * window[i];
        }
    }
    (0..length)
        .map(|i| {
            let at = SEGMENT / 2 + i;
            if norm[at] > 1e-10 { sum[at] / norm[at] } else { sum[at] }
        })
        .collect()
}

/// The stereo impulse response of a shape, energy-normalized per channel.
pub fn impulse_response(shape: &Shape) -> Vec<Frame> {
    let rate = shape.rate as f64;
    // The undamped tail reaches -90 dB.
    let length = (1.5 * shape.decay_seconds * rate).round_ties_even() as usize;
    let mut noise = Noise::new(shape.seed);
    let channels: Vec<Vec<f64>> = (0..2)
        .map(|_| {
            let raw: Vec<f64> = (0..length).map(|_| noise.normal()).collect();
            let mut shaped = shape_decay(&raw, shape);
            let onset = length.min((0.003 * rate).round_ties_even() as usize);
            for (i, x) in shaped[..onset].iter_mut().enumerate() {
                *x *= 0.5 - 0.5 * (PI * i as f64 / onset as f64).cos();
            }
            shaped
        })
        .collect();
    let width = shape.width_percent / 100.0;
    let mut ir: Vec<Frame> = (0..length)
        .map(|i| [channels[0][i] + width * channels[1][i], channels[0][i] - width * channels[1][i]])
        .collect();
    Cascade::new(butter(2, shape.lowcut_hz, true, rate)).process(&mut ir);
    for c in 0..2 {
        let energy = ir.iter().map(|f| f[c] * f[c]).sum::<f64>().sqrt();
        if energy > 0.0 {
            for f in ir.iter_mut() {
                f[c] /= energy;
            }
        }
    }
    let predelay = (shape.predelay_ms * rate / 1000.0).round_ties_even() as usize;
    let mut out = vec![[0.0; 2]; predelay];
    out.extend(ir);
    out
}

/// Taps run as a direct filter, and the frames in a slice of spread work.
const HEAD: usize = 64;
/// Block lengths of the partition levels.
const BLOCKS: [usize; 5] = [64, 256, 1024, 4096, 16384];

/// Where a level's part of the response starts.
fn level_start(level: usize) -> usize {
    if level == 0 { BLOCKS[0] } else { 2 * BLOCKS[level] }
}

struct LevelKernel {
    block: usize,
    parts: usize,
    /// Per channel, each partition's spectrum: `parts` rows of `block + 1` bins.
    spectra: [Vec<Complex64>; 2],
    forward: Arc<dyn RealToComplex<f64>>,
    inverse: Arc<dyn ComplexToReal<f64>>,
}

/// An impulse response prepared for convolution. Shared by every reverb with
/// the same shape and kept between compiles.
pub struct Kernel {
    shape: Shape,
    /// The first taps, reversed, per channel; empty when they are all zero.
    head: [Vec<f64>; 2],
    levels: Vec<LevelKernel>,
    frames: usize,
}

impl std::fmt::Debug for Kernel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Kernel({:?}, {} frames)", self.shape, self.frames)
    }
}

impl Kernel {
    pub fn new(shape: Shape) -> Kernel {
        Kernel::from_response(shape, &impulse_response(&shape))
    }

    /// A kernel for any response; `shape` names it.
    pub fn from_response(shape: Shape, ir: &[Frame]) -> Kernel {
        let tap = |i: usize, c: usize| ir.get(i).map_or(0.0, |f| f[c]);
        let head: [Vec<f64>; 2] = [0, 1].map(|c| {
            let taps: Vec<f64> = (0..HEAD).rev().map(|i| tap(i, c)).collect();
            if taps.iter().all(|x| *x == 0.0) { Vec::new() } else { taps }
        });
        let mut planner = RealFftPlanner::<f64>::new();
        let mut levels = Vec::new();
        for (level, &block) in BLOCKS.iter().enumerate() {
            let start = level_start(level);
            if ir.len() <= start {
                break;
            }
            let end = BLOCKS.get(level + 1).map_or(ir.len(), |_| level_start(level + 1).min(ir.len()));
            let parts = (end - start).div_ceil(block);
            let forward = planner.plan_fft_forward(2 * block);
            let inverse = planner.plan_fft_inverse(2 * block);
            let spectra = [0, 1].map(|c| {
                let mut all = Vec::with_capacity(parts * (block + 1));
                let mut time = vec![0.0; 2 * block];
                let mut spectrum = forward.make_output_vec();
                for k in 0..parts {
                    time.fill(0.0);
                    for (i, t) in time[..block].iter_mut().enumerate() {
                        let at = start + k * block + i;
                        *t = if at < end { tap(at, c) } else { 0.0 };
                    }
                    forward.process(&mut time, &mut spectrum).expect("block length");
                    all.extend_from_slice(&spectrum);
                }
                all
            });
            levels.push(LevelKernel {
                block,
                parts,
                spectra,
                forward,
                inverse,
            });
        }
        Kernel {
            shape,
            head,
            levels,
            frames: ir.len(),
        }
    }

    pub fn shape(&self) -> &Shape {
        &self.shape
    }

    /// The response's length in frames.
    pub fn frames(&self) -> usize {
        self.frames
    }
}

struct LevelState {
    /// Spectra of the last `parts` input windows, newest at `newest`.
    line: Vec<Complex64>,
    newest: usize,
    /// The block being played and the one being computed.
    current: Vec<Frame>,
    result: Vec<Frame>,
    sums: [Vec<Complex64>; 2],
    done: usize,
    time: Vec<f64>,
    forward_scratch: Vec<Complex64>,
    inverse_scratch: Vec<Complex64>,
}

/// A convolution in progress: the input's history and each level's work.
pub struct Convolver {
    kernel: Arc<Kernel>,
    /// The mono input, in a ring as long as the largest transform.
    input: Vec<f64>,
    /// The last `HEAD` inputs, twice, so the direct filter reads a straight run.
    recent: Vec<f64>,
    count: u64,
    levels: Vec<LevelState>,
}

impl Convolver {
    pub fn new(kernel: Arc<Kernel>) -> Convolver {
        Convolver::staggered(kernel, 0)
    }

    /// A convolver whose blocks end `slices` slices of work away from those of
    /// one made with `new`. The convolution is the same; several reverbs given
    /// different numbers do their transforms at different times, which keeps
    /// the load of any one callback down.
    pub fn staggered(kernel: Arc<Kernel>, slices: usize) -> Convolver {
        let levels: Vec<LevelState> = kernel
            .levels
            .iter()
            .map(|k| LevelState {
                line: vec![Complex64::new(0.0, 0.0); k.parts * (k.block + 1)],
                newest: 0,
                current: vec![[0.0; 2]; k.block],
                result: vec![[0.0; 2]; k.block],
                sums: [0, 1].map(|_| vec![Complex64::new(0.0, 0.0); k.block + 1]),
                done: 0,
                time: vec![0.0; 2 * k.block],
                forward_scratch: k.forward.make_scratch_vec(),
                inverse_scratch: k.inverse.make_scratch_vec(),
            })
            .collect();
        let ring = kernel.levels.last().map_or(2 * HEAD, |k| 2 * k.block);
        Convolver {
            input: vec![0.0; ring],
            recent: vec![0.0; 2 * HEAD],
            count: (slices * HEAD) as u64,
            levels,
            kernel,
        }
    }

    pub fn kernel(&self) -> &Arc<Kernel> {
        &self.kernel
    }

    /// The wet frame for one more input frame.
    #[inline]
    pub fn tick(&mut self, x: f64) -> Frame {
        let ring = self.input.len();
        let at = (self.count % ring as u64) as usize;
        self.input[at] = x;
        let p = (self.count % HEAD as u64) as usize;
        self.recent[p] = x;
        self.recent[p + HEAD] = x;
        let mut wet = [0.0; 2];
        for c in 0..2 {
            let head = &self.kernel.head[c];
            if !head.is_empty() {
                let run = &self.recent[p + 1..p + 1 + HEAD];
                wet[c] = head.iter().zip(run).map(|(h, x)| h * x).sum();
            }
        }
        for (k, s) in self.kernel.levels.iter().zip(&self.levels) {
            let f = s.current[(self.count % k.block as u64) as usize];
            wet[0] += f[0];
            wet[1] += f[1];
        }
        self.count += 1;
        if self.count % HEAD as u64 == 0 {
            self.boundary();
        }
        wet
    }

    /// The work due each `HEAD` frames of input.
    fn boundary(&mut self) {
        let kernel = self.kernel.clone();
        for (level, k) in kernel.levels.iter().enumerate() {
            let phase = (self.count % k.block as u64) as usize / HEAD;
            let slices = k.block / HEAD;
            if slices == 1 {
                // The first level's block is needed at once.
                self.transform(level, k);
                self.accumulate(level, k, k.parts);
                self.invert(level, k, 0);
                self.invert(level, k, 1);
                let s = &mut self.levels[level];
                std::mem::swap(&mut s.current, &mut s.result);
            } else if phase == 0 {
                let s = &mut self.levels[level];
                std::mem::swap(&mut s.current, &mut s.result);
                self.transform(level, k);
            } else if phase == slices - 2 {
                self.invert(level, k, 0);
            } else if phase == slices - 1 {
                self.invert(level, k, 1);
            } else {
                self.accumulate(level, k, (k.parts * phase).div_ceil(slices - 3));
            }
        }
    }

    /// Takes the spectrum of the last two blocks of input into the level's line.
    fn transform(&mut self, level: usize, k: &LevelKernel) {
        let ring = self.input.len();
        let s = &mut self.levels[level];
        let n = 2 * k.block;
        let start = ((self.count + ring as u64 - n as u64 % ring as u64) % ring as u64) as usize;
        for (i, t) in s.time.iter_mut().enumerate() {
            *t = self.input[(start + i) % ring];
        }
        s.newest = (s.newest + 1) % k.parts;
        let bins = k.block + 1;
        let slot = &mut s.line[s.newest * bins..(s.newest + 1) * bins];
        let _ = k.forward.process_with_scratch(&mut s.time, slot, &mut s.forward_scratch);
        for sum in &mut s.sums {
            sum.fill(Complex64::new(0.0, 0.0));
        }
        s.done = 0;
    }

    /// Adds partitions' products to the sums until `target` are done.
    fn accumulate(&mut self, level: usize, k: &LevelKernel, target: usize) {
        let s = &mut self.levels[level];
        let bins = k.block + 1;
        while s.done < target.min(k.parts) {
            let part = s.done;
            let window = (s.newest + k.parts - part) % k.parts;
            let x = &s.line[window * bins..(window + 1) * bins];
            for c in 0..2 {
                let h = &k.spectra[c][part * bins..(part + 1) * bins];
                for ((sum, x), h) in s.sums[c].iter_mut().zip(x).zip(h) {
                    *sum += x * h;
                }
            }
            s.done += 1;
        }
    }

    /// Turns a channel's sum into its half of the next block.
    fn invert(&mut self, level: usize, k: &LevelKernel, channel: usize) {
        self.accumulate(level, k, k.parts);
        let s = &mut self.levels[level];
        let sum = &mut s.sums[channel];
        sum[0].im = 0.0;
        sum[k.block].im = 0.0;
        let _ = k.inverse.process_with_scratch(sum, &mut s.time, &mut s.inverse_scratch);
        let scale = 1.0 / (2 * k.block) as f64;
        for (out, t) in s.result.iter_mut().zip(&s.time[k.block..]) {
            out[channel] = t * scale;
        }
    }
}

/// Mono-in, stereo-out convolution with a kernel, mixed with the input.
pub struct Reverb {
    convolver: Convolver,
    pub mix: Knob,
    mixes: Vec<f64>,
}

impl Reverb {
    /// `stagger` sets this reverb's work apart from that of others; see
    /// `Convolver::staggered`.
    pub fn new(kernel: Arc<Kernel>, mix: Knob, max_block: usize, stagger: usize) -> Reverb {
        Reverb {
            convolver: Convolver::staggered(kernel, stagger),
            mix,
            mixes: vec![0.0; max_block],
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        self.mix.fill(clock.frame, clock.step, &mut self.mixes[..n]);
        for (frame, mix) in x.iter_mut().zip(&self.mixes[..n]) {
            let mix = mix / 100.0;
            let wet = self.convolver.tick((frame[0] + frame[1]) / 2.0);
            *frame = [frame[0] * (1.0 - mix) + wet[0] * mix, frame[1] * (1.0 - mix) + wet[1] * mix];
        }
    }

    /// Continues `old`'s tail, when it has the same response.
    pub fn take_over(&mut self, old: &mut Reverb) {
        if old.convolver.kernel.shape.key() != self.convolver.kernel.shape.key() {
            return;
        }
        std::mem::swap(&mut self.convolver, &mut old.convolver);
        self.mix.take_over(&old.mix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Param;

    fn shape() -> Shape {
        Shape {
            decay_seconds: 1.5,
            predelay_ms: 10.0,
            damping_hz: 6000.0,
            lowcut_hz: 100.0,
            width_percent: 100.0,
            seed: 0,
            rate: 48000,
        }
    }

    /// Seconds to fall 60 dB, from the slope of the backward-integrated energy
    /// between -5 and -35 dB, in the band between two frequencies.
    fn decay_time(ir: &[Frame], low: f64, high: f64) -> f64 {
        let mut band: Vec<Frame> = ir.to_vec();
        Cascade::new(butter(4, low, true, 48000.0)).process(&mut band);
        Cascade::new(butter(4, high, false, 48000.0)).process(&mut band);
        let mut energy: Vec<f64> = band.iter().map(|f| f[0] * f[0]).collect();
        for i in (0..energy.len() - 1).rev() {
            energy[i] += energy[i + 1];
        }
        let db = |i: usize| 10.0 * (energy[i] / energy[0]).log10();
        let first = (0..energy.len()).find(|i| db(*i) <= -5.0).unwrap();
        let last = (0..energy.len()).find(|i| db(*i) <= -35.0).unwrap();
        (last - first) as f64 / 48000.0 * 2.0
    }

    #[test]
    fn the_response_has_its_decay_damping_predelay_and_energy() {
        let s = Shape {
            decay_seconds: 2.0,
            predelay_ms: 20.0,
            damping_hz: 3000.0,
            ..shape()
        };
        let ir = impulse_response(&s);
        assert_eq!(ir.len(), 960 + 144000);
        let mid = decay_time(&ir, 700.0, 1400.0);
        assert!((mid / 2.0 - 1.0).abs() < 0.1, "{mid}");
        // Above the damping frequency the decay time falls as 1/f.
        assert!(decay_time(&ir, 8000.0, 12000.0) < 0.5 * mid);
        assert!(ir[..960].iter().all(|f| *f == [0.0; 2]));
        assert!(ir[960..1500].iter().any(|f| f[0].abs() > 1e-3));
        for c in 0..2 {
            let energy: f64 = ir.iter().map(|f| f[c] * f[c]).sum();
            assert!((energy - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn the_response_is_seeded_and_width_decorrelates() {
        let a = impulse_response(&shape());
        assert_eq!(a, impulse_response(&shape()));
        assert_ne!(a, impulse_response(&Shape { seed: 1, ..shape() }));
        let dot = |x: &[Frame]| x.iter().map(|f| f[0] * f[1]).sum::<f64>();
        assert!(dot(&a).abs() < 0.1, "{}", dot(&a));
        let mono = impulse_response(&Shape {
            width_percent: 0.0,
            ..shape()
        });
        assert!(mono.iter().all(|f| f[0] == f[1]));
    }

    #[test]
    fn the_low_cut_removes_low_end() {
        let low = |hz: f64| {
            let ir = impulse_response(&Shape { lowcut_hz: hz, ..shape() });
            let mut band = ir.clone();
            Cascade::new(butter(4, 80.0, false, 48000.0)).process(&mut band);
            band.iter().map(|f| f[0] * f[0]).sum::<f64>()
        };
        assert!(low(400.0) < low(20.0) / 30.0);
    }

    /// A response that exercises every level, and its exact convolution with a
    /// few impulses.
    fn random_response(frames: usize) -> Vec<Frame> {
        let mut noise = Noise::new(3);
        (0..frames).map(|i| [noise.normal() * (-(i as f64) / 30000.0).exp(), noise.normal()]).collect()
    }

    #[test]
    fn the_convolution_is_exact_and_adds_no_latency() {
        let ir = random_response(70000);
        let kernel = Arc::new(Kernel::from_response(shape(), &ir));
        assert_eq!(kernel.levels.len(), 5);
        let impulses = [(0usize, 1.0), (1, -0.5), (63, 0.25), (64, 2.0), (5000, -1.5), (40000, 0.75), (41000, 1.0)];
        let n = 120000;
        let mut input = vec![0.0; n];
        for (at, gain) in impulses {
            input[at] = gain;
        }
        // Wherever its blocks fall on the input.
        for stagger in [0, 37, 255] {
            let mut convolver = Convolver::staggered(kernel.clone(), stagger);
            let wet: Vec<Frame> = input.iter().map(|x| convolver.tick(*x)).collect();
            let mut worst = 0.0f64;
            for (i, w) in wet.iter().enumerate() {
                for c in 0..2 {
                    let expected: f64 = impulses.iter().filter(|(at, _)| *at <= i && i - at < ir.len()).map(|(at, gain)| gain * ir[i - at][c]).sum();
                    worst = worst.max((w[c] - expected).abs());
                }
            }
            assert!(worst < 1e-12, "stagger {stagger}: {worst}");
        }
    }

    #[test]
    fn the_reverb_does_not_depend_on_blocks_and_a_dry_mix_is_the_input() {
        let kernel = Arc::new(Kernel::new(Shape {
            decay_seconds: 0.4,
            predelay_ms: 0.0,
            ..shape()
        }));
        let mut noise = Noise::new(8);
        let x: Vec<Frame> = (0..30000).map(|_| [noise.normal() * 0.1, noise.normal() * 0.1]).collect();
        let clock = Clock {
            frame: 0,
            step: 1,
            total: 1 << 40,
        };
        let reverb = |mix: f64| Reverb::new(kernel.clone(), Knob::new(Param::fixed(mix), 240), 4096, 0);
        let mut whole = x.clone();
        let mut r = reverb(40.0);
        for chunk in whole.chunks_mut(4096) {
            r.process(chunk, clock);
        }
        let mut pieces = x.clone();
        let mut r = reverb(40.0);
        let (mut at, mut size) = (0, 1);
        while at < pieces.len() {
            let n = size.min(pieces.len() - at);
            r.process(&mut pieces[at..at + n], clock);
            at += n;
            size = size * 3 % 1021 + 1;
        }
        assert_eq!(whole, pieces);
        assert!(whole.iter().zip(&x).any(|(a, b)| a != b));
        let mut dry = x.clone();
        reverb(0.0).process(&mut dry[..4096], clock);
        assert_eq!(&dry[..4096], &x[..4096]);
    }

    #[test]
    fn steady_noise_keeps_its_level() {
        let kernel = Arc::new(Kernel::new(Shape {
            decay_seconds: 1.0,
            ..shape()
        }));
        let mut noise = Noise::new(5);
        let mut convolver = Convolver::new(kernel);
        let (mut dry, mut wet) = (0.0, 0.0);
        for i in 0..4 * 48000 {
            let x = noise.normal() * 0.1;
            let w = convolver.tick(x);
            if i >= 2 * 48000 {
                dry += x * x;
                wet += w[0] * w[0];
            }
        }
        let db = 10.0 * (wet / dry).log10();
        assert!(db.abs() < 0.5, "{db} dB");
    }
}

//! What an equalizer's panel draws its curve over: the signal leaving the
//! device, tapped on the audio thread into a ring the app reads from, and
//! the spectrum worked out from that ring off the audio thread.
//!
//! The tap is a ring of the last `TAP_FRAMES` frames, summed to mono, each
//! an atomic store, with a count of frames written. The audio thread never
//! waits and never allocates; a reader that catches the ring mid-block sees
//! a window a few frames torn, which a picture does not show.

use crate::meter::Ring;
use crate::Frame;
use realfft::num_complex::Complex64;
use realfft::{RealFftPlanner, RealToComplex};
use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard};

/// Frames the tap keeps, and the length of the analysis: 85 ms at 48 kHz,
/// bins 11.7 Hz apart.
pub const TAP_FRAMES: usize = 4096;

/// Bins the analysis gives, from 0 Hz to half the sample rate.
pub const BINS: usize = TAP_FRAMES / 2 + 1;

/// The last `TAP_FRAMES` frames a device put out, as mono.
#[derive(Debug)]
pub struct Tap {
    samples: Box<[AtomicU32]>,
    written: AtomicU64,
}

impl Default for Tap {
    fn default() -> Tap {
        Tap {
            samples: (0..TAP_FRAMES).map(|_| AtomicU32::new(0)).collect(),
            written: AtomicU64::new(0),
        }
    }
}

impl Tap {
    /// Writes a block, on the audio thread.
    #[inline]
    pub fn write(&self, x: &[Frame]) {
        let start = self.written.load(Relaxed) as usize;
        for (i, f) in x.iter().enumerate() {
            let mono = ((f[0] + f[1]) * 0.5) as f32;
            self.samples[(start + i) & (TAP_FRAMES - 1)].store(mono.to_bits(), Relaxed);
        }
        self.written.fetch_add(x.len() as u64, Relaxed);
    }

    /// Frames written so far.
    pub fn written(&self) -> u64 {
        self.written.load(Relaxed)
    }

    /// The last `TAP_FRAMES` frames, oldest first; zeros before the first
    /// frames were written.
    pub fn read(&self, out: &mut [f64; TAP_FRAMES]) {
        let end = self.written.load(Relaxed) as usize;
        for (i, o) in out.iter_mut().enumerate() {
            let at = end.wrapping_sub(TAP_FRAMES).wrapping_add(i);
            *o = f32::from_bits(self.samples[at & (TAP_FRAMES - 1)].load(Relaxed)) as f64;
        }
    }
}

/// The taps of a song's equalizers and the rings of its analyzers, by the
/// row that owns the chain (a track's, group's or return's ID, or `master`)
/// and the effect's index in it, kept between compiles so that the app
/// reads the same ring while a song is edited. Entries a compile does not
/// ask for are dropped at `finish`.
#[derive(Debug, Default)]
pub struct Taps {
    inner: Mutex<Inner>,
}

#[derive(Debug, Default)]
struct Inner {
    generation: u64,
    taps: HashMap<(String, usize), (u64, Arc<Tap>)>,
    rings: HashMap<(String, usize), (u64, Arc<Ring>)>,
}

impl Taps {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Starts a compile.
    pub fn begin(&self) {
        self.lock().generation += 1;
    }

    /// The tap of an equalizer, made if the song had none there.
    pub fn get(&self, owner: &str, index: usize) -> Arc<Tap> {
        let mut inner = self.lock();
        let generation = inner.generation;
        let entry = inner.taps.entry((owner.to_string(), index)).or_insert_with(|| (generation, Arc::default()));
        entry.0 = generation;
        entry.1.clone()
    }

    /// The ring of an analyzer, made if the song had none there.
    pub fn ring(&self, owner: &str, index: usize) -> Arc<Ring> {
        let mut inner = self.lock();
        let generation = inner.generation;
        let entry = inner.rings.entry((owner.to_string(), index)).or_insert_with(|| (generation, Arc::default()));
        entry.0 = generation;
        entry.1.clone()
    }

    /// Drops the taps and rings the compile did not ask for.
    pub fn finish(&self) {
        let mut inner = self.lock();
        let generation = inner.generation;
        inner.taps.retain(|_, e| e.0 == generation);
        inner.rings.retain(|_, e| e.0 == generation);
    }

    /// The tap of an equalizer, if the last compile gave it one.
    pub fn find(&self, owner: &str, index: usize) -> Option<Arc<Tap>> {
        self.lock().taps.get(&(owner.to_string(), index)).map(|e| e.1.clone())
    }

    /// The ring of an analyzer, if the last compile gave it one.
    pub fn find_ring(&self, owner: &str, index: usize) -> Option<Arc<Ring>> {
        self.lock().rings.get(&(owner.to_string(), index)).map(|e| e.1.clone())
    }
}

/// Works out the spectrum of a tap, off the audio thread: a Hann window and
/// a real FFT, with the plan and its scratch kept between calls.
pub struct Analyzer {
    fft: Arc<dyn RealToComplex<f64>>,
    window: Vec<f64>,
    input: Vec<f64>,
    output: Vec<Complex64>,
    scratch: Vec<Complex64>,
    frames: [f64; TAP_FRAMES],
}

impl Default for Analyzer {
    fn default() -> Analyzer {
        let fft = RealFftPlanner::<f64>::new().plan_fft_forward(TAP_FRAMES);
        Analyzer {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            window: (0..TAP_FRAMES).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / TAP_FRAMES as f64).cos()).collect(),
            fft,
            frames: [0.0; TAP_FRAMES],
        }
    }
}

impl Analyzer {
    /// The level of each bin of the tap's last window, in dB relative to
    /// full scale: a sine at full scale reads 0 dB at its bin. Silence
    /// reads −200 dB.
    pub fn analyze(&mut self, tap: &Tap) -> Vec<f32> {
        tap.read(&mut self.frames);
        self.levels()
    }

    /// `analyze` of frames given directly.
    pub fn analyze_frames(&mut self, frames: &[f64; TAP_FRAMES]) -> Vec<f32> {
        self.frames = *frames;
        self.levels()
    }

    fn levels(&mut self) -> Vec<f32> {
        for ((i, w), x) in self.input.iter_mut().zip(&self.window).zip(&self.frames) {
            *i = w * x;
        }
        let _ = self.fft.process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch);
        // A Hann window sums to half its length; a sine's energy splits
        // between its positive and negative frequencies.
        let scale = 4.0 / TAP_FRAMES as f64;
        self.output
            .iter()
            .map(|c| {
                let magnitude = c.norm() * scale;
                (20.0 * magnitude.max(1e-10).log10()) as f32
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tap_keeps_the_last_frames_in_order() {
        let tap = Tap::default();
        let mut expected = [0.0; TAP_FRAMES];
        tap.read(&mut expected);
        assert!(expected.iter().all(|x| *x == 0.0), "silent before anything is written");
        // 6000 frames in blocks of 128 and a remainder: the ring holds the last 4096.
        let frames: Vec<Frame> = (0..6000).map(|i| [i as f64, i as f64 + 2.0]).collect();
        for chunk in frames.chunks(128) {
            tap.write(chunk);
        }
        assert_eq!(tap.written(), 6000);
        let mut got = [0.0; TAP_FRAMES];
        tap.read(&mut got);
        for (i, x) in got.iter().enumerate() {
            assert_eq!(*x, (6000 - TAP_FRAMES + i) as f64 + 1.0, "frame {i}");
        }
    }

    #[test]
    fn a_full_scale_sine_reads_zero_db_at_its_bin() {
        let mut analyzer = Analyzer::default();
        // Bin 100: 100 cycles across the window, so the window's edges meet.
        let frames: Vec<Frame> = (0..TAP_FRAMES).map(|i| [(2.0 * PI * 100.0 * i as f64 / TAP_FRAMES as f64).sin(); 2]).collect();
        let tap = Tap::default();
        tap.write(&frames);
        let levels = analyzer.analyze(&tap);
        assert_eq!(levels.len(), BINS);
        assert!((levels[100]).abs() < 0.01, "{} dB at the tone's bin", levels[100]);
        assert!(levels[50] < -100.0 && levels[2000] < -100.0, "nothing elsewhere: {} {}", levels[50], levels[2000]);
        let silent = analyzer.analyze(&Tap::default());
        assert!(silent.iter().all(|l| *l <= -199.0));
    }
}

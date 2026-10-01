//! Waveform peaks: what a track's voices sum to, reduced to the least and the
//! greatest sample of each stretch of frames, for a display to draw.
//!
//! The peaks are of the sampler's output, before the track's inserts, fader
//! and sends, on the timeline without the delays that align latency: what the
//! track's clips play. They depend only on the track's voices and the session
//! length, so they stay as they are while levels and effects change.

use crate::program::Voice;
use crate::render::{Frame, Voices};

/// Frames in a bucket of the finest level.
pub const BASE: usize = 64;
/// How many buckets of one level make a bucket of the next.
pub const STEP: usize = 4;
/// A level has a coarser one after it while it has more buckets than this.
pub const COARSEST: usize = 2048;

const BLOCK: usize = 64 * BASE;

/// One resolution of a track's peaks.
#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub frames_per_bucket: usize,
    /// For each bucket its least and its greatest sample over both channels,
    /// with full scale at 127; louder is drawn as full scale.
    pub data: Vec<i8>,
}

impl Level {
    pub fn buckets(&self) -> usize {
        self.data.len() / 2
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Peaks {
    /// The session length the peaks cover.
    pub frames: usize,
    /// From the finest, `BASE` frames a bucket, to the coarsest.
    pub levels: Vec<Level>,
}

/// A least sample rounds down and a greatest up, so that a bucket never shows
/// less than it holds.
fn quantize(lo: f64, hi: f64) -> [i8; 2] {
    let scale = |x: f64| x.clamp(-1.0, 1.0) * 127.0;
    [scale(lo).floor() as i8, scale(hi).ceil() as i8]
}

/// The next level: each bucket covers `STEP` of `level`'s.
fn coarser(level: &Level) -> Level {
    let data = level
        .data
        .chunks(2 * STEP)
        .flat_map(|chunk| {
            let lo = chunk.iter().step_by(2).copied().min().unwrap_or(0);
            let hi = chunk.iter().skip(1).step_by(2).copied().max().unwrap_or(0);
            [lo, hi]
        })
        .collect();
    Level {
        frames_per_bucket: level.frames_per_bucket * STEP,
        data,
    }
}

/// The peaks of a track's voices over a session of `total` frames. Nothing
/// sounds past the session end, as in a render.
pub fn peaks(voices: &[Voice], total: usize) -> Peaks {
    let mut data = vec![0i8; 2 * total.div_ceil(BASE)];
    let mut state = Voices::new(voices, BLOCK);
    state.seek(voices, 0, false);
    let mut block: Vec<Frame> = vec![[0.0; 2]; BLOCK];
    let mut start = 0;
    while start < total {
        let n = BLOCK.min(total - start);
        // Stretches of silence, which most of a track can be, cost nothing.
        if !state.idle(voices, (start + n) as i64) {
            let out = &mut block[..n];
            out.fill([0.0; 2]);
            state.render(voices, out, start as i64, total as i64, 0);
            for (bucket, frames) in out.chunks(BASE).enumerate() {
                let (lo, hi) = frames.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), f| {
                    (lo.min(f[0]).min(f[1]), hi.max(f[0]).max(f[1]))
                });
                let at = 2 * (start / BASE + bucket);
                data[at..at + 2].copy_from_slice(&quantize(lo, hi));
            }
        }
        start += n;
    }
    let mut levels = vec![Level {
        frames_per_bucket: BASE,
        data,
    }];
    while levels.last().is_some_and(|l| l.buckets() > COARSEST) {
        levels.push(coarser(levels.last().expect("a level")));
    }
    Peaks { frames: total, levels }
}

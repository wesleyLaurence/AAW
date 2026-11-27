//! What a compressor's, a limiter's or a clipper's panel shows while the
//! song plays: the level reaching it, the level leaving it and the gain it
//! takes off, a hop each `HOP_FRAMES` frames, in a ring the app reads.
//!
//! The device folds each frame into the hop it is making and, at the end of
//! the hop, stores it with four atomic stores and counts it. The audio
//! thread never waits and never allocates; a reader that catches the ring
//! mid-store sees one hop torn, which a picture does not show. Only frames
//! played while the transport rolls make hops, so that the picture holds
//! what was last played while the song is stopped.

use crate::{Clock, Frame};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

/// Frames a hop covers: 5.3 ms at 48 kHz.
pub const HOP_FRAMES: usize = 256;

/// Hops the ring keeps: 5.5 s at 48 kHz.
pub const GAUGE_HOPS: usize = 1024;

/// What the level reads where nothing played.
pub const SILENCE_DB: f32 = -200.0;

/// One hop of a device's work. Levels are the louder channel's highest
/// sample in the hop, in dB relative to full scale; silence reads −200.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hop {
    /// What the device's detector heard: a compressor's key when it has one.
    pub input_db: f32,
    pub output_db: f32,
    /// The most gain the device took off in the hop, in dB, 0 or more.
    pub reduction_db: f32,
    /// The level it works against: a compressor's threshold, a limiter's or
    /// a clipper's ceiling, as automation had it at the hop's end.
    pub threshold_db: f32,
}

impl Default for Hop {
    fn default() -> Hop {
        Hop {
            input_db: SILENCE_DB,
            output_db: SILENCE_DB,
            reduction_db: 0.0,
            threshold_db: 0.0,
        }
    }
}

/// The last `GAUGE_HOPS` hops of a device, and how many it has made.
#[derive(Debug)]
pub struct Gauge {
    values: [Box<[AtomicU32]>; 4],
    written: AtomicU64,
}

impl Default for Gauge {
    fn default() -> Gauge {
        let hop = Hop::default();
        let filled = |v: f32| (0..GAUGE_HOPS).map(|_| AtomicU32::new(v.to_bits())).collect();
        Gauge {
            values: [filled(hop.input_db), filled(hop.output_db), filled(hop.reduction_db), filled(hop.threshold_db)],
            written: AtomicU64::new(0),
        }
    }
}

impl Gauge {
    /// Stores a hop, on the audio thread.
    #[inline]
    fn push(&self, hop: Hop) {
        let at = self.written.load(Relaxed) as usize & (GAUGE_HOPS - 1);
        for (values, v) in self.values.iter().zip([hop.input_db, hop.output_db, hop.reduction_db, hop.threshold_db]) {
            values[at].store(v.to_bits(), Relaxed);
        }
        self.written.fetch_add(1, Relaxed);
    }

    /// Hops made so far.
    pub fn written(&self) -> u64 {
        self.written.load(Relaxed)
    }

    /// The last `count` hops, at most `GAUGE_HOPS`, oldest first; silence
    /// before the first was made.
    pub fn read(&self, count: usize) -> Vec<Hop> {
        let count = count.min(GAUGE_HOPS);
        let end = self.written.load(Relaxed) as usize;
        (0..count)
            .map(|i| {
                let at = end.wrapping_sub(count).wrapping_add(i) & (GAUGE_HOPS - 1);
                let [a, b, c, d] = &self.values;
                let get = |v: &[AtomicU32]| f32::from_bits(v[at].load(Relaxed));
                Hop {
                    input_db: get(a),
                    output_db: get(b),
                    reduction_db: get(c),
                    threshold_db: get(d),
                }
            })
            .collect()
    }
}

/// The hop a device is making, and the gauge it goes to; with no gauge,
/// nothing is kept.
#[derive(Clone, Debug, Default)]
pub struct Hopper {
    gauge: Option<std::sync::Arc<Gauge>>,
    frames: usize,
    input: f64,
    output: f64,
    reduction: f64,
}

/// The louder channel's sample, as an amplitude.
#[inline]
pub fn peak(f: Frame) -> f64 {
    f[0].abs().max(f[1].abs())
}

fn db(amplitude: f64) -> f32 {
    if amplitude > 0.0 { (20.0 * amplitude.log10()).max(SILENCE_DB as f64) as f32 } else { SILENCE_DB }
}

impl Hopper {
    pub fn new(gauge: Option<std::sync::Arc<Gauge>>) -> Hopper {
        Hopper {
            gauge,
            ..Hopper::default()
        }
    }

    /// Whether the block's frames make hops: a gauge is watching and the
    /// transport rolls.
    #[inline]
    pub fn on(&self, clock: &Clock) -> bool {
        self.gauge.is_some() && clock.step == 1
    }

    /// Folds in a frame: the amplitudes of what the detector heard and of
    /// what left, the gain taken off and the threshold in dB. Call only
    /// when `on`.
    #[inline]
    pub fn add(&mut self, input: f64, output: f64, reduction_db: f64, threshold_db: f64) {
        self.input = self.input.max(input);
        self.output = self.output.max(output);
        self.reduction = self.reduction.max(reduction_db);
        self.frames += 1;
        if self.frames == HOP_FRAMES {
            if let Some(gauge) = &self.gauge {
                gauge.push(Hop {
                    input_db: db(self.input),
                    output_db: db(self.output),
                    reduction_db: self.reduction.max(0.0) as f32,
                    threshold_db: threshold_db as f32,
                });
            }
            (self.frames, self.input, self.output, self.reduction) = (0, 0.0, 0.0, 0.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn hops_keep_the_most_of_their_frames_in_order() {
        let gauge = Arc::new(Gauge::default());
        assert_eq!(gauge.read(2), vec![Hop::default(); 2], "silent before anything plays");
        let mut hopper = Hopper::new(Some(gauge.clone()));
        let rolling = Clock { frame: 0, step: 1, total: 1 << 40 };
        assert!(hopper.on(&rolling));
        assert!(!hopper.on(&Clock { step: 0, ..rolling }), "nothing is made while stopped");
        assert!(!Hopper::new(None).on(&rolling));
        // Three hops and a part: the part is not stored.
        for i in 0..(3 * HOP_FRAMES + 10) {
            let hop = i / HOP_FRAMES;
            let top = i % HOP_FRAMES == 7;
            let input = if top { 0.5 } else { 0.1 };
            hopper.add(input, input * 0.5, if top { hop as f64 + 1.0 } else { 0.0 }, -12.0 - hop as f64);
        }
        assert_eq!(gauge.written(), 3);
        let hops = gauge.read(4);
        assert_eq!(hops[0], Hop::default(), "one hop more than were made reads silence");
        for (i, hop) in hops[1..].iter().enumerate() {
            assert!((hop.input_db - -6.0206).abs() < 1e-3, "{hop:?}");
            assert!((hop.output_db - -12.0412).abs() < 1e-3, "{hop:?}");
            assert_eq!(hop.reduction_db, i as f32 + 1.0);
            assert_eq!(hop.threshold_db, -12.0 - i as f32);
        }
        // The ring wraps and keeps the last.
        for _ in 0..(GAUGE_HOPS * HOP_FRAMES) {
            hopper.add(0.0, 0.0, 2.0, -3.0);
        }
        assert_eq!(gauge.written(), 3 + GAUGE_HOPS as u64);
        let last = gauge.read(GAUGE_HOPS + 5);
        assert_eq!(last.len(), GAUGE_HOPS);
        // The part left over, with a top of its own, went on into the next hop.
        assert_eq!(last[0].reduction_db, 4.0);
        assert!((last[0].input_db - -6.0206).abs() < 1e-3);
        assert!(last[1..].iter().all(|h| h.reduction_db == 2.0 && h.input_db == SILENCE_DB && h.threshold_db == -3.0));
    }
}

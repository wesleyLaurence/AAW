//! A stereo chorus: each channel through a delay of `delay_ms` moved
//! `depth_ms` either side by a sine at `rate_hz`, the right channel a quarter
//! cycle behind the left, mixed under the dry signal.
//!
//! The sweep is a function of the frames processed, so a render is the same
//! bytes twice and does not depend on blocks, and the sweep goes on while
//! the transport stands, as a tail does.

use crate::envelope::Knob;
use crate::{Clock, Frame};
use std::f64::consts::TAU;

/// The longest delay the line holds, with the deepest sweep, in milliseconds.
const LONGEST_MS: f64 = 40.0 + 20.0;

/// The line `back` frames before `position`, interpolated.
#[inline]
fn read(line: &[Frame], position: usize, back: f64, channel: usize) -> f64 {
    let n = line.len();
    let back = back.clamp(1.0, (n - 2) as f64);
    let i = back as usize;
    let frac = back - i as f64;
    let a = line[(position + n - i) % n][channel];
    let b = line[(position + n - i - 1) % n][channel];
    a + (b - a) * frac
}

#[derive(Clone, Debug)]
pub struct Chorus {
    rate: f64,
    line: Vec<Frame>,
    position: usize,
    /// The sweep's phase, in cycles.
    phase: f64,
    pub rate_hz: Knob,
    pub depth: Knob,
    pub delay: Knob,
    pub mix: Knob,
    values: [Vec<f64>; 4],
}

impl Chorus {
    pub fn new(rate_hz: Knob, depth: Knob, delay: Knob, mix: Knob, rate: f64, max_block: usize) -> Chorus {
        let frames = (LONGEST_MS / 1000.0 * rate).ceil() as usize + 3;
        Chorus {
            rate,
            line: vec![[0.0; 2]; frames],
            position: 0,
            phase: 0.0,
            rate_hz,
            depth,
            delay,
            mix,
            values: [0; 4].map(|_| vec![0.0; max_block]),
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        let [rates, depths, delays, mixes] = &mut self.values;
        self.rate_hz.fill(clock.frame, clock.step, &mut rates[..n]);
        self.depth.fill(clock.frame, clock.step, &mut depths[..n]);
        self.delay.fill(clock.frame, clock.step, &mut delays[..n]);
        self.mix.fill(clock.frame, clock.step, &mut mixes[..n]);
        let per_ms = self.rate / 1000.0;
        let line = &mut self.line;
        let mut position = self.position;
        let mut phase = self.phase;
        for i in 0..n {
            let frame = x[i];
            line[position] = frame;
            let centre = delays[i] * per_ms;
            let depth = depths[i] * per_ms;
            let left = centre + depth * (TAU * phase).sin();
            let right = centre + depth * (TAU * (phase - 0.25)).sin();
            let wet = [read(line, position, left, 0), read(line, position, right, 1)];
            let mix = mixes[i] / 100.0;
            x[i] = [frame[0] * (1.0 - mix) + wet[0] * mix, frame[1] * (1.0 - mix) + wet[1] * mix];
            position = (position + 1) % line.len();
            phase = (phase + rates[i] / self.rate).rem_euclid(1.0);
        }
        self.position = position;
        self.phase = phase;
    }

    /// Continues from `old`'s line and sweep; the knobs glide to their own
    /// values.
    pub fn take_over(&mut self, old: &mut Chorus) {
        if old.line.len() == self.line.len() {
            std::mem::swap(&mut self.line, &mut old.line);
            self.position = old.position;
        }
        self.phase = old.phase;
        self.rate_hz.take_over(&old.rate_hz);
        self.depth.take_over(&old.depth);
        self.delay.take_over(&old.delay);
        self.mix.take_over(&old.mix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Param;

    fn clock(frame: i64) -> Clock {
        Clock {
            frame,
            step: 1,
            total: 1 << 40,
        }
    }

    fn chorus(rate_hz: f64, depth_ms: f64, delay_ms: f64, mix: f64) -> Chorus {
        let knob = |v: f64| Knob::new(Param::fixed(v), 240);
        Chorus::new(knob(rate_hz), knob(depth_ms), knob(delay_ms), knob(mix), 48000.0, 4096)
    }

    #[test]
    fn a_still_chorus_is_an_echo_and_a_moving_one_sweeps_without_depending_on_blocks() {
        // No depth: the wet signal is the input 10 ms ago, 480 frames.
        let mut x = vec![[0.0; 2]; 2000];
        x[0] = [1.0, 0.5];
        chorus(1.0, 0.0, 10.0, 100.0).process(&mut x, clock(0));
        assert!((x[480][0] - 1.0).abs() < 1e-9 && (x[480][1] - 0.5).abs() < 1e-9, "{:?}", &x[478..483]);
        assert_eq!(x.iter().filter(|f| f[0] != 0.0).count(), 1);
        // A sweep moves a tone's pitch up and down: the zero crossings of a
        // second drift either side of the input's.
        let tone: Vec<Frame> = (0..48000).map(|i| [(TAU * 440.0 * i as f64 / 48000.0).sin(); 2]).collect();
        let mut whole = tone.clone();
        let mut c = chorus(2.0, 5.0, 12.0, 100.0);
        for (b, chunk) in whole.chunks_mut(4096).enumerate() {
            c.process(chunk, clock(b as i64 * 4096));
        }
        let mut pieces = tone.clone();
        let mut c = chorus(2.0, 5.0, 12.0, 100.0);
        let mut at = 0;
        for chunk in pieces.chunks_mut(333) {
            c.process(chunk, clock(at));
            at += chunk.len() as i64;
        }
        assert_eq!(whole, pieces);
        let crossings = |x: &[Frame], from: usize, to: usize| x[from..to].windows(2).filter(|w| w[0][0] <= 0.0 && w[1][0] > 0.0).count();
        // Over a quarter cycle of the sweep (125 ms) the delay grows, so the
        // pitch is lower; over the next it shrinks and the pitch is higher.
        let (down, up) = (crossings(&whole, 1000, 7000), crossings(&whole, 7000, 13000));
        assert!(down < 55 && up > 55, "{down} {up}");
        // The two channels are a quarter cycle apart: not the same signal.
        assert!(whole[1000..7000].iter().any(|f| (f[0] - f[1]).abs() > 0.05));
        // Dry is dry.
        let mut dry = tone.clone();
        let mut c = chorus(2.0, 5.0, 12.0, 0.0);
        for chunk in dry.chunks_mut(4096) {
            c.process(chunk, clock(0));
        }
        assert_eq!(dry, tone);
    }
}

//! Tempo-synced feedback delay, as `effects.DelayDevice`:
//! `wet[n] = F(input[n - D] + feedback * wet[n - D])`.
//!
//! F is the optional low and high cut, so every repeat passes through it again
//! and darkens. Ping-pong feeds the mono input to the left and crosses the
//! feedback between channels.

use crate::biquad::{butter, Cascade};
use crate::envelope::Knob;
use crate::{Clock, Frame};

#[derive(Clone, Debug)]
pub struct Delay {
    frames: usize,
    ping_pong: bool,
    filter: Option<Cascade>,
    /// The last `frames` frames fed to the line, and the wet signal they made.
    inputs: Vec<Frame>,
    wet: Vec<Frame>,
    position: usize,
    pub feedback: Knob,
    pub mix: Knob,
    feedbacks: Vec<f64>,
    mixes: Vec<f64>,
}

impl Delay {
    /// `frames` is the delay time; the cuts are in Hz. After a live edit the
    /// cuts cross over in `glide` frames.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        frames: usize,
        lowcut_hz: Option<f64>,
        highcut_hz: Option<f64>,
        ping_pong: bool,
        feedback: Knob,
        mix: Knob,
        rate: f64,
        max_block: usize,
        glide: usize,
    ) -> Delay {
        let frames = frames.max(1);
        let mut sections = Vec::new();
        if let Some(hz) = lowcut_hz.filter(|hz| *hz != 0.0) {
            sections.extend(butter(2, hz, true, rate));
        }
        if let Some(hz) = highcut_hz.filter(|hz| *hz != 0.0) {
            sections.extend(butter(2, hz, false, rate));
        }
        Delay {
            frames,
            ping_pong,
            filter: (!sections.is_empty()).then(|| Cascade::with_glide(sections, glide)),
            inputs: vec![[0.0; 2]; frames],
            wet: vec![[0.0; 2]; frames],
            position: 0,
            feedback,
            mix,
            feedbacks: vec![0.0; max_block],
            mixes: vec![0.0; max_block],
        }
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        self.feedback.fill(clock.frame, clock.step, &mut self.feedbacks[..n]);
        self.mix.fill(clock.frame, clock.step, &mut self.mixes[..n]);
        for (i, frame) in x.iter_mut().enumerate() {
            let feedback = self.feedbacks[i] / 100.0;
            let mix = self.mixes[i] / 100.0;
            let source = if self.ping_pong { [(frame[0] + frame[1]) / 2.0, 0.0] } else { *frame };
            let mut fed = self.wet[self.position];
            if self.ping_pong {
                fed.swap(0, 1);
            }
            let delayed = self.inputs[self.position];
            let mut v = [delayed[0] + feedback * fed[0], delayed[1] + feedback * fed[1]];
            if let Some(filter) = &mut self.filter {
                v = filter.tick(v);
            }
            self.wet[self.position] = v;
            self.inputs[self.position] = source;
            self.position = (self.position + 1) % self.frames;
            *frame = [frame[0] * (1.0 - mix) + v[0] * mix, frame[1] * (1.0 - mix) + v[1] * mix];
        }
    }

    /// Continues from `old`'s line, when it is as long; the feedback and mix
    /// glide to their own values.
    pub fn take_over(&mut self, old: &mut Delay) {
        if old.frames != self.frames {
            return;
        }
        std::mem::swap(&mut self.inputs, &mut old.inputs);
        std::mem::swap(&mut self.wet, &mut old.wet);
        self.position = old.position;
        if let (Some(new), Some(was)) = (&mut self.filter, &old.filter) {
            new.take_over(was);
        }
        self.feedback.take_over(&old.feedback);
        self.mix.take_over(&old.mix);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::Param;

    fn clock() -> Clock {
        Clock {
            frame: 0,
            step: 1,
            total: 1 << 40,
        }
    }

    fn delay(frames: usize, feedback: f64, mix: f64, ping_pong: bool, highcut: Option<f64>) -> Delay {
        Delay::new(
            frames,
            None,
            highcut,
            ping_pong,
            Knob::new(Param::fixed(feedback), 240),
            Knob::new(Param::fixed(mix), 240),
            48000.0,
            4096,
            240,
        )
    }

    fn click(n: usize) -> Vec<Frame> {
        let mut x = vec![[0.0; 2]; n];
        x[0] = [1.0, 0.5];
        x
    }

    #[test]
    fn echoes_land_on_exact_frames_with_feedback_gains() {
        let mut x = click(4000);
        let mut d = delay(1000, 50.0, 100.0, false, None);
        for chunk in x.chunks_mut(333) {
            d.process(chunk, clock());
        }
        for (k, gain) in [(1, 1.0), (2, 0.5), (3, 0.25)] {
            assert_eq!(x[k * 1000], [gain, gain * 0.5], "echo {k}");
        }
        assert_eq!(x.iter().filter(|f| **f != [0.0; 2]).count(), 3);
    }

    #[test]
    fn ping_pong_alternates_and_the_cut_darkens() {
        let mut x = click(3500);
        delay(1000, 50.0, 100.0, true, None).process(&mut x, clock());
        assert_eq!(x[1000], [0.75, 0.0]);
        assert_eq!(x[2000], [0.0, 0.375]);
        assert_eq!(x[3000], [0.1875, 0.0]);
        let mut y = click(3500);
        delay(1000, 80.0, 100.0, false, Some(2000.0)).process(&mut y, clock());
        let energy = |a: usize| y[a..a + 500].iter().map(|f| f[0] * f[0]).sum::<f64>();
        let peak = |a: usize| y[a..a + 500].iter().fold(0.0f64, |m, f| m.max(f[0].abs()));
        // Each repeat is filtered again: its peak falls faster than feedback alone.
        assert!(peak(2000) < peak(1000) * 0.8 && energy(2000) > 0.0);
    }

    #[test]
    fn a_dry_mix_passes_the_input_unchanged() {
        let x: Vec<Frame> = (0..3000).map(|i| [(i as f64 * 0.01).sin(), (i as f64 * 0.013).cos()]).collect();
        let mut y = x.clone();
        delay(700, 60.0, 0.0, false, Some(3000.0)).process(&mut y, clock());
        assert_eq!(x, y);
    }
}

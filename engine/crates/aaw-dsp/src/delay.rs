//! Tempo-synced feedback delay:
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
    /// The delay time from each frame on, the first from the start: one time,
    /// or one for each step of the song's tempo.
    times: Vec<(i64, usize)>,
    /// The time now, and the one it is crossfading from with how far to go.
    frames: usize,
    from: usize,
    left: usize,
    crossfade: usize,
    ping_pong: bool,
    filter: Option<Cascade>,
    /// The last frames fed to the line, as many as the longest time, and the
    /// wet signal they made.
    inputs: Vec<Frame>,
    wet: Vec<Frame>,
    position: usize,
    pub feedback: Knob,
    pub mix: Knob,
    feedbacks: Vec<f64>,
    mixes: Vec<f64>,
}

/// How long a delay takes to move from one time to the next at a tempo step.
const CROSSFADE_SECONDS: f64 = 0.02;

impl Delay {
    /// `times` is the delay time from each timeline frame on, the first
    /// for the whole song before the next; the cuts are in Hz. After a live
    /// edit the cuts cross over in `glide` frames.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        times: Vec<(i64, usize)>,
        lowcut_hz: Option<f64>,
        highcut_hz: Option<f64>,
        ping_pong: bool,
        feedback: Knob,
        mix: Knob,
        rate: f64,
        max_block: usize,
        glide: usize,
    ) -> Delay {
        let mut times: Vec<(i64, usize)> = times.into_iter().map(|(f, d)| (f, d.max(1))).collect();
        if times.is_empty() {
            times.push((i64::MIN, 1));
        }
        times[0].0 = i64::MIN;
        let longest = times.iter().map(|t| t.1).max().unwrap_or(1);
        let mut sections = Vec::new();
        if let Some(hz) = lowcut_hz.filter(|hz| *hz != 0.0) {
            sections.extend(butter(2, hz, true, rate));
        }
        if let Some(hz) = highcut_hz.filter(|hz| *hz != 0.0) {
            sections.extend(butter(2, hz, false, rate));
        }
        Delay {
            frames: times[0].1,
            from: times[0].1,
            left: 0,
            crossfade: ((CROSSFADE_SECONDS * rate).round() as usize).max(1),
            times,
            ping_pong,
            filter: (!sections.is_empty()).then(|| Cascade::with_glide(sections, glide)),
            inputs: vec![[0.0; 2]; longest],
            wet: vec![[0.0; 2]; longest],
            position: 0,
            feedback,
            mix,
            feedbacks: vec![0.0; max_block],
            mixes: vec![0.0; max_block],
        }
    }

    /// The time at a timeline frame.
    fn time_at(&self, frame: i64) -> usize {
        let i = self.times.partition_point(|t| t.0 <= frame);
        self.times[i.saturating_sub(1)].1
    }

    /// What the line gives `frames` back: the input then and the wet signal
    /// it made fed back.
    #[inline]
    fn tap(&self, frames: usize, feedback: f64) -> Frame {
        let len = self.inputs.len();
        let at = (self.position + len - frames) % len;
        let mut fed = self.wet[at];
        if self.ping_pong {
            fed.swap(0, 1);
        }
        let delayed = self.inputs[at];
        [delayed[0] + feedback * fed[0], delayed[1] + feedback * fed[1]]
    }

    pub fn process(&mut self, x: &mut [Frame], clock: Clock) {
        let n = x.len();
        self.feedback.fill(clock.frame, clock.step, &mut self.feedbacks[..n]);
        self.mix.fill(clock.frame, clock.step, &mut self.mixes[..n]);
        let moves = self.times.len() > 1;
        let len = self.inputs.len();
        for (i, frame) in x.iter_mut().enumerate() {
            if moves {
                let want = self.time_at(clock.frame + i as i64 * clock.step);
                if want != self.frames {
                    self.from = self.frames;
                    self.frames = want;
                    self.left = self.crossfade;
                }
            }
            let feedback = self.feedbacks[i] / 100.0;
            let mix = self.mixes[i] / 100.0;
            let source = if self.ping_pong { [(frame[0] + frame[1]) / 2.0, 0.0] } else { *frame };
            let mut v = self.tap(self.frames, feedback);
            if self.left > 0 {
                // The step's crossfade, from the old time to the new.
                let old = self.tap(self.from, feedback);
                let g = self.left as f64 / self.crossfade as f64;
                v = [g * old[0] + (1.0 - g) * v[0], g * old[1] + (1.0 - g) * v[1]];
                self.left -= 1;
            }
            if let Some(filter) = &mut self.filter {
                v = filter.tick(v);
            }
            self.wet[self.position] = v;
            self.inputs[self.position] = source;
            self.position = (self.position + 1) % len;
            *frame = [frame[0] * (1.0 - mix) + v[0] * mix, frame[1] * (1.0 - mix) + v[1] * mix];
        }
    }

    /// Continues from `old`'s line, when its times are the same; the
    /// feedback and mix glide to their own values.
    pub fn take_over(&mut self, old: &mut Delay) {
        if old.times != self.times {
            return;
        }
        std::mem::swap(&mut self.inputs, &mut old.inputs);
        std::mem::swap(&mut self.wet, &mut old.wet);
        self.position = old.position;
        (self.frames, self.from, self.left) = (old.frames, old.from, old.left);
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
            vec![(0, frames)],
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

    #[test]
    fn a_tempo_step_moves_the_time_from_its_frame() {
        // 1000 frames until frame 10000, then 600; a click after the step
        // echoes 600 frames later, and one before it 1000 frames later.
        let mut d = Delay::new(
            vec![(0, 1000), (10000, 600)],
            None,
            None,
            false,
            Knob::new(Param::fixed(0.0), 240),
            Knob::new(Param::fixed(100.0), 240),
            48000.0,
            4096,
            240,
        );
        let mut x = vec![[0.0; 2]; 20000];
        x[100] = [1.0, 1.0];
        x[15000] = [1.0, 1.0];
        for (i, block) in x.chunks_mut(512).enumerate() {
            d.process(block, Clock { frame: (i * 512) as i64, step: 1, total: 1 << 40 });
        }
        assert_eq!(x[1100], [1.0, 1.0]);
        assert_eq!(x[15600], [1.0, 1.0]);
        assert!(x[16000][0].abs() < 1e-12);
    }
}

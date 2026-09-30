//! The one processing path for offline export and real-time playback.
//!
//! A `Renderer` produces consecutive blocks of any size from any starting frame.
//! Every frame is computed from the voice's own position, so the output does not
//! depend on how the timeline is cut into blocks, and a render started mid-song
//! equals the same frames of a render from the start. `process` and `seek` never
//! allocate, so the real-time transport can reposition a renderer.

use crate::program::Program;
use std::sync::Arc;

pub type Frame = [f64; 2];

#[derive(Clone, Copy, Debug)]
struct Active {
    voice: usize,
    cursor: usize,
    /// Frames since a seek picked this voice up mid-sample, while it fades in;
    /// `DONE` for voices that play at full level.
    ramp: usize,
}

const DONE: usize = usize::MAX;

struct TrackState {
    /// The next voice not yet started.
    next: usize,
    /// Sounding voices in the order they started.
    active: Vec<Active>,
}

pub struct Renderer {
    program: Arc<Program>,
    position: usize,
    tracks: Vec<TrackState>,
    scratch: Vec<Frame>,
    /// Whether voices start; a renderer fading out lets sounding voices ring on.
    triggers: bool,
    /// Frames over which voices picked up by the last seek fade in.
    ramp: usize,
}

impl Renderer {
    /// A renderer positioned at `from`, with voices that started earlier and are
    /// still sounding picked up mid-sample. Blocks may be up to `max_block` frames.
    pub fn new(program: Arc<Program>, from: usize, max_block: usize) -> Renderer {
        let tracks = program
            .tracks
            .iter()
            .map(|t| TrackState {
                next: 0,
                // A voice stays listed from the block it starts in to the block it
                // ends in, so capacity covers overlaps widened by one block.
                active: Vec::with_capacity(overlap(&t.voices, max_block)),
            })
            .collect();
        let mut r = Renderer {
            program,
            position: 0,
            tracks,
            scratch: vec![[0.0; 2]; max_block],
            triggers: true,
            ramp: 0,
        };
        r.seek(from, 0);
        r
    }

    /// Moves to `from` and picks up the voices sounding there, fading them in over
    /// `ramp` frames (0 for none). Voices starting at or after `from` play in full.
    /// Re-enables triggers. Does not allocate.
    pub fn seek(&mut self, from: usize, ramp: usize) {
        self.position = from;
        self.triggers = true;
        self.ramp = ramp;
        for (t, state) in self.program.tracks.iter().zip(self.tracks.iter_mut()) {
            state.active.clear();
            state.next = 0;
            for (i, v) in t.voices.iter().enumerate() {
                if v.start >= from as i64 {
                    break;
                }
                let elapsed = (from as i64 - v.start) as usize;
                if elapsed < v.length {
                    state.active.push(Active {
                        voice: i,
                        cursor: elapsed,
                        ramp: if ramp > 0 { 0 } else { DONE },
                    });
                }
                state.next = i + 1;
            }
        }
    }

    /// Stops or resumes starting new voices. Sounding voices continue either way.
    pub fn set_triggers(&mut self, on: bool) {
        self.triggers = on;
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn program(&self) -> &Arc<Program> {
        &self.program
    }

    pub fn max_block(&self) -> usize {
        self.scratch.len()
    }

    /// Renders the next `mix.len()` frames into `mix`, after the master gain and
    /// end fade. `stem` receives each track's block after its gain, pan, mute and
    /// solo, before the master envelope. Frames past the session end are silent.
    pub fn process(&mut self, mix: &mut [Frame], mut stem: impl FnMut(usize, &[Frame])) {
        let n = mix.len();
        assert!(n <= self.scratch.len(), "block larger than max_block");
        mix.fill([0.0; 2]);
        let program = &*self.program;
        let start = self.position;
        let end = (start + n).min(program.total).max(start);
        let count = end - start;
        let buf = &mut self.scratch[..n];
        for (ti, track) in program.tracks.iter().enumerate() {
            buf.fill([0.0; 2]);
            let state = &mut self.tracks[ti];
            while state.next < track.voices.len() && track.voices[state.next].start < end as i64 {
                if self.triggers {
                    state.active.push(Active {
                        voice: state.next,
                        cursor: 0,
                        ramp: DONE,
                    });
                }
                state.next += 1;
            }
            for a in state.active.iter_mut() {
                let v = &track.voices[a.voice];
                let offset = start.max(v.start as usize);
                let frames = (end - offset).min(v.length - a.cursor);
                let out = &mut buf[offset - start..offset - start + frames];
                if a.ramp == DONE {
                    for (k, o) in out.iter_mut().enumerate() {
                        let i = a.cursor + k;
                        o[0] += v.sample(i, 0);
                        o[1] += v.sample(i, 1);
                    }
                } else {
                    let ramp = self.ramp;
                    for (k, o) in out.iter_mut().enumerate() {
                        let i = a.cursor + k;
                        let g = ((a.ramp + k) as f64 / ramp as f64).min(1.0);
                        o[0] += v.sample(i, 0) * g;
                        o[1] += v.sample(i, 1) * g;
                    }
                    a.ramp = if a.ramp + frames >= ramp { DONE } else { a.ramp + frames };
                }
                a.cursor += frames;
            }
            state.active.retain(|a| a.cursor < track.voices[a.voice].length);
            for f in buf[..count].iter_mut() {
                if track.silent {
                    *f = [0.0; 2];
                } else {
                    *f = [f[0] * track.pan[0] * track.gain, f[1] * track.pan[1] * track.gain];
                }
            }
            for (m, f) in mix[..count].iter_mut().zip(buf[..count].iter()) {
                m[0] += f[0];
                m[1] += f[1];
            }
            stem(ti, &buf[..n]);
        }
        for (k, m) in mix[..count].iter_mut().enumerate() {
            let env = program.envelope(start + k);
            m[0] *= env;
            m[1] *= env;
        }
        self.position = start + n;
    }

    /// Whether the session end has been reached.
    pub fn finished(&self) -> bool {
        self.position >= self.program.total
    }
}

/// The most voices listed at once when each is held `extra` frames past its end.
fn overlap(voices: &[crate::program::Voice], extra: usize) -> usize {
    let mut edges: Vec<(i64, i32)> = Vec::with_capacity(voices.len() * 2);
    for v in voices {
        edges.push((v.start, 1));
        edges.push((v.start + (v.length + extra) as i64, -1));
    }
    edges.sort();
    let (mut now, mut most) = (0i32, 0i32);
    for (_, d) in edges {
        now += d;
        most = most.max(now);
    }
    most as usize
}

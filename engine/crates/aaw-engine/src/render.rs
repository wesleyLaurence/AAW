//! The one processing path for offline export and real-time playback.
//!
//! A `Renderer` produces consecutive blocks of any size from any starting frame.
//! Every frame is computed from the voice's own position, so the output does not
//! depend on how the timeline is cut into blocks, and a render started mid-song
//! equals the same frames of a render from the start. `process` never allocates.

use crate::program::Program;
use std::sync::Arc;

pub type Frame = [f64; 2];

#[derive(Clone, Copy, Debug)]
struct Active {
    voice: usize,
    cursor: usize,
}

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
}

impl Renderer {
    /// A renderer positioned at `from`, with voices that started earlier and are
    /// still sounding picked up mid-sample. Blocks may be up to `max_block` frames.
    pub fn new(program: Arc<Program>, from: usize, max_block: usize) -> Renderer {
        let tracks = program
            .tracks
            .iter()
            .map(|t| {
                // A voice stays listed from the block it starts in to the block it
                // ends in, so capacity covers overlaps widened by one block.
                let mut active = Vec::with_capacity(overlap(&t.voices, max_block));
                let mut next = 0;
                for (i, v) in t.voices.iter().enumerate() {
                    if v.start >= from as i64 {
                        break;
                    }
                    let elapsed = (from as i64 - v.start) as usize;
                    if elapsed < v.length {
                        active.push(Active {
                            voice: i,
                            cursor: elapsed,
                        });
                    }
                    next = i + 1;
                }
                TrackState { next, active }
            })
            .collect();
        Renderer {
            program,
            position: from,
            tracks,
            scratch: vec![[0.0; 2]; max_block],
        }
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
                state.active.push(Active {
                    voice: state.next,
                    cursor: 0,
                });
                state.next += 1;
            }
            for a in state.active.iter_mut() {
                let v = &track.voices[a.voice];
                let offset = start.max(v.start as usize);
                let frames = (end - offset).min(v.length - a.cursor);
                let out = &mut buf[offset - start..offset - start + frames];
                for (k, o) in out.iter_mut().enumerate() {
                    let i = a.cursor + k;
                    o[0] += v.sample(i, 0);
                    o[1] += v.sample(i, 1);
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

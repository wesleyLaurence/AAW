//! A take: what an armed track records while the song plays, from an input
//! device or from the song's own sound, and how it becomes a sample and an
//! audio clip when it ends.
//!
//! Frames arrive from the engine's captures in blocks headed by when each
//! is: a tapped stem's block carries its timeline frame, an input block the
//! instant it was captured, which the output's anchor turns into a timeline
//! frame. When the take ends the frames are laid on the timeline, pulled
//! earlier by the devices' reported latency, resampled to the song's rate
//! where the device ran at another, cut to the span from the start position
//! to where the take ended, and written as a 32-bit float WAV under
//! `samples/`; the host then adds the sample and the clip as one edit.

use aaw_engine::capture::{Capture, ChunkTime, InputStream};
use aaw_engine::player::Shared;
use aaw_model::{Input, Project};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;

/// How long a capture's rings hold undrained: the host drains them every
/// tick, many times a second.
pub const RING_SECONDS: f64 = 8.0;

enum Source {
    /// An input device's stream, and the output's latency in the song's
    /// frames, which the person heard the song through.
    Device { stream: InputStream, output_latency: u32 },
    /// A tap in the player.
    Tap,
}

/// One track's take as it is recorded.
pub struct Take {
    pub track: String,
    pub input: Input,
    source: Source,
    capture: Capture,
    /// The rate the frames are at: the device's, or the song's for a tap.
    rate: u32,
    channels: usize,
    samples: Vec<f32>,
    /// Each block's time and how many frames it has, in order.
    chunks: Vec<(ChunkTime, usize)>,
}

impl Take {
    /// A take from an input device: the stream opened on it, at the song's
    /// rate when it offers it.
    pub fn from_device(track: &str, input: &Input, name: Option<&str>, channels: &[i64], song_rate: u32, output_latency: u32) -> Result<Take> {
        let (stream, capture) = aaw_engine::capture::open_input(name, channels, song_rate, RING_SECONDS)?;
        Ok(Take {
            track: track.into(),
            input: input.clone(),
            rate: stream.rate,
            channels: capture.channels,
            source: Source::Device { stream, output_latency },
            capture,
            samples: Vec::new(),
            chunks: Vec::new(),
        })
    }

    /// A take from a tap in the player, whose capture is `capture`.
    pub fn from_tap(track: &str, input: &Input, capture: Capture, song_rate: u32) -> Take {
        Take {
            track: track.into(),
            input: input.clone(),
            rate: song_rate,
            channels: capture.channels,
            source: Source::Tap,
            capture,
            samples: Vec::new(),
            chunks: Vec::new(),
        }
    }

    /// The device the take records through, for a device take.
    pub fn device(&self) -> Option<&str> {
        match &self.source {
            Source::Device { stream, .. } => Some(&stream.device),
            Source::Tap => None,
        }
    }

    /// Whether the driver stopped the input stream.
    pub fn dead(&self) -> bool {
        matches!(&self.source, Source::Device { stream, .. } if stream.dead())
    }

    /// Takes what has arrived into the take.
    pub fn drain(&mut self) {
        let taken = self.capture.drain(&mut self.samples);
        self.chunks.extend(taken);
    }

    /// Frames recorded so far, at the take's rate.
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    /// The timeline frame, at the song's rate, that the take's first frame
    /// is at: a fraction, from the first block's time. None until a block
    /// has arrived, or, for an input take, until the output has an anchor.
    fn first_frame(&self, song_rate: u32, anchor: Option<(u64, i64)>) -> Option<f64> {
        let (time, _) = self.chunks.first()?;
        match time {
            ChunkTime::Frame(f) => Some(*f as f64),
            ChunkTime::Ns(ns) => {
                let (anchor_ns, anchor_frame) = anchor?;
                let delta = *ns as f64 - anchor_ns as f64;
                Some(anchor_frame as f64 + delta * song_rate as f64 / 1e9)
            }
        }
    }

    /// How many of the song's frames the take is pulled earlier by: the
    /// input device's reported latency and the output's, since the person
    /// played against what they heard late and the frames reached the
    /// stream late. A tap is exact and pulled by nothing.
    fn pull(&self, song_rate: u32) -> f64 {
        match &self.source {
            Source::Device { stream, output_latency } => {
                stream.latency_frames as f64 * song_rate as f64 / stream.rate as f64 + *output_latency as f64
            }
            Source::Tap => 0.0,
        }
    }

    /// Lays the take on the timeline and writes the part from `start` to
    /// `end` (song frames) under the project's `samples/`, as the sample
    /// `id`. `anchor` is the output's, for an input take.
    fn finish(mut self, dir: &Path, id: &str, song_rate: u32, start: i64, end: Option<i64>, anchor: Option<(u64, i64)>) -> Result<Finished> {
        self.drain();
        let lost = self.capture.lost();
        let first = self
            .first_frame(song_rate, anchor)
            .ok_or_else(|| format!("{}: nothing was recorded; the take ended before the count-in was over, or the input gave no frames", self.track))?;
        let pull = self.pull(song_rate);
        let channels = self.channels.max(1);
        let mut data: Vec<f64> = self.samples.iter().map(|x| *x as f64).collect();
        if self.rate != song_rate {
            data = aaw_dsp::resample::resample_poly(&data, channels, song_rate as usize, self.rate as usize);
        }
        let frames = data.len() / channels;
        // Where the take's first frame lands, pulled earlier by the latency.
        let first = first - pull;
        // The frames of the take that are from `start` on, and before `end`.
        let from = ((start as f64 - first).round().max(0.0) as usize).min(frames);
        let to = match end {
            Some(end) => ((end as f64 - first).round().max(0.0) as usize).clamp(from, frames),
            None => frames,
        };
        if to <= from {
            return Err(format!("{}: the take has no frames after the start position", self.track));
        }
        // The clip starts at `start`, or later when the take began after it.
        let at = start.max(first.round() as i64);
        let kept: Vec<f32> = data[from * channels..to * channels].iter().map(|x| *x as f32).collect();
        let bytes = aaw_engine::wav::float_wav_interleaved(&kept, channels as u16, song_rate);
        let sha256 = hex(&Sha256::digest(&bytes));
        let relative = PathBuf::from("samples").join(format!("{id}.wav"));
        aaw_engine::wav::write_atomic(&dir.join(&relative), &bytes)?;
        Ok(Finished {
            track: self.track.clone(),
            input: self.input.clone(),
            device: self.device().map(str::to_string),
            id: id.into(),
            path: relative.to_string_lossy().into_owned(),
            sha256,
            seconds: (to - from) as f64 / song_rate as f64,
            at,
            pulled_ms: pull / song_rate as f64 * 1000.0,
            lost,
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A take written to its file: what the host adds to the song.
#[derive(Clone, Debug)]
pub struct Finished {
    pub track: String,
    pub input: Input,
    pub device: Option<String>,
    pub id: String,
    pub path: String,
    pub sha256: String,
    pub seconds: f64,
    /// The song frame the clip begins at.
    pub at: i64,
    pub pulled_ms: f64,
    pub lost: u64,
}

impl Finished {
    /// The sample's record for `samples.ID`.
    pub fn sample(&self) -> Json {
        json!({
            "path": self.path,
            "sha256": self.sha256,
            "source": format!("recorded from {}", self.device.clone().unwrap_or_else(|| self.input.label())),
        })
    }

    /// The take as the reply reports it.
    pub fn report(&self, song_rate: u32, tempo: f64) -> Json {
        json!({
            "track": self.track,
            "sample": self.id,
            "path": self.path,
            "input": self.input.label(),
            "device": self.device,
            "seconds": (self.seconds * 1000.0).round() / 1000.0,
            "at_beat": (self.at as f64 * tempo / 60.0 / song_rate as f64 * 1000.0).round() / 1000.0,
            "pulled_ms": (self.pulled_ms * 100.0).round() / 100.0,
            "lost_frames": self.lost,
        })
    }
}

/// The takes of one recording, from Record to its end.
pub struct Recording {
    pub takes: Vec<Take>,
    /// The song frame the takes' clips begin at: the start position.
    pub start: i64,
    /// Frames of count-in before it.
    pub count_in: usize,
    /// The loop's end frame, where the recording ends on its own while the
    /// loop is on.
    pub loop_end: Option<usize>,
    /// The player's wrap count when the recording began.
    pub wraps: u64,
    pub began: Instant,
    /// Whether the transport has been seen rolling since Record: before
    /// that, a stopped transport is one that has not started yet.
    pub rolled: bool,
    shared: Arc<Shared>,
}

impl Recording {
    pub fn new(takes: Vec<Take>, start: i64, count_in: usize, loop_end: Option<usize>, shared: Arc<Shared>) -> Recording {
        Recording {
            takes,
            start,
            count_in,
            loop_end,
            wraps: shared.wraps.load(std::sync::atomic::Ordering::Relaxed),
            began: Instant::now(),
            rolled: false,
            shared,
        }
    }

    /// Takes what has arrived into each take.
    pub fn drain(&mut self) {
        for take in &mut self.takes {
            take.drain();
        }
    }

    /// Whether playback has wrapped to the loop's start since the recording
    /// began, which ends it at the loop's end.
    pub fn wrapped(&self) -> bool {
        self.loop_end.is_some() && self.shared.wraps.load(std::sync::atomic::Ordering::Relaxed) > self.wraps
    }

    /// Whether any take records through a device.
    pub fn uses_device(&self) -> bool {
        self.takes.iter().any(|t| t.device().is_some())
    }

    /// The tracks recording.
    pub fn tracks(&self) -> Vec<String> {
        self.takes.iter().map(|t| t.track.clone()).collect()
    }

    /// Ends the recording: each take written under `dir/samples/`, named
    /// `TRACK-take-N` after the takes the song has, from the start to
    /// `end`, or to each take's last frame with None.
    pub fn finish(self, dir: &Path, project: &Project, song_rate: u32, end: Option<i64>) -> Vec<Result<Finished>> {
        let anchor = self.shared.anchor();
        let mut taken: Vec<String> = project.samples.keys().cloned().collect();
        let start = self.start;
        self.takes
            .into_iter()
            .map(|take| {
                let id = next_take(&take.track, |id| taken.iter().any(|t| t == id) || dir.join("samples").join(format!("{id}.wav")).exists());
                taken.push(id.clone());
                take.finish(dir, &id, song_rate, start, end, anchor)
            })
            .collect()
    }
}

/// `TRACK-take-N`, the first N from 1 not taken.
fn next_take(track: &str, taken: impl Fn(&str) -> bool) -> String {
    (1..)
        .map(|n| format!("{track}-take-{n}"))
        .find(|id| !taken(id))
        .expect("a free take number")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_are_numbered_past_the_ones_the_song_has() {
        assert_eq!(next_take("vox", |_| false), "vox-take-1");
        let has = ["vox-take-1", "vox-take-2", "gtr-take-1"];
        assert_eq!(next_take("vox", |id| has.contains(&id)), "vox-take-3");
        assert_eq!(next_take("gtr", |id| has.contains(&id)), "gtr-take-2");
    }
}

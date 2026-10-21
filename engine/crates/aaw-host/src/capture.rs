//! A sound captured from an input with no track: the agent panel's Record
//! Audio, and a quick capture through a running host. The input is opened
//! as a take's is and its frames are drained every tick; when it stops the
//! sound is resampled to the song's rate and written as a 32-bit float WAV
//! where it was asked for. While the song plays, the timeline frame the
//! first frame was heard against is kept through the output's anchor and
//! pulled earlier by the devices' latency, as a take's is, so a line hummed
//! along from bar 5 says so.

use aaw_engine::capture::{Capture as Ring, ChunkTime, InputStream};
use serde_json::{json, Value as Json};
use std::path::{Path, PathBuf};
use std::time::Instant;

type Result<T> = std::result::Result<T, String>;

/// One sound as it is captured.
pub struct Capture {
    /// Where the sound is written when it stops.
    pub file: PathBuf,
    stream: InputStream,
    ring: Ring,
    /// The rate the frames are at: the device's.
    rate: u32,
    channels: usize,
    samples: Vec<f32>,
    /// The instant the first block was captured, once one has arrived.
    first_ns: Option<u64>,
    /// The timeline frame, at the song's rate, the first frame was heard
    /// against, pulled by the latency: set at the first drain that finds
    /// the song playing with an anchor, and never moved after.
    first_frame: Option<f64>,
    /// The output's hardware latency in the song's frames when the capture
    /// began, 0 with no output open.
    output_latency: u32,
    pub began: Instant,
    /// The peak of the frames drained last, 0 to 1, for a meter.
    pub level: f32,
}

impl Capture {
    /// Opens the input `name` names, or the default with None, taking
    /// `channels` counted from 1, at the song's rate when the device
    /// offers it. `file` is where the sound will be written; it must not
    /// exist yet, and its folder is made now so a place that cannot be
    /// written refuses the start rather than the stop.
    pub fn open(file: PathBuf, name: Option<&str>, channels: &[i64], song_rate: u32, output_latency: u32) -> Result<Capture> {
        if file.exists() {
            return Err(format!("{} exists; choose another file", file.display()));
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let (stream, ring) = aaw_engine::capture::open_input(name, channels, song_rate, crate::record::RING_SECONDS)?;
        Ok(Capture {
            file,
            rate: stream.rate,
            channels: ring.channels,
            stream,
            ring,
            samples: Vec::new(),
            first_ns: None,
            first_frame: None,
            output_latency,
            began: Instant::now(),
            level: 0.0,
        })
    }

    pub fn device(&self) -> &str {
        &self.stream.device
    }

    /// Whether the driver stopped the input stream.
    pub fn dead(&self) -> bool {
        self.stream.dead()
    }

    /// Frames captured so far, at the device's rate.
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / self.rate as f64
    }

    pub fn lost(&self) -> u64 {
        self.ring.lost()
    }

    /// Where the first frame lands on the timeline, in the song's frames,
    /// once it is known.
    pub fn first_frame(&self) -> Option<f64> {
        self.first_frame
    }

    /// Takes what has arrived. `anchor` is the output's and `playing`
    /// whether the song rolls now: the first drain that finds both fixes
    /// where the first frame was heard.
    pub fn drain(&mut self, song_rate: u32, anchor: Option<(u64, i64)>, playing: bool) {
        let before = self.samples.len();
        let chunks = self.ring.drain(&mut self.samples);
        if self.first_ns.is_none() {
            if let Some((ChunkTime::Ns(ns), _)) = chunks.first() {
                self.first_ns = Some(*ns);
            }
        }
        if self.samples.len() > before {
            self.level = self.samples[before..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        }
        if self.first_frame.is_none() && playing {
            if let (Some(first_ns), Some((anchor_ns, anchor_frame))) = (self.first_ns, anchor) {
                let heard = anchor_frame as f64 + (first_ns as f64 - anchor_ns as f64) * song_rate as f64 / 1e9;
                self.first_frame = Some(heard - self.pull(song_rate));
            }
        }
    }

    /// How many of the song's frames the sound is pulled earlier by: the
    /// input device's reported latency and the output's.
    fn pull(&self, song_rate: u32) -> f64 {
        self.stream.latency_frames as f64 * song_rate as f64 / self.rate as f64 + self.output_latency as f64
    }

    /// Writes the sound at the song's rate, resampled where the device ran
    /// at another, and says what landed.
    pub fn finish(mut self, song_rate: u32) -> Result<Captured> {
        let frames = self.frames();
        if frames == 0 {
            return Err(format!("Nothing was recorded: {} gave no frames", self.stream.device));
        }
        let channels = self.channels.max(1);
        let lost = self.lost();
        let pull = self.pull(song_rate);
        let data: Vec<f32> = if self.rate != song_rate {
            let wide: Vec<f64> = self.samples.iter().map(|x| *x as f64).collect();
            aaw_dsp::resample::resample_poly(&wide, channels, song_rate as usize, self.rate as usize)
                .into_iter()
                .map(|x| x as f32)
                .collect()
        } else {
            std::mem::take(&mut self.samples)
        };
        let bytes = aaw_engine::wav::float_wav_interleaved(&data, channels as u16, song_rate);
        aaw_engine::wav::write_atomic(&self.file, &bytes)?;
        let peak = data.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        Ok(Captured {
            file: self.file.clone(),
            device: self.stream.device.clone(),
            channels,
            sample_rate: song_rate,
            frames: data.len() / channels,
            peak_dbfs: (peak > 0.0).then(|| 20.0 * (peak as f64).log10()),
            lost,
            at_frame: self.first_frame,
            pulled_ms: pull / song_rate as f64 * 1000.0,
        })
    }
}

/// A sound written to its file.
#[derive(Clone, Debug)]
pub struct Captured {
    pub file: PathBuf,
    pub device: String,
    pub channels: usize,
    pub sample_rate: u32,
    pub frames: usize,
    /// None for silence.
    pub peak_dbfs: Option<f64>,
    pub lost: u64,
    /// The song frame the sound's first frame was heard against, when the
    /// song was playing as it began.
    pub at_frame: Option<f64>,
    pub pulled_ms: f64,
}

impl Captured {
    pub fn seconds(&self) -> f64 {
        self.frames as f64 / self.sample_rate as f64
    }

    /// The sound as the reply reports it; `dir` is the project's folder,
    /// which a file under it is given relative to.
    pub fn report(&self, dir: &Path, tempo: f64) -> Json {
        let round3 = |x: f64| (x * 1000.0).round() / 1000.0;
        let file = self.file.strip_prefix(dir).unwrap_or(&self.file);
        json!({
            "file": file.to_string_lossy(),
            "device": self.device,
            "channels": self.channels,
            "sample_rate": self.sample_rate,
            "frames": self.frames,
            "seconds": round3(self.seconds()),
            "peak_dbfs": self.peak_dbfs.map(|p| (p * 100.0).round() / 100.0),
            "lost_frames": self.lost,
            "at_beat": self.at_frame.map(|f| round3(f * tempo / 60.0 / self.sample_rate as f64)),
            "pulled_ms": (self.pulled_ms * 100.0).round() / 100.0,
            "tempo": tempo,
        })
    }
}

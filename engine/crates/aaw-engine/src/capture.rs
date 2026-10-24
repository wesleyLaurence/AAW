//! Audio taken while the song plays: from an input device, the microphone
//! or an interface, through its own stream, or from the song's own sound, a
//! track's or a group's stem or the mix, tapped in the player. Either hands
//! blocks of frames to the host through lock-free rings, each block headed
//! by when it is: the timeline frame a tapped block was heard at, or the
//! instant an input block was captured, which the host turns into a
//! timeline frame through the anchor the output callback publishes
//! (`player::Shared::anchor`). Nothing here allocates on an audio thread.

use crate::program::StemKind;
use crate::render::Frame;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::StreamInstant;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;

/// When a block of captured frames is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChunkTime {
    /// The timeline frame of the block's first frame, at the song's rate:
    /// a tapped block, exact.
    Frame(i64),
    /// The instant the block's first frame was captured at the device, in
    /// nanoseconds of the audio clock (`instant_ns`): an input block.
    Ns(u64),
}

/// The header of one block in the ring: when it is and how many frames
/// follow it in the samples ring, interleaved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chunk {
    pub time: ChunkTime,
    pub frames: u32,
}

/// Nanoseconds of a stream instant on the audio clock, which the output's
/// playback instants and an input's capture instants share.
pub fn instant_ns(at: StreamInstant) -> u64 {
    at.duration_since(StreamInstant::ZERO).as_nanos() as u64
}

/// Blocks of frames on their way to the host: the chunk headers and the
/// samples, interleaved by `channels`. A block the rings have no room for
/// is counted in `lost` and left out whole, so the headers stay true.
pub struct Feed {
    chunks: rtrb::Producer<Chunk>,
    samples: rtrb::Producer<f32>,
    channels: usize,
    lost: Arc<AtomicU64>,
}

impl Feed {
    /// Pushes a block of stereo frames.
    pub fn push_frames(&mut self, time: ChunkTime, block: &[Frame]) {
        self.push_frames_scaled(time, block, |_| 1.0);
    }

    /// Pushes a block of stereo frames, each scaled by `gain` of its place
    /// in the block: the master's gain and end fade a stem carries.
    pub fn push_frames_scaled(&mut self, time: ChunkTime, block: &[Frame], gain: impl Fn(usize) -> f64) {
        debug_assert_eq!(self.channels, 2);
        if block.is_empty() || !self.room(block.len()) {
            return;
        }
        for (k, f) in block.iter().enumerate() {
            let g = gain(k);
            let _ = self.samples.push((f[0] * g) as f32);
            let _ = self.samples.push((f[1] * g) as f32);
        }
        let _ = self.chunks.push(Chunk {
            time,
            frames: block.len() as u32,
        });
    }

    /// Pushes a block of interleaved samples with `stride` channels a frame,
    /// keeping the channels at `picked` (counted from 0), in that order.
    pub fn push_picked(&mut self, time: ChunkTime, data: &[f32], stride: usize, picked: &[usize]) {
        let frames = data.len() / stride.max(1);
        if frames == 0 || !self.room(frames) {
            return;
        }
        for frame in data.chunks_exact(stride) {
            for c in picked {
                let _ = self.samples.push(frame.get(*c).copied().unwrap_or(0.0));
            }
        }
        let _ = self.chunks.push(Chunk {
            time,
            frames: frames as u32,
        });
    }

    fn room(&mut self, frames: usize) -> bool {
        if self.chunks.slots() == 0 || self.samples.slots() < frames * self.channels {
            self.lost.fetch_add(frames as u64, Relaxed);
            return false;
        }
        true
    }
}

/// The host's end of a feed.
pub struct Capture {
    chunks: rtrb::Consumer<Chunk>,
    samples: rtrb::Consumer<f32>,
    pub channels: usize,
    lost: Arc<AtomicU64>,
}

impl Capture {
    /// Takes every whole block that has arrived: its time, and its samples
    /// appended to `samples`, interleaved.
    pub fn drain(&mut self, samples: &mut Vec<f32>) -> Vec<(ChunkTime, usize)> {
        let mut taken = Vec::new();
        while let Ok(chunk) = self.chunks.peek().copied() {
            let n = chunk.frames as usize * self.channels;
            if self.samples.slots() < n {
                break;
            }
            let _ = self.chunks.pop();
            let at = samples.len();
            samples.resize(at + n, 0.0);
            let _ = self.samples.pop_entire_slice(&mut samples[at..]);
            taken.push((chunk.time, chunk.frames as usize));
        }
        taken
    }

    /// Frames the rings had no room for, which are missing from the take.
    pub fn lost(&self) -> u64 {
        self.lost.load(Relaxed)
    }
}

/// A feed and its capture, with room for `seconds` of audio at `rate`,
/// which is as long as the host may leave the rings undrained.
pub fn feed(channels: usize, rate: u32, seconds: f64) -> (Feed, Capture) {
    let frames = (rate as f64 * seconds).ceil() as usize;
    let (chunk_tx, chunk_rx) = rtrb::RingBuffer::new(8192);
    let (sample_tx, sample_rx) = rtrb::RingBuffer::new(frames * channels);
    let lost = Arc::new(AtomicU64::new(0));
    (
        Feed {
            chunks: chunk_tx,
            samples: sample_tx,
            channels,
            lost: lost.clone(),
        },
        Capture {
            chunks: chunk_rx,
            samples: sample_rx,
            channels,
            lost,
        },
    )
}

/// What a tap in the player records.
#[derive(Clone, Debug, PartialEq)]
pub enum TapSource {
    /// A track's or a group's stem: its sound after its fader, as the
    /// render writes it.
    Stem { kind: StemKind, id: String },
    /// The mix after the master's effects and the end fade, before the
    /// metronome.
    Master,
}

/// A feed inside the player, filled from a stem or the mix as blocks are
/// rendered while the transport rolls.
pub struct Tap {
    pub source: TapSource,
    pub feed: Feed,
}

/// An open input stream feeding a capture: the device's frames, the
/// channels wanted picked out, headed by their capture instants.
pub struct InputStream {
    _stream: cpal::Stream,
    pub device: String,
    /// The rate the device runs at, which the frames are at.
    pub rate: u32,
    /// How many channels each frame of the capture has.
    pub channels: usize,
    /// The device's reported input latency, in its own frames: what the
    /// sound took to reach the frames from the air.
    pub latency_frames: u32,
    dead: Arc<AtomicBool>,
    callbacks: Arc<AtomicU64>,
}

impl InputStream {
    /// Whether the driver has stopped the stream.
    pub fn dead(&self) -> bool {
        self.dead.load(Relaxed)
    }

    /// How many blocks the device has given.
    pub fn callbacks(&self) -> u64 {
        self.callbacks.load(Relaxed)
    }
}

/// Opens the input device whose name contains `name`, or the Mac's default
/// input with None, at `rate` when it offers it and else at its own rate,
/// taking the channels numbered in `channels` from 1. The capture holds
/// `seconds` of audio undrained.
pub fn open_input(name: Option<&str>, channels: &[i64], rate: u32, seconds: f64) -> Result<(InputStream, Capture), String> {
    let text = |e: cpal::Error| e.to_string();
    crate::devices::watch();
    let host = cpal::default_host();
    let device = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(name) => crate::devices::find_input(name)?.ok_or_else(|| format!("No audio input device named {name}; daw audio-device list names them"))?,
        None => host.default_input_device().ok_or("No audio input device")?,
    };
    let device_name = device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "input".into());
    let wanted: Vec<usize> = channels.iter().map(|c| (*c).max(1) as usize - 1).collect();
    let least = wanted.iter().map(|c| c + 1).max().unwrap_or(1) as u16;
    let configs: Vec<cpal::SupportedStreamConfigRange> = device
        .supported_input_configs()
        .map_err(text)?
        .filter(|c| c.sample_format() == cpal::SampleFormat::F32 && c.channels() >= least)
        .collect();
    if configs.is_empty() {
        let has = device.supported_input_configs().map(|c| c.map(|c| c.channels()).max().unwrap_or(0)).unwrap_or(0);
        return Err(format!("{device_name} has {has} input channel{}, not channel {least}", if has == 1 { "" } else { "s" }));
    }
    let offering = |at: u32| {
        configs
            .iter()
            .filter(|c| c.min_sample_rate() <= at && at <= c.max_sample_rate())
            .min_by_key(|c| c.channels())
            .cloned()
    };
    let (device_rate, range) = match offering(rate) {
        Some(range) => (rate, range),
        None => {
            let now = device.default_input_config().map_err(text)?.sample_rate();
            let range = offering(now).ok_or_else(|| format!("{device_name} cannot record at {rate} Hz, nor at its own {now} Hz"))?;
            (now, range)
        }
    };
    let stride = range.channels() as usize;
    let config = cpal::StreamConfig {
        channels: range.channels(),
        sample_rate: device_rate,
        buffer_size: cpal::BufferSize::Default,
    };
    let (mut feed, capture) = feed(wanted.len(), device_rate, seconds);
    let dead = Arc::new(AtomicBool::new(false));
    let callbacks = Arc::new(AtomicU64::new(0));
    let (dead_in_error, counted) = (dead.clone(), callbacks.clone());
    let stream = device
        .build_input_stream::<f32, _, _>(
            config,
            move |data: &[f32], info| {
                // The instant the first frame was captured, on the clock the
                // output's playback instants are on.
                let at = instant_ns(info.timestamp().callback);
                feed.push_picked(ChunkTime::Ns(at), data, stride, &wanted);
                counted.fetch_add(1, Relaxed);
            },
            move |e: cpal::Error| {
                if e.kind() != cpal::ErrorKind::Xrun {
                    dead_in_error.store(true, Relaxed);
                }
            },
            None,
        )
        .map_err(text)?;
    stream.play().map_err(text)?;
    let latency_frames = crate::devices::latency_frames(&device_name, true).unwrap_or(0);
    Ok((
        InputStream {
            _stream: stream,
            device: device_name,
            rate: device_rate,
            channels: capture.channels,
            latency_frames,
            dead,
            callbacks,
        },
        capture,
    ))
}

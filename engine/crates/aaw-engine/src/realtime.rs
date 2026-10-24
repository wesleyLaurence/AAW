//! Real-time playback through CoreAudio (via cpal). The callback runs the
//! transport's player, which uses the same renderer as offline export. It never
//! allocates, locks, blocks or logs, and debug builds abort if it allocates.

use crate::player::{channel, Control, MAX_BLOCK};
use crate::program::Program;
use crate::render::Renderer;
use aaw_model::value::{dict, Value};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub struct PlayOptions {
    /// The first frame to play.
    pub from: usize,
    /// Stop after this many seconds of audio.
    pub seconds: Option<f64>,
    /// The requested hardware buffer, in frames.
    pub buffer: u32,
}

#[derive(Default)]
struct Stats {
    callbacks: AtomicU64,
    frames: AtomicU64,
    busy_total_ns: AtomicU64,
    busy_max_ns: AtomicU64,
    over_budget: AtomicU64,
    xruns: AtomicU64,
    errors: AtomicU64,
    /// The driver stopped the stream: the device went away or changed its
    /// rate under it. Nothing more will be heard from it.
    dead: AtomicBool,
}

fn ms(ns: f64) -> Value {
    Value::Float((ns / 1e4).round() / 100.0)
}

fn last_frame(program: &Program, opts: &PlayOptions) -> usize {
    let limit = opts
        .seconds
        .map_or(usize::MAX, |s| opts.from + (s * program.rate as f64).round() as usize);
    limit.min(program.total)
}

/// How the output is opened: which device, how large a buffer, and for
/// checking, the rate to open the device at whatever the song's is.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// Part of an output device's name, ignoring case; None is the system's
    /// default output, or the device `AAW_OUTPUT_DEVICE` names when that
    /// is set, as a scripted run that must not be heard sets it.
    pub output: Option<String>,
    /// The requested hardware buffer, in frames.
    pub buffer: u32,
    /// Open the device at this rate rather than the song's, as a device
    /// that offers only this rate would be opened; None picks as described
    /// at `Transport::open`.
    pub device_rate: Option<u32>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            output: None,
            buffer: 128,
            device_rate: None,
        }
    }
}

/// An open output stream running a player. The stream runs until dropped and
/// is silent while the transport is stopped.
pub struct Transport {
    pub control: Control,
    stream: cpal::Stream,
    pub device: String,
    /// The song's rate, which the player renders at.
    pub rate: u32,
    /// The device's rate: the song's unless the device could not take it.
    pub device_rate: u32,
    pub channels: usize,
    pub buffer: u32,
    stats: Arc<Stats>,
    opened: Instant,
    /// Whether opening it moved the device to another rate.
    moved_rate: bool,
}

impl Transport {
    /// Opens the output `settings` names with a fixed buffer: at the song's
    /// rate when the device offers it, so what is heard is the render bit
    /// for bit; otherwise at the rate the device runs at now, with the
    /// song resampled to it in the callback, as AirPods at 48 kHz play a
    /// song made at 44.1. A device that offers neither in stereo is refused
    /// with its name.
    pub fn open(program: Arc<Program>, settings: &Settings) -> Result<Transport, String> {
        let first = Self::open_once(program.clone(), settings)?;
        if first.running_soon() {
            return Ok(first);
        }
        // The driver pauses a stream born as the device changes its rate,
        // late from setting that rate; built again now that the device has
        // settled, it runs.
        drop(first);
        Self::open_once(program, settings)
    }

    /// Whether the stream has run a callback, waiting a moment for the
    /// first; false once the driver has stopped it. A stream that moved its
    /// device to another rate is stopped some 450 ms on as the change lands,
    /// after a callback or two or none, so it is watched that long: a play
    /// or a take's tap sent to it would go with it.
    fn running_soon(&self) -> bool {
        let started = Instant::now();
        let watch = Duration::from_millis(if self.moved_rate { 800 } else { 0 });
        while !self.stats.dead.load(Relaxed)
            && (self.stats.callbacks.load(Relaxed) == 0 || started.elapsed() < watch)
            && started.elapsed() < Duration::from_millis(600).max(watch)
        {
            std::thread::sleep(Duration::from_millis(5));
        }
        self.stats.callbacks.load(Relaxed) > 0 && !self.stats.dead.load(Relaxed)
    }

    /// Whether the driver has stopped the stream: the device went away or
    /// changed its rate under it. The host opens the output again.
    pub fn dead(&self) -> bool {
        self.stats.dead.load(Relaxed)
    }

    fn open_once(program: Arc<Program>, settings: &Settings) -> Result<Transport, String> {
        let text = |e: cpal::Error| e.to_string();
        crate::devices::watch();
        let host = cpal::default_host();
        let wanted = settings
            .output
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or_else(|| std::env::var("AAW_OUTPUT_DEVICE").ok().filter(|n| !n.trim().is_empty()));
        let device = match wanted {
            Some(name) => crate::devices::find_output(&name)?
                .ok_or_else(|| format!("No audio output device named {}; daw device list names them", name.trim()))?,
            None => host.default_output_device().ok_or("No audio output device")?,
        };
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "output".into());
        let rate = program.rate;
        let configs: Vec<cpal::SupportedStreamConfigRange> = device
            .supported_output_configs()
            .map_err(text)?
            .filter(|c| c.sample_format() == cpal::SampleFormat::F32 && c.channels() >= 2)
            .collect();
        let offering = |at: u32| {
            configs
                .iter()
                .filter(|c| c.min_sample_rate() <= at && at <= c.max_sample_rate())
                .min_by_key(|c| c.channels())
                .cloned()
        };
        let (device_rate, range) = match settings.device_rate {
            Some(at) => (at, offering(at).ok_or_else(|| format!("{name} cannot play {at} Hz stereo audio"))?),
            None => match offering(rate) {
                Some(range) => (rate, range),
                None => {
                    let now = device.default_output_config().map_err(text)?.sample_rate();
                    let range = offering(now).ok_or_else(|| format!("{name} cannot play {rate} Hz stereo audio, nor its own {now} Hz"))?;
                    (now, range)
                }
            },
        };
        let moved_rate = device.default_output_config().map_or(true, |c| c.sample_rate() != device_rate);
        let buffer = settings.buffer;
        if let cpal::SupportedBufferSize::Range { min, max } = range.buffer_size() {
            if buffer < *min || buffer > *max {
                return Err(format!("{name} supports buffers of {min} to {max} frames"));
            }
        }
        let channels = range.channels() as usize;
        let config = cpal::StreamConfig {
            channels: range.channels(),
            sample_rate: device_rate,
            buffer_size: cpal::BufferSize::Fixed(buffer),
        };
        let stats = Arc::new(Stats::default());
        let (control, mut player) = channel(program);
        let shared = control.shared().clone();
        let mut block = vec![[0.0f64; 2]; MAX_BLOCK];
        let mut stream_resampler = (device_rate != rate).then(|| aaw_dsp::resample::Stream::new(rate, device_rate));
        let ns_per_frame = 1e9 / device_rate as f64;
        let callback_stats = stats.clone();
        let error_stats = stats.clone();
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |data: &mut [f32], info| {
                    assert_no_alloc::assert_no_alloc(|| {
                        let started = Instant::now();
                        let ahead = info.timestamp().playback.duration_since(info.timestamp().callback);
                        // What the output trails the renderer by, in the song's
                        // frames: the driver's lead, and what the resampler holds.
                        let held = stream_resampler.as_ref().map_or(0.0, |s| s.ahead());
                        shared
                            .output_latency
                            .store((ahead.as_secs_f64() * rate as f64 + held).round() as u64, Relaxed);
                        let frames = data.len() / channels;
                        // When the first frame written here plays, on the audio
                        // clock, for the anchor a recording maps its input by;
                        // what the resampler holds plays first, so the renderer's
                        // next frame plays that much later.
                        let plays = crate::capture::instant_ns(info.timestamp().callback) + (held / rate as f64 * 1e9) as u64;
                        let mut at = Some(plays);
                        let mut done = 0;
                        while done < frames {
                            let n = (frames - done).min(MAX_BLOCK);
                            match &mut stream_resampler {
                                None => player.render_at(&mut block[..n], at.take()),
                                Some(s) => s.run(&mut block[..n], &mut |b| player.render_at(b, at.take())),
                            }
                            for (k, f) in block[..n].iter().enumerate() {
                                let out = &mut data[(done + k) * channels..(done + k + 1) * channels];
                                out[0] = f[0] as f32;
                                out[1] = f[1] as f32;
                                out[2..].fill(0.0);
                            }
                            done += n;
                        }
                        let ns = started.elapsed().as_nanos() as u64;
                        let s = &callback_stats;
                        s.callbacks.fetch_add(1, Relaxed);
                        s.frames.fetch_add(frames as u64, Relaxed);
                        s.busy_total_ns.fetch_add(ns, Relaxed);
                        s.busy_max_ns.fetch_max(ns, Relaxed);
                        if ns as f64 > frames as f64 * ns_per_frame {
                            s.over_budget.fetch_add(1, Relaxed);
                        }
                    });
                },
                move |e: cpal::Error| {
                    if e.kind() == cpal::ErrorKind::Xrun {
                        error_stats.xruns.fetch_add(1, Relaxed);
                    } else {
                        error_stats.errors.fetch_add(1, Relaxed);
                        error_stats.dead.store(true, Relaxed);
                    }
                },
                None,
            )
            .map_err(text)?;
        stream.play().map_err(text)?;
        Ok(Transport {
            control,
            stream,
            device: name,
            rate,
            device_rate,
            channels,
            buffer,
            stats,
            opened: Instant::now(),
            moved_rate,
        })
    }

    /// Whether the song is resampled to the device's rate.
    pub fn resampled(&self) -> bool {
        self.device_rate != self.rate
    }

    /// Callback timing and dropouts since the stream opened.
    pub fn report(&self) -> Value {
        let s = &self.stats;
        let callbacks = s.callbacks.load(Relaxed).max(1) as f64;
        let frames = s.frames.load(Relaxed);
        let ns_per_frame = 1e9 / self.device_rate as f64;
        let shared = self.control.shared();
        dict(vec![
            ("device", Value::str(&self.device)),
            ("sample_rate", Value::int(self.rate as i64)),
            ("device_sample_rate", Value::int(self.device_rate as i64)),
            ("resampled", Value::Bool(self.resampled())),
            ("channels", Value::int(self.channels as i64)),
            ("buffer_frames", Value::int(self.buffer as i64)),
            ("output_latency_frames", Value::int(shared.output_latency.load(Relaxed) as i64)),
            (
                "played_seconds",
                Value::Float((shared.played.load(Relaxed) as f64 / self.rate as f64 * 1000.0).round() / 1000.0),
            ),
            ("wall_seconds", Value::Float((self.opened.elapsed().as_secs_f64() * 1000.0).round() / 1000.0)),
            ("callbacks", Value::int(s.callbacks.load(Relaxed) as i64)),
            ("mean_callback_frames", Value::Float((frames as f64 / callbacks).round())),
            ("budget_ms", ms(self.buffer as f64 * ns_per_frame)),
            ("mean_callback_ms", ms(s.busy_total_ns.load(Relaxed) as f64 / callbacks)),
            ("max_callback_ms", ms(s.busy_max_ns.load(Relaxed) as f64)),
            ("over_budget_callbacks", Value::int(s.over_budget.load(Relaxed) as i64)),
            ("xruns", Value::int(s.xruns.load(Relaxed) as i64)),
            ("stream_errors", Value::int(s.errors.load(Relaxed) as i64)),
            ("program_swaps", Value::int(shared.swaps.load(Relaxed) as i64)),
        ])
    }

    /// Pauses the device; dropping the transport also closes it.
    pub fn close(self) {
        drop(self.stream);
    }
}

/// Times the playback path without a device: the renderer in buffer-sized blocks
/// as fast as possible, against the time each block would have to play.
pub fn benchmark(program: Arc<Program>, opts: &PlayOptions) -> Value {
    let buffer = opts.buffer as usize;
    let mut renderer = Renderer::new(program.clone(), opts.from, buffer);
    let mut block = vec![[0.0f64; 2]; buffer];
    let end = last_frame(&program, opts);
    let budget = buffer as f64 * 1e9 / program.rate as f64;
    let mut times: Vec<f64> = Vec::with_capacity((end - opts.from.min(end)) / buffer + 1);
    let started = Instant::now();
    while renderer.position() < end {
        let t = Instant::now();
        renderer.render(&mut block, |_, _, _| {});
        times.push(t.elapsed().as_nanos() as f64);
    }
    let cpu = started.elapsed().as_secs_f64();
    let audio = times.len() as f64 * buffer as f64 / program.rate as f64;
    let mut sorted = times.clone();
    sorted.sort_by(f64::total_cmp);
    let pick = |q: f64| sorted.get(((sorted.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
    let mean = times.iter().sum::<f64>() / times.len().max(1) as f64;
    dict(vec![
        ("sample_rate", Value::int(program.rate as i64)),
        ("buffer_frames", Value::int(buffer as i64)),
        ("from_frame", Value::int(opts.from as i64)),
        ("blocks", Value::int(times.len() as i64)),
        ("audio_seconds", Value::Float((audio * 1000.0).round() / 1000.0)),
        ("realtime_factor", Value::Float((audio / cpu.max(1e-9)).round())),
        ("budget_ms", ms(budget)),
        ("mean_block_ms", ms(mean)),
        ("p99_block_ms", ms(pick(0.99))),
        ("max_block_ms", ms(pick(1.0))),
        ("over_budget_blocks", Value::int(times.iter().filter(|t| **t > budget).count() as i64)),
        ("latency_frames", Value::int(program.latency as i64)),
    ])
}

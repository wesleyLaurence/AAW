//! Real-time playback through CoreAudio (via cpal). The callback runs the
//! transport's player, which uses the same renderer as offline export. It never
//! allocates, locks, blocks or logs, and debug builds abort if it allocates.

use crate::player::{channel, Control, MAX_BLOCK};
use crate::program::Program;
use crate::render::Renderer;
use aaw_model::value::{dict, Value};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::Arc;
use std::time::Instant;

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

/// An open output stream running a player. The stream runs until dropped and
/// is silent while the transport is stopped.
pub struct Transport {
    pub control: Control,
    stream: cpal::Stream,
    pub device: String,
    pub rate: u32,
    pub channels: usize,
    pub buffer: u32,
    stats: Arc<Stats>,
    opened: Instant,
}

impl Transport {
    /// Opens the default output at the program's rate with a fixed buffer.
    pub fn open(program: Arc<Program>, buffer: u32) -> Result<Transport, String> {
        let text = |e: cpal::Error| e.to_string();
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("No audio output device")?;
        let name = device
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| "output".into());
        let rate = program.rate;
        let range = device
            .supported_output_configs()
            .map_err(text)?
            .filter(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && c.channels() >= 2
                    && c.min_sample_rate() <= rate
                    && rate <= c.max_sample_rate()
            })
            .min_by_key(|c| c.channels())
            .ok_or_else(|| format!("{name} cannot play {rate} Hz stereo audio"))?;
        if let cpal::SupportedBufferSize::Range { min, max } = range.buffer_size() {
            if buffer < *min || buffer > *max {
                return Err(format!("{name} supports buffers of {min} to {max} frames"));
            }
        }
        let channels = range.channels() as usize;
        let config = cpal::StreamConfig {
            channels: range.channels(),
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Fixed(buffer),
        };
        let stats = Arc::new(Stats::default());
        let (control, mut player) = channel(program);
        let mut block = vec![[0.0f64; 2]; MAX_BLOCK];
        let ns_per_frame = 1e9 / rate as f64;
        let callback_stats = stats.clone();
        let error_stats = stats.clone();
        let stream = device
            .build_output_stream::<f32, _, _>(
                config,
                move |data: &mut [f32], _info| {
                    assert_no_alloc::assert_no_alloc(|| {
                        let started = Instant::now();
                        let frames = data.len() / channels;
                        let mut done = 0;
                        while done < frames {
                            let n = (frames - done).min(MAX_BLOCK);
                            player.render(&mut block[..n]);
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
            channels,
            buffer,
            stats,
            opened: Instant::now(),
        })
    }

    /// Callback timing and dropouts since the stream opened.
    pub fn report(&self) -> Value {
        let s = &self.stats;
        let callbacks = s.callbacks.load(Relaxed).max(1) as f64;
        let frames = s.frames.load(Relaxed);
        let ns_per_frame = 1e9 / self.rate as f64;
        let shared = self.control.shared();
        dict(vec![
            ("device", Value::str(&self.device)),
            ("sample_rate", Value::int(self.rate as i64)),
            ("channels", Value::int(self.channels as i64)),
            ("buffer_frames", Value::int(self.buffer as i64)),
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
        renderer.process(&mut block, |_, _| {});
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
        ("omitted", Value::List(program.omitted.iter().map(|s| Value::str(s)).collect())),
    ])
}

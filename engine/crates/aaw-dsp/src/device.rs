//! An effect built for processing: the device its spec and lanes call for.
//!
//! A constant lane becomes the spec's static value, so the device renders
//! exactly as the static spec would; only moving lanes take the automated path.

use crate::biquad::{butter, sections, Cascade};
use crate::chorus::Chorus;
use crate::clipper::Clipper;
use crate::delay::Delay;
use crate::dynamics::{Compressor, CompressorSettings, Limiter, Reductions};
use crate::envelope::{Envelope, Knob, Param};
use crate::meter::Ring;
use crate::reverb::{Kernel, Reverb, Shape};
use crate::saturation::Saturation;
use crate::spectrum::Tap;
use crate::svf::{self, Svf};
use crate::utility::Utility;
use crate::{Clock, Frame};
use aaw_model::{frame, Effect, FilterMode};
use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// Reverb kernels kept between compiles. Making one takes far longer than
/// anything else in a compile, and an edit elsewhere leaves it as it was.
#[derive(Default)]
pub struct Kernels {
    generation: u64,
    kernels: HashMap<[u64; 7], (u64, Arc<Kernel>)>,
}

impl Kernels {
    /// Starts a compile; kernels it does not ask for are dropped at `finish`.
    pub fn begin(&mut self) {
        self.generation += 1;
    }

    pub fn get(&mut self, shape: Shape) -> Arc<Kernel> {
        let generation = self.generation;
        let entry = self
            .kernels
            .entry(shape.key())
            .or_insert_with(|| (generation, Arc::new(Kernel::new(shape))));
        entry.0 = generation;
        entry.1.clone()
    }

    pub fn finish(&mut self) {
        let generation = self.generation;
        self.kernels.retain(|_, e| e.0 == generation);
    }
}

fn hash(parts: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    parts.hash(&mut h);
    h.finish()
}

/// An effect as compiled: its spec, the envelopes of its lanes by parameter
/// name, such as `cutoff_hz` or `bands.0.gain_db`, and a reverb's kernel.
/// Devices are built from it, each with state of its own.
#[derive(Clone, Debug)]
pub struct Plan {
    pub effect: Effect,
    pub lanes: BTreeMap<String, Arc<Envelope>>,
    kernel: Option<Arc<Kernel>>,
    rate: i64,
    tempo: f64,
    /// Frames by which the device delays its input.
    pub latency: usize,
    /// What decides whether another device's state fits this one: the type
    /// and whatever shapes the state. Levels and knobs are not part of it.
    pub signature: u64,
    /// Where an equalizer's output is tapped for the app's spectrum, when
    /// something wants to draw it.
    pub tap: Option<Arc<Tap>>,
    /// Where an analyzer's output goes, every frame, for the app's meters.
    pub ring: Option<Arc<Ring>>,
}

impl Plan {
    pub fn new(effect: &Effect, lanes: BTreeMap<String, Arc<Envelope>>, rate: i64, tempo: f64, kernels: &mut Kernels) -> Plan {
        let moves = |name: &str| lanes.get(name).is_some_and(|e| e.constant().is_none());
        let mut kernel = None;
        let mut latency = 0;
        let signature = match effect {
            Effect::Filter(f) => hash(("filter", moves("cutoff_hz"), f.mode == FilterMode::Highpass, f.slope_db_per_octave)),
            Effect::Eq(e) => {
                let moving = (0..e.bands.len()).any(|j| ["freq_hz", "gain_db", "q"].iter().any(|f| moves(&format!("bands.{j}.{f}"))));
                hash(("eq", moving, e.bands.iter().map(|b| (b.shape, b.sections())).collect::<Vec<_>>()))
            }
            Effect::Compressor(_) => hash(("compressor", effect.sidechain().is_some())),
            Effect::Limiter(l) => {
                latency = Limiter::latency(l.lookahead_ms, rate as f64, l.true_peak);
                hash(("limiter", latency, l.true_peak))
            }
            Effect::Clipper(c) => {
                latency = Clipper::latency(c.oversample);
                hash(("clipper", c.oversample))
            }
            Effect::Delay(d) => {
                let cut = |hz: Option<f64>| hz.is_some_and(|hz| hz != 0.0);
                hash(("delay", delay_frames(d, tempo, rate), cut(d.lowcut_hz), cut(d.highcut_hz), d.ping_pong))
            }
            Effect::Reverb(r) => {
                let shape = Shape {
                    decay_seconds: r.decay_seconds,
                    predelay_ms: r.predelay_ms,
                    damping_hz: r.damping_hz,
                    lowcut_hz: r.lowcut_hz,
                    width_percent: r.width_percent,
                    seed: r.seed as u64,
                    rate: rate as u32,
                };
                kernel = Some(kernels.get(shape));
                hash(("reverb", shape.key()))
            }
            Effect::Chorus(_) => hash("chorus"),
            Effect::Saturation(s) => hash(("saturation", s.mode)),
            // A polarity flip or a sum to mono is a jump, so a change of either
            // fades through rather than gliding.
            Effect::Utility(u) => hash(("utility", u.invert, u.mono, u.mono_below_hz.map(f64::to_bits))),
            Effect::Analyzer(_) => hash("analyzer"),
        };
        Plan {
            effect: effect.clone(),
            lanes,
            kernel,
            rate,
            tempo,
            latency,
            signature,
            tap: None,
            ring: None,
        }
    }
}

fn delay_frames(d: &aaw_model::Delay, tempo: f64, rate: i64) -> usize {
    frame(&d.time_exact(), tempo, rate).max(1) as usize
}

pub enum Device {
    /// A filter or equalizer with fixed coefficients.
    Sections(Cascade),
    /// A filter or equalizer that a lane moves.
    Moving(Svf),
    Compressor(Compressor),
    Limiter(Limiter),
    Clipper(Clipper),
    Delay(Delay),
    Reverb(Reverb),
    Chorus(Chorus),
    Saturation(Saturation),
    Utility(Utility),
    /// An analyzer, which changes nothing: what it shows is read from the
    /// unit's ring.
    Analyzer,
}

/// A device in a chain.
pub struct Unit {
    pub device: Device,
    pub latency: usize,
    pub signature: u64,
    /// The ring the device's output is written to for the app's spectrum.
    tap: Option<Arc<Tap>>,
    /// The ring an analyzer's output is written to for the app's meters.
    ring: Option<Arc<Ring>>,
}

impl Unit {
    /// The device for an effect that is not bypassed. Blocks are at most
    /// `max_block` frames; after a live edit, values glide over `glide` frames.
    /// `stagger` numbers the reverbs of one renderer, which spread their work.
    pub fn new(plan: &Plan, max_block: usize, glide: usize, stagger: usize) -> Unit {
        let (effect, lanes) = (&plan.effect, &plan.lanes);
        let rate = plan.rate as f64;
        let param = |name: &str, value: f64| Param::new(value, lanes.get(name));
        let knob = |name: &str, value: f64| Knob::new(Param::new(value, lanes.get(name)), glide);
        let device = match effect {
            Effect::Filter(f) => {
                let highpass = f.mode == FilterMode::Highpass;
                let order = (f.slope_db_per_octave / 6) as usize;
                let cutoff = param("cutoff_hz", f.cutoff_hz);
                if cutoff.moves() {
                    Device::Moving(Svf::new(svf::Shape::Filter { highpass, order }, vec![cutoff], rate))
                } else {
                    Device::Sections(Cascade::with_glide(butter(order, cutoff.value(), highpass, rate), glide))
                }
            }
            Effect::Eq(e) => {
                let params: Vec<Param> = e
                    .bands
                    .iter()
                    .enumerate()
                    .flat_map(|(j, band)| {
                        [("freq_hz", band.freq_hz), ("gain_db", band.gain_db), ("q", band.q)]
                            .map(|(field, value)| param(&format!("bands.{j}.{field}"), value))
                    })
                    .collect();
                if params.iter().any(Param::moves) {
                    let bands = e.bands.iter().map(|band| (band.shape, band.sections())).collect();
                    Device::Moving(Svf::new(svf::Shape::Eq { bands }, params, rate))
                } else {
                    let sections = e
                        .bands
                        .iter()
                        .zip(params.chunks(3))
                        .flat_map(|(spec, p)| {
                            let mut still = spec.clone();
                            (still.freq_hz, still.gain_db, still.q) = (p[0].value(), p[1].value(), p[2].value());
                            sections(&still, rate)
                        })
                        .collect();
                    Device::Sections(Cascade::with_glide(sections, glide))
                }
            }
            Effect::Compressor(c) => {
                let settings = CompressorSettings {
                    ratio: c.ratio,
                    attack_ms: c.attack_ms,
                    release_ms: c.release_ms,
                    knee_db: c.knee_db,
                };
                Device::Compressor(Compressor::new(
                    settings,
                    knob("threshold_db", c.threshold_db),
                    knob("makeup_db", c.makeup_db),
                    rate,
                    max_block,
                ))
            }
            Effect::Limiter(l) => Device::Limiter(Limiter::new(l.ceiling_db, l.release_ms, l.lookahead_ms, l.true_peak, rate)),
            Effect::Clipper(c) => Device::Clipper(Clipper::new(
                c.oversample,
                knob("ceiling_db", c.ceiling_db),
                knob("drive_db", c.drive_db),
                knob("knee_db", c.knee_db),
                max_block,
            )),
            Effect::Delay(d) => Device::Delay(Delay::new(
                delay_frames(d, plan.tempo, plan.rate),
                d.lowcut_hz,
                d.highcut_hz,
                d.ping_pong,
                knob("feedback_percent", d.feedback_percent),
                knob("mix_percent", d.mix_percent),
                rate,
                max_block,
                glide,
            )),
            Effect::Reverb(r) => {
                let kernel = plan.kernel.clone().expect("a reverb's plan has its kernel");
                Device::Reverb(Reverb::new(kernel, knob("mix_percent", r.mix_percent), max_block, stagger * 37 % 256))
            }
            Effect::Chorus(c) => Device::Chorus(Chorus::new(
                knob("rate_hz", c.rate_hz),
                knob("depth_ms", c.depth_ms),
                knob("delay_ms", c.delay_ms),
                knob("mix_percent", c.mix_percent),
                rate,
                max_block,
            )),
            Effect::Saturation(s) => Device::Saturation(Saturation::new(
                s.mode,
                knob("drive_db", s.drive_db),
                knob("output_db", s.output_db),
                knob("mix_percent", s.mix_percent),
                rate,
                max_block,
            )),
            Effect::Utility(u) => Device::Utility(Utility::new(
                u.invert,
                u.mono,
                u.mono_below_hz,
                knob("gain_db", u.gain_db),
                knob("pan", u.pan),
                knob("width_percent", u.width_percent),
                rate,
                max_block,
            )),
            Effect::Analyzer(_) => Device::Analyzer,
        };
        Unit {
            device,
            latency: plan.latency,
            signature: plan.signature,
            tap: plan.tap.clone(),
            ring: plan.ring.clone(),
        }
    }

    /// Processes a block in place. `clock` is where the block's input is on
    /// the timeline; `key` is a sidechain's audio for the same frames.
    pub fn process(&mut self, x: &mut [Frame], key: Option<&[Frame]>, clock: Clock) {
        match &mut self.device {
            Device::Sections(d) => d.process(x),
            Device::Moving(d) => d.process(x, clock),
            Device::Compressor(d) => d.process(x, key, clock),
            Device::Limiter(d) => d.process(x, clock),
            Device::Clipper(d) => d.process(x, clock),
            Device::Delay(d) => d.process(x, clock),
            Device::Reverb(d) => d.process(x, clock),
            Device::Chorus(d) => d.process(x, clock),
            Device::Saturation(d) => d.process(x, clock),
            Device::Utility(d) => d.process(x, clock),
            Device::Analyzer => {}
        }
        if let Some(tap) = &self.tap {
            tap.write(x);
        }
        if let Some(ring) = &self.ring {
            ring.write(x);
        }
    }

    /// Continues from the state of a unit with the same signature, which a
    /// program being replaced was playing: tails ring on, and levels and knobs
    /// glide from where they were. `old` is left with state to be freed off
    /// the audio thread. Does not allocate.
    pub fn take_over(&mut self, old: &mut Unit) {
        match (&mut self.device, &mut old.device) {
            (Device::Sections(new), Device::Sections(was)) => new.take_over(was),
            (Device::Moving(new), Device::Moving(was)) => new.take_over(was),
            (Device::Compressor(new), Device::Compressor(was)) => new.take_over(was),
            (Device::Limiter(new), Device::Limiter(was)) => new.take_over(was),
            (Device::Clipper(new), Device::Clipper(was)) => new.take_over(was),
            (Device::Delay(new), Device::Delay(was)) => new.take_over(was),
            (Device::Reverb(new), Device::Reverb(was)) => new.take_over(was),
            (Device::Chorus(new), Device::Chorus(was)) => new.take_over(was),
            (Device::Saturation(new), Device::Saturation(was)) => new.take_over(was),
            (Device::Utility(new), Device::Utility(was)) => new.take_over(was),
            _ => {}
        }
    }

    /// Gain reduction so far, for a compressor, a limiter or a clipper.
    pub fn reduction(&self) -> Option<&Reductions> {
        match &self.device {
            Device::Compressor(d) => Some(&d.reduction),
            Device::Limiter(d) => Some(&d.reduction),
            Device::Clipper(d) => Some(&d.reduction),
            _ => None,
        }
    }

    /// Has a compressor, a limiter or a clipper measure its reduction between these
    /// timeline frames apart, for a render's sections.
    pub fn split_reduction(&mut self, edges: &[i64]) {
        match &mut self.device {
            Device::Compressor(d) => d.reduction.split(edges),
            Device::Limiter(d) => d.reduction.split(edges),
            Device::Clipper(d) => d.reduction.split(edges),
            _ => {}
        }
    }
}

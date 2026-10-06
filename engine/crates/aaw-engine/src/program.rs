//! A song compiled for playback: every voice's prepared audio and parameters,
//! each channel's effect chain and levels with their automation, the routing
//! between them and the delays that align their latencies. Compiled off the
//! audio thread; a `Renderer` plays it.
//!
//! Latency. Only the limiter delays its input, by its look-ahead. Every other
//! path is delayed to match:
//! a track keyed by a source with latency renders its voices that much later,
//! tracks are delayed to the slowest track before their faders and sends,
//! returns to the slowest return, and the output trails a voice by `latency`
//! frames in all.

use crate::sndfile;
use crate::stretch::{self, MARGIN_SECONDS};
use aaw_dsp::device::{Kernels, Plan};
use aaw_dsp::envelope::{Envelope, Param};
use aaw_dsp::resample::{repitch_ratio, resample_poly};
use aaw_dsp::synth::{track_seed, Patch};
use aaw_dsp::wavetable::Wavetable;
use aaw_model::rules::{midi, target, Owner, TargetKind, MIDDLE_C};
use aaw_model::schedule::{track_notes, track_triggers, NoteOn, Trigger};
use aaw_model::{frame, AudioClip, Effect, Event, FadeCurve, Lane, Pad, Project, Stretch, Stretcher};
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

/// A pad's source after trim, reverse, downmix and repitch: interleaved frames.
#[derive(Debug)]
pub struct Prepared {
    pub channels: usize,
    pub data: Vec<f64>,
}

impl Prepared {
    pub fn frames(&self) -> usize {
        self.data.len() / self.channels
    }
}

/// One hit: a prepared buffer and the gains and envelope it plays with.
#[derive(Clone, Debug)]
pub struct Voice {
    pub start: i64,
    pub audio: Arc<Prepared>,
    /// `stereo_pan` gains for the pad: equal-power for mono, balance for stereo.
    pub pan: [f64; 2],
    pub gain: f64,
    pub velocity: f64,
    pub attack: usize,
    pub release: usize,
    pub length: usize,
    /// The shape of its fades: a line for a pad, a clip's own for an audio clip.
    pub curve: FadeCurve,
}

/// A fade's level when it is `x` of the way from silence to full.
#[inline]
fn shape(curve: FadeCurve, x: f64) -> f64 {
    match curve {
        FadeCurve::Linear => x,
        FadeCurve::EqualPower => (x * PI / 2.0).sin(),
    }
}

impl Voice {
    /// The voice's output at its frame `i` for channel `c`, as `Voice.process`.
    #[inline]
    pub fn sample(&self, i: usize, c: usize) -> f64 {
        let p = if self.audio.channels == 1 {
            self.audio.data[i]
        } else {
            self.audio.data[i * 2 + c]
        };
        (p * self.pan[c] * self.gain * self.velocity) * self.envelope(i)
    }

    #[inline]
    fn envelope(&self, i: usize) -> f64 {
        let mut env = 1.0;
        if self.attack != 0 {
            env *= shape(self.curve, (i as f64 / self.attack as f64).min(1.0));
        }
        // Every natural sample ending fades. A gate releases at the note-off frame.
        let natural = self.release.min(self.length);
        if natural != 0 {
            let remaining = (self.length as f64 - 1.0 - i as f64) / natural.max(1) as f64;
            env *= shape(self.curve, remaining.max(0.0).min(1.0));
        }
        env
    }
}

/// `stereo_pan`'s gains for a pan position.
pub fn pan_gains(channels: usize, pan: f64) -> [f64; 2] {
    if channels == 1 {
        let angle = (pan + 1.0) * PI / 4.0;
        [angle.cos(), angle.sin()]
    } else {
        [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)]
    }
}

/// `10 ** (db / 20)`.
pub fn amplitude(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// The sidechain of a device: the track whose audio keys it, by its place
/// among the program's tracks, and the delay that lines the key up with the
/// audio reaching the device.
#[derive(Debug)]
pub struct Key {
    pub source: usize,
    pub delay: usize,
}

#[derive(Debug)]
pub struct DeviceProgram {
    pub plan: Plan,
    /// The latency of the devices before this one in its chain.
    pub upstream: usize,
    pub key: Option<Key>,
}

/// A channel's inserts. Bypassed effects are not processed.
#[derive(Debug, Default)]
pub struct ChainProgram {
    pub devices: Vec<DeviceProgram>,
    /// For each effect of the document, its type and its place among
    /// `devices`, or None when it is bypassed.
    pub effects: Vec<(&'static str, Option<usize>)>,
    pub latency: usize,
}

/// A track's send to a return. A track has one for every return, so that a
/// send added or removed during playback is a level that fades.
#[derive(Debug)]
pub struct SendProgram {
    pub pre_fader: bool,
    /// The level in dB; None when the track does not send to the return.
    pub level: Option<Param>,
}

/// A MIDI track's Synth: the notes it plays, shared with every program
/// compiled while the track's clips stay as they are, its patch as the
/// synth plays it, and the patch's own effects, run on the sum of the
/// voices before the track's inserts.
#[derive(Debug)]
pub struct SynthProgram {
    pub notes: Arc<Vec<NoteOn>>,
    pub patch: Arc<Patch>,
    pub chain: ChainProgram,
}

#[derive(Debug)]
pub struct TrackProgram {
    pub id: String,
    /// Voices in trigger order, shared with every program compiled while the
    /// track's clips, patterns and pads stay as they are. A track with a
    /// Synth has none; its notes are the synth's.
    pub voices: Arc<Vec<Voice>>,
    pub synth: Option<SynthProgram>,
    /// Equal for tracks whose voices are the same hits of the same audio
    /// within the same session length, so that what was worked out from
    /// them, such as waveform peaks, can be kept.
    pub voices_id: u64,
    pub chain: ChainProgram,
    /// Level in dB and pan, static or automated.
    pub gain: Param,
    pub pan: Param,
    /// Muted, or solo-muted.
    pub silent: bool,
    /// A send for each of the program's returns.
    pub sends: Vec<SendProgram>,
    /// Frames by which the track's voices trail the transport, so that its
    /// sidechain keys have come through their own chains.
    pub delay: usize,
    /// Frames the track's inserts' output is delayed to line up with the
    /// slowest track.
    pub align: usize,
    /// Whether the track is heard and has a stem. A preview's helper tracks,
    /// which only key or feed the target, are not.
    pub in_mix: bool,
    /// The parameters of the track's lanes.
    pub automation: Vec<String>,
}

#[derive(Debug)]
pub struct ReturnProgram {
    pub id: String,
    pub chain: ChainProgram,
    pub gain: Param,
    pub pan: Param,
    pub mute: bool,
    /// Frames the return's output is delayed to line up with the slowest return.
    pub align: usize,
    pub senders: Vec<String>,
    pub automation: Vec<String>,
}

#[derive(Debug)]
pub struct MasterProgram {
    /// The master level in dB: the session's, or a lane's.
    pub gain: Param,
    pub chain: ChainProgram,
    pub automation: Vec<String>,
}

#[derive(Debug)]
pub struct Program {
    pub rate: u32,
    /// Beats per minute, to carry a playhead across a tempo change.
    pub tempo: f64,
    /// The session length in frames.
    pub total: usize,
    /// Tracks in render order: sidechain sources first.
    pub tracks: Vec<TrackProgram>,
    pub returns: Vec<ReturnProgram>,
    pub master: MasterProgram,
    pub fade: usize,
    /// Frames by which the slowest track's inserts trail the transport: the
    /// timeline the faders, sends and return inputs share.
    pub track_offset: usize,
    /// The slowest return's latency.
    pub return_latency: usize,
    /// Frames by which the output trails the transport.
    pub latency: usize,
    /// Equal for programs whose channels, devices and delays line up one to
    /// one, so that one can take over from the other without a break.
    pub structure: u64,
}

impl Program {
    /// The end fade at a frame: `np.linspace(1, 0, fade)` over the last frames.
    #[inline]
    pub fn fader(&self, f: usize) -> f64 {
        let tail = self.total - self.fade;
        if f < tail {
            return 1.0;
        }
        let i = f - tail;
        if self.fade == 1 {
            1.0
        } else if i == self.fade - 1 {
            0.0
        } else {
            i as f64 * (-1.0 / (self.fade - 1) as f64) + 1.0
        }
    }

    /// The master gain and end fade at a frame.
    pub fn envelope(&self, f: usize) -> f64 {
        self.fader(f) * amplitude(self.master.gain.at(f as i64))
    }

    /// Where each stem is: tracks in render order, then returns. The name, and
    /// whether it is a return.
    pub fn stems(&self) -> Vec<(&str, bool)> {
        let tracks = self.tracks.iter().filter(|t| t.in_mix).map(|t| (t.id.as_str(), false));
        tracks.chain(self.returns.iter().map(|r| (r.id.as_str(), true))).collect()
    }
}

/// A sample file's identity on disk: a replaced file is decoded again.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PrepKey {
    path: PathBuf,
    stamp: Stamp,
    start: u64,
    end: Option<u64>,
    reverse: bool,
    mono: bool,
    speed: u64,
    /// How many times as fast it plays at its own pitch, as bits; 1 for none.
    stretch: u64,
    stretcher: Stretcher,
    rate: i64,
}

/// A track's voices as last compiled, with what they were compiled from.
struct TrackEntry {
    generation: u64,
    key: String,
    voices: Arc<Vec<Voice>>,
    prepared: Vec<PrepKey>,
    originals: Vec<PathBuf>,
}

/// What is kept between compiles, so recompiling after an edit redoes only
/// what the edit changed: decoded and prepared pad audio, each track's voices
/// and reverb kernels. Entries a compile does not use are dropped.
#[derive(Default)]
pub struct Cache {
    generation: u64,
    original: HashMap<PathBuf, (u64, Stamp, Arc<sndfile::Audio>)>,
    prepared: HashMap<PrepKey, (u64, Arc<Prepared>)>,
    tracks: HashMap<String, TrackEntry>,
    /// Each Synth track's notes, with what they were made from, so that a
    /// patch edit leaves them as they are and a playing synth carries on.
    notes: HashMap<String, (u64, String, Arc<Vec<NoteOn>>)>,
    /// Wavetables read from samples, by the file and its stamp.
    tables: HashMap<(PathBuf, Stamp), (u64, Arc<Wavetable>)>,
    kernels: Kernels,
}

impl Cache {
    fn begin(&mut self) {
        self.generation += 1;
        self.kernels.begin();
    }

    fn finish(&mut self) {
        let g = self.generation;
        self.original.retain(|_, e| e.0 == g);
        self.prepared.retain(|_, e| e.0 == g);
        self.tracks.retain(|_, e| e.generation == g);
        self.notes.retain(|_, e| e.0 == g);
        self.tables.retain(|_, e| e.0 == g);
        self.kernels.finish();
    }

    /// The wavetable of a sample's file, one cycle, from the cache when the
    /// file is as it was.
    fn wavetable(&mut self, p: &Project, directory: &Path, sample: &str) -> Result<Arc<Wavetable>, String> {
        let asset = p.samples.get(sample).ok_or_else(|| format!("unknown sample {sample}"))?;
        let path = directory.join(&asset.path);
        let stamp = stamp(&path)?;
        let g = self.generation;
        if let Some(entry) = self.tables.get_mut(&(path.clone(), stamp.clone())) {
            entry.0 = g;
            return Ok(entry.1.clone());
        }
        let audio = sndfile::read(&path)?;
        let key = format!("{}@{}:{:?}", sample, stamp.len, stamp.modified);
        let table = Arc::new(Wavetable::from_cycle(&key, &audio.data, audio.channels).map_err(|e| format!("{sample}: {e}"))?);
        self.tables.insert((path, stamp), (g, table.clone()));
        Ok(table)
    }

    /// A Synth track's notes, from the cache when its clips, the tempo and
    /// the rate are as they were.
    fn synth_notes(&mut self, p: &Project, t: &aaw_model::Track, midi: &aaw_model::Midi) -> Arc<Vec<NoteOn>> {
        let key = format!("{:?}|{}|{}", midi.clips, p.session.tempo.to_bits(), p.session.sample_rate);
        let g = self.generation;
        if let Some(entry) = self.notes.get_mut(&t.id) {
            if entry.1 == key {
                entry.0 = g;
                return entry.2.clone();
            }
        }
        let notes = Arc::new(track_notes(p, midi));
        self.notes.insert(t.id.clone(), (g, key, notes.clone()));
        notes
    }
}

fn stamp(path: &Path) -> Result<Stamp, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Stamp {
        len: meta.len(),
        modified: meta.modified().ok(),
    })
}

/// A pad's audio to prepare: the decoded file and what to do with it.
struct Job {
    key: PrepKey,
    audio: Arc<sndfile::Audio>,
    sample: String,
}

impl Job {
    /// Trims, reverses, downmixes, stretches and repitches, as `Sampler.prepare`
    /// did all but the stretching.
    fn make(&self) -> Result<Prepared, String> {
        let (x, k) = (&self.audio, &self.key);
        let frames = x.frames();
        let source_rate = x.rate as f64;
        let start = (f64::from_bits(k.start) * source_rate).round_ties_even() as usize;
        let end = match k.end {
            Some(e) => (f64::from_bits(e) * source_rate).round_ties_even() as usize,
            None => frames,
        };
        if start >= frames || end > frames {
            return Err(format!("{}: trim outside sample", self.sample));
        }
        let ch = x.channels;
        let stretch = f64::from_bits(k.stretch);
        // A stretched region is stretched with some of the file either side of
        // it, which is then cut off, so what the stretcher does at its own start
        // and end is not in the region.
        let margin = if stretch == 1.0 { 0 } else { (MARGIN_SECONDS * source_rate) as usize };
        let (from, to) = (start.saturating_sub(margin), (end.max(start) + margin).min(frames));
        let rows: Vec<&[f64]> = (from..to).map(|i| &x.data[i * ch..(i + 1) * ch]).collect();
        let (channels, mut data): (usize, Vec<f64>) = if k.mono {
            let mean = |r: &[f64]| r.iter().fold(0.0, |s, v| s + v) / ch as f64;
            (1, rows.iter().map(|r| mean(r)).collect())
        } else {
            (ch, rows.concat())
        };
        if stretch != 1.0 {
            let whole = stretch::stretch(&data, channels, x.rate, stretch, k.stretcher)
                .map_err(|e| format!("{}: {e}", self.sample))?;
            let first = ((start - from) as f64 / stretch).round() as usize;
            let count = ((end.max(start) - start) as f64 / stretch).round() as usize;
            let last = (first + count).min(whole.len() / channels);
            data = whole[first.min(last) * channels..last * channels].to_vec();
        }
        if k.reverse {
            data = data.chunks(channels).rev().flatten().copied().collect();
        }
        if let Some((up, down)) = repitch_ratio(k.rate as u32, x.rate, f64::from_bits(k.speed)) {
            if !data.is_empty() {
                data = resample_poly(&data, channels, up, down);
            }
        }
        Ok(Prepared { channels, data })
    }
}

/// Prepares, side by side, the pad audio that tracks about to be compiled
/// need and the cache lacks. Repitching is most of a first compile. What
/// cannot be prepared is left for the compile to report.
fn prepare_ahead(p: &Project, directory: &Path, cache: &mut Cache, tracks: &[usize]) {
    let mut jobs: Vec<Job> = Vec::new();
    let mut seen: std::collections::HashSet<PrepKey> = std::collections::HashSet::new();
    let mut sampler = Sampler::new(p, directory, cache);
    'tracks: for &ti in tracks {
        let t = &p.tracks[ti];
        for tr in track_triggers(p, ti) {
            let Ok(job) = sampler.job(&t.sound_pads()[&tr.pad], &tr.event) else { break 'tracks };
            if !sampler.cache.prepared.contains_key(&job.key) && seen.insert(job.key.clone()) {
                jobs.push(job);
            }
        }
        for clip in &t.audio {
            let leaves = sampler.clip_leaves(&t.audio, clip);
            let Ok(job) = sampler.clip_job(clip, leaves) else { break 'tracks };
            if !sampler.cache.prepared.contains_key(&job.key) && seen.insert(job.key.clone()) {
                jobs.push(job);
            }
        }
    }
    if jobs.len() < 2 {
        return;
    }
    let next = std::sync::atomic::AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).min(jobs.len());
    let made: Vec<(usize, Prepared)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    let mut made = Vec::new();
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(job) = jobs.get(i) else { break };
                        if let Ok(prepared) = job.make() {
                            made.push((i, prepared));
                        }
                    }
                    made
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a sample preparation panicked")).collect()
    });
    let g = cache.generation;
    for (i, prepared) in made {
        cache.prepared.insert(jobs[i].key.clone(), (g, Arc::new(prepared)));
    }
}

/// Loads and prepares pad audio through a cache.
pub struct Sampler<'a> {
    project: &'a Project,
    directory: PathBuf,
    cache: &'a mut Cache,
    /// What the voices made so far were prepared from.
    prepared: Vec<PrepKey>,
    originals: Vec<PathBuf>,
}

impl<'a> Sampler<'a> {
    pub fn new(project: &'a Project, directory: &Path, cache: &'a mut Cache) -> Self {
        Sampler {
            project,
            directory: directory.to_path_buf(),
            cache,
            prepared: Vec::new(),
            originals: Vec::new(),
        }
    }

    fn original(&mut self, name: &str) -> Result<(PathBuf, Stamp, Arc<sndfile::Audio>), String> {
        let path = self.directory.join(&self.project.samples[name].path);
        let g = self.cache.generation;
        if let Some((used, stamp, audio)) = self.cache.original.get_mut(&path) {
            if *used == g {
                return Ok((path, stamp.clone(), audio.clone()));
            }
        }
        let stamp = stamp(&path)?;
        if let Some((used, known, audio)) = self.cache.original.get_mut(&path) {
            if *known == stamp {
                *used = g;
                return Ok((path, stamp, audio.clone()));
            }
        }
        let audio = sndfile::read(&path)?;
        if !(audio.channels == 1 || audio.channels == 2)
            || audio.frames() == 0
            || !audio.data.iter().all(|x| x.is_finite())
        {
            return Err(format!("{name}: expected finite mono/stereo audio"));
        }
        let audio = Arc::new(audio);
        self.cache.original.insert(path.clone(), (g, stamp.clone(), audio.clone()));
        Ok((path, stamp, audio))
    }

    /// What a pad's audio for an event is prepared from.
    fn job(&mut self, pad: &Pad, event: &Event) -> Result<Job, String> {
        let asset = &self.project.samples[&pad.sample];
        let mut semitones = pad.transpose + event.transpose;
        if let Some(note) = &event.note {
            // Only a pitched map entry plays a sample with no root note.
            let root = match asset.root_note.as_deref().filter(|r| !r.is_empty()) {
                Some(root) => midi(root)?,
                None => MIDDLE_C,
            };
            semitones += (midi(note)? - root) as f64;
        }
        let follow = (pad.source_bpm, pad.stretch);
        self.part(&pad.sample, pad.start_seconds, pad.end_seconds, pad.reverse, pad.mono, semitones, follow)
    }

    /// Seconds of an audio clip's file to a second of the timeline.
    fn clip_speed(&self, clip: &AudioClip) -> f64 {
        clip.source_bpm.map_or(1.0, |bpm| self.project.session.tempo / bpm)
    }

    /// The second of the timeline an audio clip's beat is on, before any
    /// rounding to frames.
    fn clip_beat(&self, clip: &AudioClip) -> f64 {
        let beats = num_traits::ToPrimitive::to_f64(&clip.at_exact()).unwrap_or(0.0);
        beats * 60.0 / self.project.session.tempo
    }

    /// How many seconds before its beat an audio clip starts: its lead, and no
    /// earlier than its file's start or the song's.
    fn clip_lead(&self, clip: &AudioClip) -> f64 {
        let in_file = clip.source_start_seconds / self.clip_speed(clip);
        (clip.lead_ms / 1000.0).min(in_file).min(self.clip_beat(clip))
    }

    /// How many seconds before its end an audio clip starts to leave: the lead
    /// of the clip of `track` that starts where it ends, so the two meet at one
    /// place before the beat, and none when no clip does.
    fn clip_leaves(&self, track: &[AudioClip], clip: &AudioClip) -> f64 {
        let Some(end) = clip.source_end_seconds else { return 0.0 };
        let ends = self.clip_beat(clip) + (end - clip.source_start_seconds) / self.clip_speed(clip);
        track
            .iter()
            .find(|next| !std::ptr::eq(*next, clip) && (self.clip_beat(next) - ends).abs() < 0.0005)
            .map_or(0.0, |next| self.clip_lead(next))
    }

    /// What an audio clip's audio is prepared from: its file from its lead
    /// before its start to where it leaves, `leaves` seconds before its end,
    /// and on through its fade out, as far as the file goes.
    fn clip_job(&mut self, clip: &AudioClip, leaves: f64) -> Result<Job, String> {
        let (_, _, audio) = self.original(&clip.sample)?;
        let length = audio.frames() as f64 / audio.rate as f64;
        let speed = self.clip_speed(clip);
        let start = clip.source_start_seconds - self.clip_lead(clip) * speed;
        let end = match clip.source_end_seconds {
            Some(end) => (end + (clip.fade_out_ms / 1000.0 - leaves) * speed).min(length),
            None => length,
        };
        if start >= end {
            return Err(format!("{}: audio clip outside sample", clip.sample));
        }
        let follow = (clip.source_bpm, clip.stretch);
        self.part(&clip.sample, start, Some(end), false, false, 0.0, follow)
    }

    /// The job for part of a sample: from `start` to `end` seconds of its
    /// file, moved by `semitones`, and following the tempo as `follow` says.
    #[allow(clippy::too_many_arguments)]
    fn part(
        &mut self,
        sample: &str,
        start: f64,
        end: Option<f64>,
        reverse: bool,
        mono: bool,
        semitones: f64,
        follow: (Option<f64>, Stretch),
    ) -> Result<Job, String> {
        let (path, stamp, audio) = self.original(sample)?;
        if !self.originals.contains(&path) {
            self.originals.push(path.clone());
        }
        let mut speed = 2f64.powf(semitones / 12.0);
        let mut stretch = 1.0;
        if let Some(bpm) = follow.0 {
            let ratio = self.project.session.tempo / bpm;
            match follow.1 {
                Stretch::Repitch => speed *= ratio,
                Stretch::PreservePitch => stretch = ratio,
            }
        }
        // Audio that is not stretched is the same whichever stretcher the song names.
        let stretcher = if stretch == 1.0 { Stretcher::Signalsmith } else { self.project.session.stretcher };
        Ok(Job {
            key: PrepKey {
                path,
                stamp,
                start: start.to_bits(),
                end: end.map(f64::to_bits),
                reverse,
                mono,
                speed: speed.to_bits(),
                stretch: stretch.to_bits(),
                stretcher,
                rate: self.project.session.sample_rate,
            },
            audio,
            sample: sample.to_string(),
        })
    }

    pub fn prepare(&mut self, pad: &Pad, event: &Event) -> Result<Arc<Prepared>, String> {
        let job = self.job(pad, event)?;
        self.prepared_for(job)
    }

    fn prepared_for(&mut self, job: Job) -> Result<Arc<Prepared>, String> {
        let g = self.cache.generation;
        if let Some((used, p)) = self.cache.prepared.get_mut(&job.key) {
            *used = g;
            if !self.prepared.contains(&job.key) {
                self.prepared.push(job.key);
            }
            return Ok(p.clone());
        }
        let prepared = Arc::new(job.make()?);
        self.prepared.push(job.key.clone());
        self.cache.prepared.insert(job.key, (g, prepared.clone()));
        Ok(prepared)
    }

    /// `Sampler.voice`: the voice a trigger plays.
    pub fn voice(&mut self, pad: &Pad, tr: &Trigger) -> Result<Voice, String> {
        let audio = self.prepare(pad, &tr.event)?;
        let rate = self.project.session.sample_rate as f64;
        let attack = (pad.attack_ms * rate / 1000.0).round_ties_even() as usize;
        let release = (pad.release_ms * rate / 1000.0).round_ties_even() as usize;
        let gate = tr.cutoff.map(|c| (c - tr.start).max(0) as usize);
        let frames = audio.frames();
        let length = match gate {
            Some(g) => frames.min(g + release),
            None => frames,
        };
        let velocity = (tr.event.velocity as f64 / 127.0 * tr.velocity_scale).min(1.0);
        Ok(Voice {
            start: tr.start,
            pan: pan_gains(audio.channels, pad.pan),
            gain: amplitude(pad.gain_db),
            velocity,
            attack,
            release,
            length,
            audio,
            curve: FadeCurve::Linear,
        })
    }

    /// The voice an audio clip plays: its part of the file, starting its lead
    /// before its beat, fading in over the start and out over the end. `track`
    /// is the audio clips of its track.
    pub fn clip_voice(&mut self, track: &[AudioClip], clip: &AudioClip) -> Result<Voice, String> {
        let leaves = self.clip_leaves(track, clip);
        let job = self.clip_job(clip, leaves)?;
        // Where the first frame of the file it plays belongs on the timeline,
        // rounded once from the exact place of its beat. A clip split in two
        // then plays the same frames at the same places as the whole one.
        let (_, _, file) = self.original(&clip.sample)?;
        let (speed, file_rate) = (self.clip_speed(clip), file.rate as f64);
        let first = ((clip.source_start_seconds - self.clip_lead(clip) * speed) * file_rate).round_ties_even();
        let before = (clip.source_start_seconds * file_rate - first) / file_rate / speed;
        let audio = self.prepared_for(job)?;
        let rate = self.project.session.sample_rate as f64;
        Ok(Voice {
            start: ((self.clip_beat(clip) - before) * rate).round_ties_even() as i64,
            // A clip plays its file as it is: a mono file at full level on both sides.
            pan: [1.0, 1.0],
            gain: amplitude(clip.gain_db),
            velocity: 1.0,
            attack: (clip.fade_in_ms * rate / 1000.0).round_ties_even() as usize,
            release: (clip.fade_out_ms * rate / 1000.0).round_ties_even() as usize,
            length: audio.frames(),
            audio,
            curve: clip.fade_curve,
        })
    }
}

/// Everything a track's voices are made from, as text: its pads, clips and
/// audio clips or its instrument and note clips, the patterns it plays, its
/// samples and their files, the tempo, the rate and the stretcher. A Synth
/// track's voices are none, whatever its patch.
fn voices_key(p: &Project, ti: usize, directory: &Path) -> String {
    use std::fmt::Write;
    let t = &p.tracks[ti];
    let session = &p.session;
    if t.midi.as_ref().is_some_and(|m| m.synth().is_some()) {
        return "synth".to_string();
    }
    let mut key = format!(
        "{:?}|{:?}|{:?}|{:?}|{}|{}|{:?}",
        t.pads,
        t.clips,
        t.audio,
        t.midi,
        session.tempo.to_bits(),
        session.sample_rate,
        session.stretcher
    );
    let mut seen: Vec<&str> = Vec::new();
    for c in &t.clips {
        if !seen.contains(&c.pattern.as_str()) {
            seen.push(&c.pattern);
            let _ = write!(key, "|{}={:?}", c.pattern, p.patterns.get(&c.pattern));
        }
    }
    seen.clear();
    for sample in t.sound_pads().values().map(|pad| &pad.sample).chain(t.audio.iter().map(|clip| &clip.sample)) {
        if !seen.contains(&sample.as_str()) {
            seen.push(sample);
            let asset = p.samples.get(sample);
            let file = asset.map(|a| stamp(&directory.join(&a.path)).ok());
            let _ = write!(key, "|{}={:?}@{:?}", sample, asset, file);
        }
    }
    key
}

/// A track's voices, from the cache when nothing they are made from changed.
/// `key` is the track's `voices_key`.
fn voices(p: &Project, ti: usize, key: String, directory: &Path, cache: &mut Cache) -> Result<Arc<Vec<Voice>>, String> {
    let t = &p.tracks[ti];
    let g = cache.generation;
    if let Some(entry) = cache.tracks.get_mut(&t.id) {
        if entry.key == key {
            entry.generation = g;
            // What the voices play stays prepared for the next edit of the track.
            for k in &entry.prepared {
                if let Some(e) = cache.prepared.get_mut(k) {
                    e.0 = g;
                }
            }
            for path in &entry.originals {
                if let Some(e) = cache.original.get_mut(path) {
                    e.0 = g;
                }
            }
            return Ok(entry.voices.clone());
        }
    }
    let mut sampler = Sampler::new(p, directory, cache);
    let mut voices = Vec::new();
    for tr in track_triggers(p, ti) {
        voices.push(sampler.voice(&t.sound_pads()[&tr.pad], &tr)?);
    }
    if !t.audio.is_empty() {
        for clip in &t.audio {
            voices.push(sampler.clip_voice(&t.audio, clip).map_err(|e| format!("{}: {e}", t.id))?);
        }
        // The renderer takes voices in the order they start.
        voices.sort_by_key(|v| v.start);
    }
    let voices = Arc::new(voices);
    let (prepared, originals) = (sampler.prepared, sampler.originals);
    cache.tracks.insert(
        t.id.clone(),
        TrackEntry {
            generation: g,
            key,
            voices: voices.clone(),
            prepared,
            originals,
        },
    );
    Ok(voices)
}

/// The voice a note played now through the Sampler on track `track` has, for
/// `note.preview`: as the same note in a clip plays it, `beats` long at the
/// session's tempo and starting at frame 0. None when no map entry plays the
/// pitch. What it plays is prepared here, off the audio thread.
pub fn preview_voice(
    p: &Project,
    directory: &Path,
    cache: &mut Cache,
    track: &str,
    pitch: i64,
    velocity: i64,
    beats: f64,
) -> Result<Option<Voice>, String> {
    let (ti, t) = p.tracks.iter().enumerate().find(|(_, t)| t.id == track).ok_or_else(|| format!("Unknown track: {track}"))?;
    let sampler = t.midi.as_ref().and_then(|m| m.sampler()).ok_or_else(|| format!("{track} has no sampler"))?;
    let beats = num_rational::BigRational::from_float(beats).ok_or("a note's length is a number")?;
    let note = NoteOn {
        pitch,
        velocity,
        start: 0,
        end: frame(&beats, p.session.tempo, p.session.sample_rate),
        at: num_rational::BigRational::from_integer(0.into()),
        beats,
    };
    let Some(trigger) = aaw_model::schedule::sampler_hits(sampler, &[note], ti, track).pop() else { return Ok(None) };
    let mut s = Sampler::new(p, directory, cache);
    s.voice(&sampler.pads[&trigger.pad], &trigger).map(Some)
}

/// An owner's lanes as envelopes, by what they drive.
#[derive(Default)]
struct Lanes {
    channel: HashMap<String, Arc<Envelope>>,
    sends: HashMap<String, Arc<Envelope>>,
    /// By effect index, then parameter name.
    effects: HashMap<usize, BTreeMap<String, Arc<Envelope>>>,
    /// A Synth's fields, by their path in the patch.
    instrument: BTreeMap<String, Arc<Envelope>>,
    params: Vec<String>,
}

impl Lanes {
    fn new(owner: Owner, lanes: &[Lane], p: &Project) -> Lanes {
        let mut out = Lanes::default();
        for lane in lanes {
            out.params.push(lane.param.clone());
            let Ok(t) = target(owner, &lane.param) else { continue };
            let env = Arc::new(Envelope::new(lane, t.domain, p.session.tempo, p.session.sample_rate));
            match &t.kind {
                TargetKind::Channel => {
                    out.channel.insert(t.field.clone(), env);
                }
                TargetKind::Send(to) => {
                    out.sends.insert(to.clone(), env);
                }
                TargetKind::Effect { index, .. } => {
                    out.effects.entry(*index).or_default().insert(t.name(), env);
                }
                TargetKind::Instrument => {
                    out.instrument.insert(t.field.clone(), env);
                }
            }
        }
        out
    }

    fn channel(&self, field: &str, value: f64) -> Param {
        Param::new(value, self.channel.get(field))
    }
}

/// A chain without its sidechain routing, which needs every chain's latency.
/// `lanes(i)` gives the lanes on the chain's effect `i` by field; `upstream`
/// is the latency of what comes before the chain, counted into its own.
fn chain(
    effects: &[Effect],
    mut lanes: impl FnMut(usize) -> BTreeMap<String, Arc<Envelope>>,
    p: &Project,
    kernels: &mut Kernels,
    upstream: usize,
) -> ChainProgram {
    let mut out = ChainProgram {
        latency: upstream,
        ..ChainProgram::default()
    };
    for (i, effect) in effects.iter().enumerate() {
        if effect.bypass() {
            out.effects.push((effect.kind(), None));
            continue;
        }
        let plan = Plan::new(effect, lanes(i), p.session.sample_rate, p.session.tempo, kernels);
        out.effects.push((effect.kind(), Some(out.devices.len())));
        let upstream = out.latency;
        out.latency += plan.latency;
        out.devices.push(DeviceProgram {
            plan,
            upstream,
            key: None,
        });
    }
    out
}

/// The lanes on a patch's effect `i`, by field, out of the track's lanes
/// on `instrument.effects.i.FIELD`.
fn patch_effect_lanes(instrument: &BTreeMap<String, Arc<Envelope>>, i: usize) -> BTreeMap<String, Arc<Envelope>> {
    let prefix = format!("effects.{i}.");
    instrument
        .iter()
        .filter_map(|(path, env)| path.strip_prefix(&prefix).map(|field| (field.to_string(), env.clone())))
        .collect()
}

/// Routes a chain's sidechains: `offset(source)` is how far the source's
/// output trails the audio entering this chain.
fn route(chain: &mut ChainProgram, tracks: &[TrackProgram], lead: impl Fn(&TrackProgram) -> usize) {
    for d in &mut chain.devices {
        let Some(name) = d.plan.effect.sidechain() else { continue };
        if let Some(source) = tracks.iter().position(|t| t.id == name) {
            d.key = Some(Key {
                source,
                delay: lead(&tracks[source]) + d.upstream,
            });
        }
    }
}

/// What to compile: the song, or one track or return as its stem, with only
/// the tracks that key or feed it.
#[derive(Clone, Copy, Debug)]
pub enum Scope<'a> {
    Song,
    Channel(&'a str),
}

/// Compiles a project whose samples live under `directory`.
pub fn compile(p: &Project, directory: &Path) -> Result<Program, String> {
    compile_cached(p, directory, &mut Cache::default())
}

/// `compile`, reusing and then pruning what earlier compiles made.
pub fn compile_cached(p: &Project, directory: &Path, cache: &mut Cache) -> Result<Program, String> {
    compile_scoped(p, directory, cache, Scope::Song)
}

pub fn compile_scoped(p: &Project, directory: &Path, cache: &mut Cache, scope: Scope) -> Result<Program, String> {
    let rate = p.session.sample_rate;
    let total = frame(&p.session.length_exact(), p.session.tempo, rate).max(0) as usize;
    // A preview needs the target's sidechain sources; a return's also needs
    // its senders and theirs. Only the target is heard.
    let is_return = |id: &str| p.returns.iter().any(|r| r.id == id);
    let (needed, heard, returns): (Vec<String>, Vec<&str>, Vec<&aaw_model::Return>) = match scope {
        Scope::Song => (
            p.tracks.iter().map(|t| t.id.clone()).collect(),
            p.tracks.iter().map(|t| t.id.as_str()).collect(),
            p.returns.iter().collect(),
        ),
        Scope::Channel(id) if is_return(id) => {
            let mut needed: Vec<String> = p.senders(id).into_iter().map(str::to_string).collect();
            for sender in p.senders(id) {
                needed.extend(p.sidechain_sources(sender));
            }
            needed.extend(p.sidechain_sources(id));
            (needed, Vec::new(), p.returns.iter().filter(|r| r.id == id).collect())
        }
        Scope::Channel(id) if p.track(id).is_some() => {
            let mut needed = p.sidechain_sources(id);
            needed.push(id.to_string());
            (needed, vec![id], Vec::new())
        }
        Scope::Channel(id) => return Err(format!("Unknown track or return: {id}")),
    };
    cache.begin();
    let any_solo = p.tracks.iter().any(|t| t.solo);
    let index: HashMap<&str, usize> = p.tracks.iter().enumerate().map(|(i, t)| (t.id.as_str(), i)).collect();
    let order: Vec<&aaw_model::Track> = p.render_order().into_iter().filter(|t| needed.contains(&t.id)).collect();
    let mut keys: HashMap<&str, String> = order
        .iter()
        .map(|t| (t.id.as_str(), voices_key(p, index[t.id.as_str()], directory)))
        .collect();
    let stale: Vec<usize> = order
        .iter()
        .filter(|t| cache.tracks.get(&t.id).is_none_or(|e| e.key != keys[t.id.as_str()]))
        .map(|t| index[t.id.as_str()])
        .collect();
    prepare_ahead(p, directory, cache, &stale);
    let mut tracks: Vec<TrackProgram> = Vec::new();
    for t in order {
        let key = keys.remove(t.id.as_str()).expect("a key for each track");
        let voices_id = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            (&key, total).hash(&mut h);
            h.finish()
        };
        let voices = voices(p, index[t.id.as_str()], key, directory, cache)?;
        let mut lanes = Lanes::new(Owner::Track(t), &t.automation, p);
        let mut synth = None;
        if let Some((midi, spec)) = t.midi.as_ref().and_then(|m| m.synth().map(|s| (m, s))) {
            // Each wavetable oscillator's table: built in, or a sample's cycle.
            let mut tables = HashMap::new();
            for (id, o) in &spec.oscillators {
                if o.wave != aaw_model::Wave::Wavetable {
                    continue;
                }
                let table = match Wavetable::builtin(&o.table) {
                    Some(t) => t,
                    None => cache.wavetable(p, directory, &o.table).map_err(|e| format!("{}: oscillator {id}: {e}", t.id))?,
                };
                tables.insert(id.clone(), table);
            }
            synth = Some(SynthProgram {
                notes: cache.synth_notes(p, t, midi),
                patch: Arc::new(Patch::new(spec, &lanes.instrument, rate as f64, p.session.tempo, track_seed(&t.id), &tables)),
                chain: chain(&spec.effects, |i| patch_effect_lanes(&lanes.instrument, i), p, &mut cache.kernels, 0),
            });
        }
        // The inserts follow the patch's effects, so their latency counts
        // the patch chain's.
        let patch_latency = synth.as_ref().map_or(0, |s| s.chain.latency);
        let mut effect_lanes = std::mem::take(&mut lanes.effects);
        let mut chain = chain(&t.effects, |i| effect_lanes.remove(&i).unwrap_or_default(), p, &mut cache.kernels, patch_latency);
        // The track waits for the slowest of its keys.
        let delay = chain
            .devices
            .iter()
            .filter_map(|d| d.plan.effect.sidechain())
            .filter_map(|name| tracks.iter().find(|s| s.id == name))
            .map(|s| s.delay + s.chain.latency)
            .max()
            .unwrap_or(0);
        route(&mut chain, &tracks, |source| delay - (source.delay + source.chain.latency));
        let sends = returns
            .iter()
            .map(|r| match t.sends.iter().find(|s| s.to == r.id) {
                Some(s) => SendProgram {
                    pre_fader: s.pre_fader,
                    level: Some(Param::new(s.gain_db, lanes.sends.get(&r.id))),
                },
                None => SendProgram {
                    pre_fader: false,
                    level: None,
                },
            })
            .collect();
        tracks.push(TrackProgram {
            id: t.id.clone(),
            voices,
            synth,
            voices_id,
            chain,
            gain: lanes.channel("gain_db", t.gain_db),
            pan: lanes.channel("pan", t.pan),
            silent: t.mute || (any_solo && !t.solo),
            sends,
            delay,
            align: 0,
            in_mix: heard.contains(&t.id.as_str()),
            automation: lanes.params,
        });
    }
    let track_offset = tracks.iter().map(|t| t.delay + t.chain.latency).max().unwrap_or(0);
    for t in &mut tracks {
        t.align = track_offset - (t.delay + t.chain.latency);
    }
    let mut buses: Vec<ReturnProgram> = Vec::new();
    for r in returns {
        let mut lanes = Lanes::new(Owner::Return(r), &r.automation, p);
        let mut effect_lanes = std::mem::take(&mut lanes.effects);
        let mut chain = chain(&r.effects, |i| effect_lanes.remove(&i).unwrap_or_default(), p, &mut cache.kernels, 0);
        route(&mut chain, &tracks, |source| track_offset - (source.delay + source.chain.latency));
        buses.push(ReturnProgram {
            id: r.id.clone(),
            chain,
            gain: lanes.channel("gain_db", r.gain_db),
            pan: lanes.channel("pan", r.pan),
            mute: r.mute,
            align: 0,
            senders: p.senders(&r.id).into_iter().map(str::to_string).collect(),
            automation: lanes.params,
        });
    }
    let return_latency = buses.iter().map(|r| r.chain.latency).max().unwrap_or(0);
    for r in &mut buses {
        r.align = return_latency - r.chain.latency;
    }
    // Track and return previews are stems, so they omit the master chain.
    let mut lanes = Lanes::new(Owner::Master(&p.master), &p.master.automation, p);
    let master_effects: &[Effect] = if matches!(scope, Scope::Song) { &p.master.effects } else { &[] };
    let mut effect_lanes = std::mem::take(&mut lanes.effects);
    let master = MasterProgram {
        gain: lanes.channel("gain_db", p.session.master_gain_db),
        chain: chain(master_effects, |i| effect_lanes.remove(&i).unwrap_or_default(), p, &mut cache.kernels, 0),
        automation: lanes.params,
    };
    cache.finish();
    let latency = track_offset + return_latency + master.chain.latency;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let devices = |chain: &ChainProgram, h: &mut std::collections::hash_map::DefaultHasher| {
        for d in &chain.devices {
            (d.plan.signature, d.upstream, d.key.as_ref().map(|k| (k.source, k.delay))).hash(h);
        }
        chain.devices.len().hash(h);
    };
    (rate, track_offset, return_latency, latency).hash(&mut h);
    for t in &tracks {
        (&t.id, t.delay, t.align, t.in_mix, t.synth.as_ref().map(|s| s.patch.signature)).hash(&mut h);
        if let Some(s) = &t.synth {
            devices(&s.chain, &mut h);
        }
        devices(&t.chain, &mut h);
    }
    for r in &buses {
        (&r.id, r.align).hash(&mut h);
        devices(&r.chain, &mut h);
    }
    devices(&master.chain, &mut h);
    Ok(Program {
        rate: rate as u32,
        tempo: p.session.tempo,
        total,
        tracks,
        returns: buses,
        master,
        fade: total.min((p.session.end_fade_ms * rate as f64 / 1000.0).round_ties_even() as usize),
        track_offset,
        return_latency,
        latency,
        structure: h.finish(),
    })
}
